//! Game analysis logic for evaluating chess games using UCI engines.
//!
//! This module provides the `GameAnalysisService` struct, which exposes methods to analyze chess games move-by-move using a UCI-compatible engine.
//! It integrates with the database for novelty detection and annotates sacrifices, supporting progress reporting for UI updates.

use std::path::PathBuf;
use std::time::Duration;

use shakmaty::{fen::Fen, uci::UciMove, CastlingMode, Chess, EnPassantMode, Position};
use tokio::sync::watch;

use crate::db::{is_position_in_db, GameQueryJs, PositionQueryJs};
use crate::error::Error;
use crate::AppState;

use super::evaluation::naive_eval;
use super::process::{EngineProcess, LineEvent};
use super::registry::{EngineLines, KILL_GRACE};
use super::types::{AnalysisOptions, EngineOption, GoMode, MoveAnalysis, ReportProgress};
use tauri_specta::Event;

/// A position to analyze: its FEN, the moves leading to it, and whether the last move was a sacrifice.
pub(crate) type GamePosition = (Fen, Vec<String>, bool);

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
        // Read engine output and keep the last complete depth for this position.
        while let Ok(Some(line)) = reader.next_line().await {
            match proc.handle_line(&line, || true) {
                LineEvent::Depth { lines, .. } => current_analysis.best = lines,
                LineEvent::BestMove { .. } => break,
                LineEvent::Other => {}
            }
        }
        analysis.push(current_analysis);
    }
    Ok(analysis)
}

/// Run [`analyze_positions`] until it finishes or `closed` turns true (the registry ended the
/// review's engine slot). On cancel the
/// engine process is killed at once and `Error::AnalysisCancelled` is returned.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn analyze_until_cancelled(
    proc: &mut EngineProcess,
    reader: &mut EngineLines,
    options: &AnalysisOptions,
    positions: &[GamePosition],
    go_mode: &GoMode,
    uci_options: &[EngineOption],
    mut closed: watch::Receiver<bool>,
    on_progress: impl FnMut(f64) -> Result<(), Error>,
) -> Result<Vec<MoveAnalysis>, Error> {
    let finished = tokio::select! {
        result = analyze_positions(proc, reader, options, positions, go_mode, uci_options, on_progress) => Some(result),
        _ = closed.wait_for(|closed| *closed) => None,
    };
    match finished {
        Some(result) => result,
        None => {
            // No "quit" handshake: a search in progress must end now.
            proc.shutdown(Duration::ZERO).await?;
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
    /// * `owner` - Tab whose closing cancels the review.
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
        owner: String,
        state: tauri::State<'_, AppState>,
        app: tauri::AppHandle,
    ) -> Result<Vec<MoveAnalysis>, Error> {
        // Validate the moves before spawning an engine.
        let mut fens = game_positions(&options)?;
        let key = (id.clone(), engine.clone());
        let started = state
            .engines
            .get_or_start(key.clone(), owner, PathBuf::from(&engine))
            .await?;
        let mut reader = started.reader.ok_or_else(|| {
            Error::EngineInitFailed(format!("A review with id {id} is already running"))
        })?;

        if options.reversed {
            fens.reverse();
        }

        let mut novelty_found = false;

        let result = {
            let mut proc = started.process.lock().await;
            analyze_until_cancelled(
                &mut proc,
                &mut reader,
                &options,
                &fens,
                &go_mode,
                &uci_options,
                started.slot.closed(),
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
            .await
        };
        // The review is over, however it ended: end its engine.
        state.engines.remove(&key, &started.slot);
        started.process.lock().await.shutdown(KILL_GRACE).await.ok();
        let mut analysis = result?;

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

    /// An engine that answers the handshake and then searches "forever",
    /// ignoring stdin, like an engine deep in a long search.
    #[cfg(unix)]
    fn busy_engine(dir: &std::path::Path) -> PathBuf {
        super::super::test_support::engine(dir, "busy-engine.sh", "0")
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn closing_the_tab_ends_a_running_review_and_its_engine_within_a_second() {
        let dir = tempfile::tempdir().unwrap();
        let registry = super::super::registry::EngineRegistry::default();
        let key = ("report_tab1".to_string(), "busy".to_string());
        let started = registry
            .get_or_start(key, "tab1".to_string(), busy_engine(dir.path()))
            .await
            .unwrap();
        let mut reader = started.reader.unwrap();
        let positions = game_positions(&options(&["e2e4"])).unwrap();

        let closer = {
            let registry = registry.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(200)).await;
                registry.kill_owner("tab1").await
            })
        };

        let started_at = Instant::now();
        let result = {
            let mut proc = started.process.lock().await;
            analyze_until_cancelled(
                &mut proc,
                &mut reader,
                &options(&["e2e4"]),
                &positions,
                &super::super::types::GoMode::Depth(20),
                &[],
                started.slot.closed(),
                |_| Ok(()),
            )
            .await
        };
        closer.await.unwrap().unwrap();

        assert!(matches!(result, Err(Error::AnalysisCancelled)));
        assert!(started_at.elapsed() < Duration::from_secs(1));
        let pid = super::super::test_support::pids(dir.path())[0];
        assert!(
            super::super::test_support::eventually(2, || !super::super::test_support::alive(pid))
                .await,
            "review engine still alive"
        );
    }
}
