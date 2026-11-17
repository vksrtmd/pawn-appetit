//! Game analysis logic for evaluating chess games using UCI engines.
//!
//! This module provides the `GameAnalysisService` struct, which exposes methods to analyze chess games move-by-move using a UCI-compatible engine.
//! It integrates with the database for novelty detection and annotates sacrifices, supporting progress reporting for UI updates.

use std::path::PathBuf;
use std::sync::Arc;

use dashmap::DashMap;
use shakmaty::{fen::Fen, uci::UciMove, CastlingMode, Chess, EnPassantMode, Position};
use tokio::sync::watch;
use vampirc_uci::parse_one;

use crate::db::{is_position_in_db, GameQueryJs, PositionQueryJs};
use crate::error::Error;
use crate::AppState;

use super::evaluation::naive_eval;
use super::process::{parse_uci_attrs, EngineProcess};
use super::types::{AnalysisOptions, EngineOption, GoMode, MoveAnalysis, ReportProgress};
use tauri_specta::Event;

/// A position to analyze: its FEN, the moves leading to it, and whether the last move was a sacrifice.
pub(crate) type GamePosition = (Fen, Vec<String>, bool);

type EngineLines = tokio::io::Lines<tokio::io::BufReader<tokio::process::ChildStdout>>;

/// Cancel switches of the running game reviews, keyed by analysis id.
#[derive(Clone, Default)]
pub struct AnalysisCancelRegistry {
    senders: Arc<DashMap<String, watch::Sender<bool>>>,
}

/// Keeps a review registered for cancellation; unregisters it on drop.
pub struct AnalysisCancelGuard {
    id: String,
    senders: Arc<DashMap<String, watch::Sender<bool>>>,
    receiver: watch::Receiver<bool>,
}

impl AnalysisCancelGuard {
    pub fn receiver(&self) -> watch::Receiver<bool> {
        self.receiver.clone()
    }
}

impl Drop for AnalysisCancelGuard {
    fn drop(&mut self) {
        self.senders.remove(&self.id);
    }
}

impl AnalysisCancelRegistry {
    /// Make the review `id` cancellable until the returned guard is dropped.
    pub fn register(&self, id: &str) -> AnalysisCancelGuard {
        let (sender, receiver) = watch::channel(false);
        self.senders.insert(id.to_string(), sender);
        AnalysisCancelGuard {
            id: id.to_string(),
            senders: self.senders.clone(),
            receiver,
        }
    }

    /// Ask the review `id` to stop. Returns whether such a review was running.
    pub fn cancel(&self, id: &str) -> bool {
        self.senders
            .get(id)
            .map(|sender| sender.send(true).is_ok())
            .unwrap_or(false)
    }
}

/// Build the list of positions to analyze (the start position plus one per move,
/// stopping at game over), tracking sacrifices.
///
/// # Errors
/// Returns `Error` if the FEN or a move does not parse, or a move is illegal.
pub(crate) fn game_positions(options: &AnalysisOptions) -> Result<Vec<GamePosition>, Error> {
    let fen = Fen::from_ascii(options.fen.as_bytes())?;
    let mut chess: Chess = fen.clone().into_position(CastlingMode::Chess960)?;
    let mut fens: Vec<GamePosition> = vec![(fen, vec![], false)];

    for (i, m) in options.moves.iter().enumerate() {
        let uci = UciMove::from_ascii(m.as_bytes())?;
        let m = uci.to_move(&chess)?;
        let previous_pos = chess.clone();
        chess.play_unchecked(&m);
        let current_pos = chess.clone();
        if !chess.is_game_over() {
            // Detect sacrifices by comparing naive evals before and after the move.
            let prev_eval = naive_eval(&previous_pos);
            let cur_eval = -naive_eval(&current_pos);
            fens.push((
                Fen::from_position(current_pos, EnPassantMode::Legal),
                options.moves.iter().take(i + 1).cloned().collect(),
                prev_eval > cur_eval + 100, // Mark as sacrifice if eval drops by > 100.
            ));
        }
    }
    Ok(fens)
}

/// Analyze every position with the engine, reporting progress (0-100) before each one.
async fn analyze_positions(
    proc: &mut EngineProcess,
    reader: &mut EngineLines,
    options: &AnalysisOptions,
    positions: &[GamePosition],
    go_mode: &GoMode,
    uci_options: &[EngineOption],
    mut on_progress: impl FnMut(f64) -> Result<(), Error>,
) -> Result<Vec<MoveAnalysis>, Error> {
    let mut analysis: Vec<MoveAnalysis> = Vec::new();

    for (i, (_, moves, _)) in positions.iter().enumerate() {
        on_progress((i as f64 / positions.len() as f64) * 100.0)?;

        // Ensure MultiPV=2 for principal variation analysis.
        let mut extra_options = uci_options.to_vec();
        if !extra_options.iter().any(|x| x.name == "MultiPV") {
            extra_options.push(EngineOption {
                name: "MultiPV".to_string(),
                value: "2".to_string(),
            });
        } else {
            extra_options.iter_mut().for_each(|x| {
                if x.name == "MultiPV" {
                    x.value = "2".to_string();
                }
            });
        }

        proc.set_options(super::types::EngineOptions {
            fen: options.fen.clone(),
            moves: moves.clone(),
            extra_options,
        })
        .await?;
        proc.go(go_mode).await?;

        let mut current_analysis = MoveAnalysis::default();
        // Read engine output and parse best moves for this position.
        while let Ok(Some(line)) = reader.next_line().await {
            match parse_one(&line) {
                vampirc_uci::UciMessage::Info(attrs) => {
                    if let Ok(best_moves) =
                        parse_uci_attrs(attrs, &proc.options.fen.parse()?, moves)
                    {
                        let multipv = best_moves.multipv;
                        let cur_depth = best_moves.depth;
                        if multipv as usize == proc.best_moves.len() + 1 {
                            proc.best_moves.push(best_moves);
                            if multipv == proc.real_multipv {
                                if proc.best_moves.iter().all(|x| x.depth == cur_depth)
                                    && cur_depth >= proc.last_depth
                                {
                                    current_analysis.best = proc.best_moves.clone();
                                    proc.last_depth = cur_depth;
                                }
                                assert_eq!(proc.best_moves.len(), proc.real_multipv as usize);
                                proc.best_moves.clear();
                            }
                        }
                    }
                }
                vampirc_uci::UciMessage::BestMove { .. } => {
                    break;
                }
                _ => {}
            }
        }
        analysis.push(current_analysis);
    }
    Ok(analysis)
}

/// Run [`analyze_positions`] until it finishes or `cancel` turns true. On cancel the
/// engine process is killed at once and `Error::AnalysisCancelled` is returned.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn analyze_until_cancelled(
    proc: &mut EngineProcess,
    reader: &mut EngineLines,
    options: &AnalysisOptions,
    positions: &[GamePosition],
    go_mode: &GoMode,
    uci_options: &[EngineOption],
    mut cancel: watch::Receiver<bool>,
    on_progress: impl FnMut(f64) -> Result<(), Error>,
) -> Result<Vec<MoveAnalysis>, Error> {
    let finished = tokio::select! {
        result = analyze_positions(proc, reader, options, positions, go_mode, uci_options, on_progress) => Some(result),
        _ = cancel.wait_for(|cancelled| *cancelled) => None,
    };
    match finished {
        Some(result) => result,
        None => {
            // No "quit" handshake: a search in progress must end now.
            proc.child.kill().await?;
            Err(Error::AnalysisCancelled)
        }
    }
}

/// Service for analyzing chess games using a UCI engine.
pub struct GameAnalysisService;

impl GameAnalysisService {
    /// Analyze a chess game move-by-move using a UCI engine.
    ///
    /// # Arguments
    /// * `id` - Unique analysis session identifier; `cancel_analysis` with this id stops the review.
    /// * `engine` - Path to the UCI engine binary.
    /// * `go_mode` - Engine search mode (depth, time, etc).
    /// * `options` - Analysis options (FEN, moves, etc).
    /// * `uci_options` - Extra UCI engine options.
    /// * `state` - Application state for DB and engine process management.
    /// * `app` - Tauri app handle for event emission.
    ///
    /// # Returns
    /// Vector of `MoveAnalysis` for each position in the game.
    ///
    /// # Errors
    /// Returns `Error` if a move is illegal, engine or DB operations fail, or the
    /// review is cancelled (`Error::AnalysisCancelled`).
    pub async fn analyze_game(
        id: String,
        engine: String,
        go_mode: GoMode,
        options: AnalysisOptions,
        uci_options: Vec<EngineOption>,
        state: tauri::State<'_, AppState>,
        app: tauri::AppHandle,
    ) -> Result<Vec<MoveAnalysis>, Error> {
        // Validate the moves before spawning an engine.
        let mut fens = game_positions(&options)?;
        let cancel = state.analysis_cancel.register(&id);

        let (mut proc, mut reader) = EngineProcess::new(PathBuf::from(&engine)).await?;

        if options.reversed {
            fens.reverse();
        }

        let mut novelty_found = false;

        let mut analysis = analyze_until_cancelled(
            &mut proc,
            &mut reader,
            &options,
            &fens,
            &go_mode,
            &uci_options,
            cancel.receiver(),
            |progress| {
                ReportProgress {
                    progress,
                    id: id.clone(),
                    finished: false,
                }
                .emit(&app)?;
                Ok(())
            },
        )
        .await?;

        if options.reversed {
            analysis.reverse();
            fens.reverse();
        }

        // Annotate sacrifices and novelties for each analyzed position.
        for (i, analysis) in analysis.iter_mut().enumerate() {
            let fen = &fens[i].0;
            let query = PositionQueryJs {
                fen: fen.to_string(),
                type_: "exact".to_string(),
                forbidden_squares: None,
                exact_pieces: None,
            };

            analysis.is_sacrifice = fens[i].2;
            if options.annotate_novelties && !novelty_found {
                if let Some(reference) = options.reference_db.clone() {
                    analysis.novelty = !is_position_in_db(
                        reference,
                        GameQueryJs::new().position(query.clone()).clone(),
                        state.clone(),
                    )
                    .await?;
                    if analysis.novelty {
                        novelty_found = true;
                    }
                } else {
                    return Err(Error::MissingReferenceDatabase);
                }
            }
        }

        ReportProgress {
            progress: 100.0,
            id: id.clone(),
            finished: true,
        }
        .emit(&app)?;
        Ok(analysis)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    fn options(moves: &[&str]) -> AnalysisOptions {
        AnalysisOptions {
            fen: START_FEN.to_string(),
            moves: moves.iter().map(|m| m.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn illegal_move_is_an_error_not_a_panic() {
        // e2e5 is not a legal pawn move.
        assert!(matches!(
            game_positions(&options(&["e2e4", "e7e5", "e2e5"])),
            Err(Error::IllegalUciMove(_))
        ));
    }

    #[test]
    fn unparsable_move_is_an_error_not_a_panic() {
        assert!(matches!(
            game_positions(&options(&["zzzz"])),
            Err(Error::ParseUciMove(_))
        ));
    }

    #[test]
    fn legal_moves_give_one_position_per_ply_plus_the_start() {
        let positions = game_positions(&options(&["e2e4", "e7e5"])).unwrap();
        assert_eq!(positions.len(), 3);
        assert_eq!(positions[2].1, vec!["e2e4", "e7e5"]);
    }

    #[test]
    fn cancelling_an_unknown_analysis_reports_nothing_to_cancel() {
        let registry = AnalysisCancelRegistry::default();
        assert!(!registry.cancel("report_tab1"));
    }

    #[test]
    fn a_registered_analysis_sees_the_cancel_request() {
        let registry = AnalysisCancelRegistry::default();
        let guard = registry.register("report_tab1");
        assert!(registry.cancel("report_tab1"));
        assert!(*guard.receiver().borrow());
    }

    #[test]
    fn dropping_the_guard_unregisters_the_analysis() {
        let registry = AnalysisCancelRegistry::default();
        drop(registry.register("report_tab1"));
        assert!(!registry.cancel("report_tab1"));
    }

    /// An engine that answers the handshake and then searches "forever",
    /// ignoring stdin, like an engine deep in a long search.
    #[cfg(unix)]
    fn busy_engine(dir: &std::path::Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("busy-engine.sh");
        std::fs::write(
            &path,
            "#!/bin/sh\n\
             while read line; do\n\
               case \"$line\" in\n\
                 uci) echo uciok;;\n\
                 isready) echo readyok;;\n\
                 go*) sleep 30;;\n\
               esac\n\
             done\n",
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancel_ends_a_running_review_and_its_engine_within_a_second() {
        let dir = tempfile::tempdir().unwrap();
        let (mut proc, mut reader) = EngineProcess::new(busy_engine(dir.path())).await.unwrap();
        let pid = proc.child.id().unwrap();
        let positions = game_positions(&options(&["e2e4"])).unwrap();
        let registry = AnalysisCancelRegistry::default();
        let guard = registry.register("report_tab1");

        let canceller = {
            let registry = registry.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(200)).await;
                registry.cancel("report_tab1");
            })
        };

        let started = Instant::now();
        let result = analyze_until_cancelled(
            &mut proc,
            &mut reader,
            &options(&["e2e4"]),
            &positions,
            &super::super::types::GoMode::Depth(20),
            &[],
            guard.receiver(),
            |_| Ok(()),
        )
        .await;
        canceller.await.unwrap();

        assert!(matches!(result, Err(Error::AnalysisCancelled)));
        assert!(started.elapsed() < Duration::from_secs(1));
        // The engine process is gone: signal 0 fails for a reaped pid.
        assert!(
            proc.child.try_wait().unwrap().is_some(),
            "pid {pid} still running"
        );
    }
}
