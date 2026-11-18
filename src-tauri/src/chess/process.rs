//! UCI engine process abstraction and communication utilities.
//!
//! This module provides the `EngineProcess` struct for managing a UCI chess engine process,
//! sending commands, updating options, and parsing engine output for best-move analysis.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use tokio::io::AsyncWriteExt;
use vampirc_uci::{uci::ScoreValue, UciInfoAttribute};

use crate::error::Error;

use super::session::EngineLifecycle;
use super::types::{BestMoves, EngineLog, EngineOptions, GoMode};
use super::uci::{UciCommunicator, UCI_TIMEOUT};
use shakmaty::{fen::Fen, san::SanPlus, uci::UciMove, CastlingMode, Chess, Color, Position};

#[cfg(target_os = "windows")]
pub const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Most recent engine log lines kept per process.
pub const MAX_LOG_LINES: usize = 2000;

/// An engine's log: the most recent [`MAX_LOG_LINES`] lines.
#[derive(Default)]
pub struct LogBuffer(VecDeque<EngineLog>);

impl LogBuffer {
    pub fn push(&mut self, line: EngineLog) {
        if self.0.len() == MAX_LOG_LINES {
            self.0.pop_front();
        }
        self.0.push_back(line);
    }

    pub fn to_vec(&self) -> Vec<EngineLog> {
        self.0.iter().cloned().collect()
    }
}

/// What one engine output line meant to the search in progress.
pub enum LineEvent {
    /// Every `MultiPV` line of a depth arrived. `lines` are the best lines of that depth;
    /// `progress` is 0-100 for the current go mode.
    Depth {
        progress: f64,
        lines: Vec<BestMoves>,
    },
    /// The engine sent `bestmove`; `lines` are the last complete depth.
    BestMove { lines: Vec<BestMoves> },
    /// Anything else (or a partial depth, or one held back by the rate limit).
    Other,
}

/// Represents a running UCI engine process and its state.
pub struct EngineProcess {
    pub lifecycle: EngineLifecycle,
    pub child: tokio::process::Child,
    pub stdin: tokio::process::ChildStdin,
    pub last_depth: u32,
    pub best_moves: Vec<BestMoves>,
    pub last_best_moves: Vec<BestMoves>,
    pub last_progress: f32,
    pub options: EngineOptions,
    pub go_mode: GoMode,
    pub running: bool,
    pub real_multipv: u16,
    pub logs: LogBuffer,
    pub start: Instant,
}

impl EngineProcess {
    /// Spawn a new UCI engine process and initialize it.
    ///
    /// Returns the process and a line reader for its stdout.
    ///
    /// # Errors
    /// Returns `Error::EngineTimeout` if engine doesn't respond within 10 seconds.
    pub async fn new(
        path: PathBuf,
    ) -> Result<
        (
            Self,
            tokio::io::Lines<tokio::io::BufReader<tokio::process::ChildStdout>>,
        ),
        Error,
    > {
        let mut lifecycle = EngineLifecycle::new();
        lifecycle.mark_starting();

        let mut comm = UciCommunicator::spawn(path).await?;
        let mut logs = LogBuffer::default();

        logs.push(EngineLog::Gui("uci\n".to_string()));
        comm.send_and_wait("uci\n", "uciok", UCI_TIMEOUT, |line| {
            logs.push(EngineLog::Engine(line.to_string()))
        })
        .await?;
        logs.push(EngineLog::Gui("isready\n".to_string()));
        comm.send_and_wait("isready\n", "readyok", Duration::from_secs(5), |line| {
            logs.push(EngineLog::Engine(line.to_string()))
        })
        .await?;

        lifecycle.mark_ready();

        Ok((
            Self {
                lifecycle,
                child: comm.child,
                stdin: comm.stdin,
                last_depth: 0,
                best_moves: Vec::new(),
                last_best_moves: Vec::new(),
                last_progress: 0.0,
                logs,
                options: EngineOptions::default(),
                real_multipv: 0,
                go_mode: GoMode::Infinite,
                running: false,
                start: Instant::now(),
            },
            comm.stdout_lines,
        ))
    }

    /// Set a single UCI option for the engine.
    pub async fn set_option<T>(&mut self, name: &str, value: T) -> Result<(), Error>
    where
        T: std::fmt::Display,
    {
        let msg = format!("setoption name {} value {}\n", name, value);
        self.stdin.write_all(msg.as_bytes()).await?;
        self.logs.push(EngineLog::Gui(msg));
        Ok(())
    }

    /// Set all engine options, including FEN, moves, and extra UCI options.
    /// Updates multipv and resets best-move tracking.
    pub async fn set_options(&mut self, options: EngineOptions) -> Result<(), Error> {
        let fen: Fen = options.fen.parse()?;
        let mut pos: Chess = match fen.into_position(CastlingMode::Chess960) {
            Ok(p) => p,
            Err(e) => e.ignore_too_much_material()?,
        };
        for m in &options.moves {
            let uci = UciMove::from_ascii(m.as_bytes())?;
            let mv = uci.to_move(&pos)?;
            pos.play_unchecked(&mv);
        }
        let multipv = options
            .extra_options
            .iter()
            .find(|x| x.name == "MultiPV")
            .map(|x| x.value.parse().unwrap_or(1))
            .unwrap_or(1);

        self.real_multipv = multipv.min(pos.legal_moves().len() as u16);

        for option in &options.extra_options {
            if !self.options.extra_options.contains(option) {
                self.set_option(&option.name, &option.value).await?;
            }
        }

        if options.fen != self.options.fen || options.moves != self.options.moves {
            self.set_position(&options.fen, &options.moves).await?;
        }
        self.last_depth = 0;
        self.options = options.clone();
        self.best_moves.clear();
        self.last_best_moves.clear();
        self.last_progress = 0.0;
        Ok(())
    }

    /// The one parser of engine output: log `line`, and when it is an `info` line or `bestmove`
    /// update the search state and say what happened. `may_emit` is asked once a depth
    /// completes; when it answers false the depth is cached but reported as `Other`.
    pub fn handle_line(&mut self, line: &str, may_emit: impl FnOnce() -> bool) -> LineEvent {
        let event = match vampirc_uci::parse_one(line) {
            vampirc_uci::UciMessage::Info(attrs) => match self.options.fen.parse::<Fen>() {
                Ok(fen) => best_moves_from_info(attrs, &fen, &self.options.moves)
                    .and_then(|best| self.record_info(best, may_emit))
                    .map_or(LineEvent::Other, |(progress, lines)| LineEvent::Depth {
                        progress,
                        lines,
                    }),
                Err(e) => {
                    log::error!(
                        "Failed to parse FEN in engine output: {} - FEN: {}",
                        e,
                        self.options.fen
                    );
                    LineEvent::Other
                }
            },
            vampirc_uci::UciMessage::BestMove { .. } => {
                self.last_progress = 100.0;
                self.running = false;
                self.lifecycle.mark_ready();
                LineEvent::BestMove {
                    lines: self.last_best_moves.clone(),
                }
            }
            _ => LineEvent::Other,
        };
        self.logs.push(EngineLog::Engine(line.to_string()));
        event
    }

    /// Record one parsed `info` line. When it completes a depth for all `MultiPV` lines, the
    /// cached state (used by the final `bestmove` payload) is always updated; the lines are
    /// returned for emission only when `may_emit` allows (the event rate limit).
    pub fn record_info(
        &mut self,
        best: BestMoves,
        may_emit: impl FnOnce() -> bool,
    ) -> Option<(f64, Vec<BestMoves>)> {
        let multipv = best.multipv;
        let cur_depth = best.depth;
        let cur_nodes = best.nodes;
        if multipv as usize != self.best_moves.len() + 1 {
            return None;
        }
        self.best_moves.push(best);
        if multipv != self.real_multipv {
            return None;
        }
        let mut emit = None;
        if self.best_moves.iter().all(|x| x.depth == cur_depth) && cur_depth >= self.last_depth {
            let progress = match self.go_mode {
                GoMode::Depth(depth) => (cur_depth as f64 / depth as f64) * 100.0,
                GoMode::Time(time) => {
                    (self.start.elapsed().as_millis() as f64 / time as f64) * 100.0
                }
                GoMode::Nodes(nodes) => (cur_nodes as f64 / nodes as f64) * 100.0,
                GoMode::PlayersTime(_) | GoMode::Infinite => 99.99,
            };
            self.last_depth = cur_depth;
            self.last_best_moves = self.best_moves.clone();
            self.last_progress = progress as f32;
            if may_emit() {
                emit = Some((progress, self.best_moves.clone()));
            }
        }
        self.best_moves.clear();
        emit
    }

    /// Set the engine's position using FEN and move list.
    pub async fn set_position(&mut self, fen: &str, moves: &[String]) -> Result<(), Error> {
        let msg = if moves.is_empty() {
            format!("position fen {}\n", fen)
        } else {
            format!("position fen {} moves {}\n", fen, moves.join(" "))
        };
        self.stdin.write_all(msg.as_bytes()).await?;
        self.options.fen = fen.to_string();
        self.options.moves = moves.to_vec();
        self.logs.push(EngineLog::Gui(msg));
        Ok(())
    }

    /// Start engine search with the given mode (depth, time, etc).
    pub async fn go(&mut self, mode: &GoMode) -> Result<(), Error> {
        self.go_mode = mode.clone();
        let msg = go_command(mode);
        self.lifecycle.start_search();
        if let Err(e) = self.stdin.write_all(msg.as_bytes()).await {
            self.lifecycle.mark_failed(e.to_string());
            return Err(e.into());
        }
        self.logs.push(EngineLog::Gui(msg));
        self.running = true;
        self.start = Instant::now();
        Ok(())
    }

    /// Stop the engine's current search.
    pub async fn stop(&mut self) -> Result<(), Error> {
        if !self.lifecycle.begin_search_stop() {
            return Ok(());
        }
        if let Err(e) = self.stdin.write_all(b"stop\n").await {
            self.lifecycle.mark_failed(e.to_string());
            return Err(e.into());
        }
        self.logs.push(EngineLog::Gui("stop\n".to_string()));
        self.running = false;
        self.lifecycle.mark_ready();
        Ok(())
    }

    /// Shut the engine down: send `quit` and wait up to `grace` for it to exit, then
    /// force-kill. A zero `grace` skips `quit` and kills at once.
    pub async fn shutdown(&mut self, grace: Duration) -> Result<(), Error> {
        use log::warn;

        if !self.lifecycle.begin_shutdown() {
            return Ok(());
        }

        // Try graceful shutdown first
        if !grace.is_zero() {
            if let Err(e) = self.stdin.write_all(b"quit\n").await {
                warn!("Failed to send quit command to engine: {}", e);
            } else {
                self.logs.push(EngineLog::Gui("quit\n".to_string()));
            }
        }

        self.running = false;

        // Wait for process to exit gracefully (2 second timeout)
        let wait_result = tokio::time::timeout(grace, self.child.wait()).await;

        match wait_result {
            Ok(Ok(status)) => {
                log::info!("Engine process exited gracefully with status: {:?}", status);
                self.lifecycle.mark_stopped();
                Ok(())
            }
            Ok(Err(e)) => {
                warn!("Error waiting for engine process: {}", e);
                // Try force kill
                if let Err(kill_error) = self.child.kill().await {
                    self.lifecycle.mark_failed(kill_error.to_string());
                    return Err(kill_error.into());
                }
                log::info!("Engine process force-killed");
                self.lifecycle.mark_stopped();
                Ok(())
            }
            Err(_) => {
                // Timeout - force kill
                warn!("Engine did not exit gracefully, force-killing");
                if let Err(kill_error) = self.child.kill().await {
                    self.lifecycle.mark_failed(kill_error.to_string());
                    return Err(kill_error.into());
                }
                // Wait for kill to complete
                let _ = self.child.wait().await;
                log::info!("Engine process force-killed after timeout");
                self.lifecycle.mark_stopped();
                Ok(())
            }
        }
    }
}

/// Invert a UCI score (for black's perspective).
fn invert_score(score: vampirc_uci::uci::Score) -> vampirc_uci::uci::Score {
    let new_value = match score.value {
        ScoreValue::Cp(x) => ScoreValue::Cp(-x),
        ScoreValue::Mate(x) => ScoreValue::Mate(-x),
    };
    let new_wdl = score.wdl.map(|(w, d, l)| (l, d, w));
    vampirc_uci::uci::Score {
        value: new_value,
        wdl: new_wdl,
        ..score
    }
}

/// Turn an engine `info` line into a best line; partial or unparseable lines yield `None`.
///
/// Lines whose score is a lower or upper bound are partial results and are dropped, so the
/// score and PV do not flicker. Parse failures are logged, except `NoMovesFound` (info lines
/// without a PV, which engines send all the time).
pub fn best_moves_from_info(
    attrs: Vec<UciInfoAttribute>,
    fen: &Fen,
    moves: &Vec<String>,
) -> Option<BestMoves> {
    match parse_uci_attrs(attrs, fen, moves) {
        Ok(best)
            if best.score.lower_bound == Some(true) || best.score.upper_bound == Some(true) =>
        {
            None
        }
        Ok(best) => Some(best),
        Err(Error::NoMovesFound) => None,
        Err(e) => {
            log::warn!("Failed to parse info line, error: {:?}", e);
            None
        }
    }
}

/// Parse UCI info attributes into a `BestMoves` struct for the current position.
///
/// # Arguments
/// * `attrs` - UCI info attributes from the engine.
/// * `fen` - FEN string for the position.
/// * `moves` - List of moves leading to the position.
///
/// # Returns
/// `BestMoves` struct with parsed data.
///
/// # Errors
/// Returns `Error` if parsing fails or no moves are found.
pub fn parse_uci_attrs(
    attrs: Vec<UciInfoAttribute>,
    fen: &Fen,
    moves: &Vec<String>,
) -> Result<BestMoves, Error> {
    let mut best_moves = BestMoves::default();

    let mut pos: Chess = match fen.clone().into_position(CastlingMode::Chess960) {
        Ok(p) => p,
        Err(e) => e.ignore_too_much_material()?,
    };
    for m in moves {
        let uci = UciMove::from_ascii(m.as_bytes())?;
        let mv = uci.to_move(&pos)?;
        pos.play_unchecked(&mv);
    }
    let turn = pos.turn();

    for a in attrs {
        match a {
            UciInfoAttribute::Pv(m) => {
                for mv in m {
                    let uci: UciMove = mv.to_string().parse()?;
                    let m = uci.to_move(&pos)?;
                    let san = SanPlus::from_move_and_play_unchecked(&mut pos, &m);
                    best_moves.san_moves.push(san.to_string());
                    best_moves.uci_moves.push(uci.to_string());
                }
            }
            UciInfoAttribute::Nps(nps) => {
                best_moves.nps = nps as u32;
            }
            UciInfoAttribute::Nodes(nodes) => {
                best_moves.nodes = nodes as u32;
            }
            UciInfoAttribute::Depth(depth) => {
                best_moves.depth = depth;
            }
            UciInfoAttribute::MultiPv(multipv) => {
                best_moves.multipv = multipv;
            }
            UciInfoAttribute::Score(score) => {
                best_moves.score = score;
            }
            _ => (),
        }
    }

    if best_moves.san_moves.is_empty() {
        return Err(Error::NoMovesFound);
    }

    if turn == Color::Black {
        best_moves.score = invert_score(best_moves.score);
    }

    Ok(best_moves)
}

/// The UCI `go` line for a search mode. A side without a clock gets no `wtime`/`btime`.
fn go_command(mode: &GoMode) -> String {
    match mode {
        GoMode::Depth(depth) => format!("go depth {}\n", depth),
        GoMode::Time(time) => format!("go movetime {}\n", time),
        GoMode::Nodes(nodes) => format!("go nodes {}\n", nodes),
        GoMode::PlayersTime(super::types::PlayersTime {
            white,
            black,
            winc,
            binc,
        }) => {
            let mut msg = "go".to_string();
            if let Some(white) = white {
                msg.push_str(&format!(" wtime {white} winc {winc}"));
            }
            if let Some(black) = black {
                msg.push_str(&format!(" btime {black} binc {binc}"));
            }
            msg.push_str(" movetime 1000\n");
            msg
        }
        GoMode::Infinite => "go infinite\n".to_string(),
    }
}

#[cfg(test)]
mod go_command_tests {
    use super::*;
    use crate::chess::types::PlayersTime;

    #[test]
    fn omits_the_time_of_a_side_without_a_clock() {
        let mode = GoMode::PlayersTime(PlayersTime {
            white: None,
            black: Some(59000),
            winc: 0,
            binc: 1000,
        });
        assert_eq!(
            go_command(&mode),
            "go btime 59000 binc 1000 movetime 1000\n"
        );
    }

    #[test]
    fn sends_both_clocks_when_both_sides_have_one() {
        let mode = GoMode::PlayersTime(PlayersTime {
            white: Some(1),
            black: Some(2),
            winc: 3,
            binc: 4,
        });
        assert_eq!(
            go_command(&mode),
            "go wtime 1 winc 3 btime 2 binc 4 movetime 1000\n"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vampirc_uci::{parse_one, UciMessage};

    const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    fn best_moves_from(line: &str) -> Option<BestMoves> {
        let UciMessage::Info(attrs) = parse_one(line) else {
            return None;
        };
        best_moves_from_info(attrs, &START.parse().unwrap(), &vec![])
    }

    fn test_process() -> EngineProcess {
        let mut child = tokio::process::Command::new("cat")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        EngineProcess {
            lifecycle: EngineLifecycle::new(),
            child,
            stdin,
            last_depth: 0,
            best_moves: Vec::new(),
            last_best_moves: Vec::new(),
            last_progress: 0.0,
            options: EngineOptions::default(),
            go_mode: GoMode::Depth(1),
            running: true,
            real_multipv: 1,
            logs: LogBuffer::default(),
            start: Instant::now(),
        }
    }

    #[tokio::test]
    async fn throttled_final_depth_is_still_kept_for_bestmove() {
        let mut proc = test_process();
        let best = best_moves_from("info depth 1 score cp 30 multipv 1 pv e2e4").unwrap();

        let emitted = proc.record_info(best, || false);

        assert!(emitted.is_none(), "throttled: nothing emitted");
        assert_eq!(proc.last_best_moves.len(), 1);
        assert_eq!(proc.last_best_moves[0].uci_moves, vec!["e2e4"]);
        assert_eq!(proc.last_progress, 100.0);
    }

    #[tokio::test]
    async fn unthrottled_depth_is_emitted_with_progress() {
        let mut proc = test_process();
        let best = best_moves_from("info depth 1 score cp 30 multipv 1 pv e2e4").unwrap();

        let (progress, lines) = proc.record_info(best, || true).unwrap();

        assert_eq!(progress, 100.0);
        assert_eq!(lines.len(), 1);
    }

    #[tokio::test]
    async fn changing_options_resets_progress() {
        let mut proc = test_process();
        proc.last_progress = 100.0;

        proc.set_options(EngineOptions {
            fen: START.to_string(),
            moves: vec![],
            extra_options: vec![],
        })
        .await
        .unwrap();

        assert_eq!(proc.last_progress, 0.0);
    }

    #[test]
    fn non_standard_info_string_stays_an_info_message() {
        let msg = parse_one("info string NNUE evaluation using nn-1111cefa1111.nnue enabled");
        assert!(matches!(msg, UciMessage::Info(_)), "{msg:?}");
    }

    #[test]
    fn info_line_with_unknown_token_keeps_known_attributes() {
        let best = best_moves_from("info depth 2 policy 1593 score cp 20 multipv 1 pv e2e4 c7c6")
            .expect("line with an unknown token is still parsed");
        assert_eq!(best.depth, 2);
        assert_eq!(best.uci_moves, vec!["e2e4", "c7c6"]);
    }

    #[test]
    fn lowerbound_line_is_not_a_best_line() {
        assert!(best_moves_from("info depth 5 score cp 30 lowerbound multipv 1 pv e2e4").is_none());
    }

    #[test]
    fn upperbound_line_is_not_a_best_line() {
        assert!(best_moves_from("info depth 5 score cp 30 upperbound multipv 1 pv e2e4").is_none());
    }

    #[test]
    fn exact_line_is_a_best_line() {
        assert!(best_moves_from("info depth 5 score cp 30 multipv 1 pv e2e4").is_some());
    }

    #[test]
    fn log_keeps_only_the_most_recent_lines() {
        let mut logs = LogBuffer::default();
        for i in 0..MAX_LOG_LINES + 500 {
            logs.push(EngineLog::Engine(i.to_string()));
        }
        let kept = logs.to_vec();
        assert_eq!(kept.len(), MAX_LOG_LINES);
        assert!(matches!(&kept[0], EngineLog::Engine(l) if l == "500"));
        assert!(
            matches!(kept.last(), Some(EngineLog::Engine(l)) if l == &(MAX_LOG_LINES + 499).to_string())
        );
    }

    #[tokio::test]
    async fn handle_line_turns_info_and_bestmove_into_events() {
        let mut proc = test_process();
        proc.options.fen = START.to_string();

        let depth = proc.handle_line("info depth 1 score cp 30 multipv 1 pv e2e4", || true);
        let LineEvent::Depth { lines, .. } = depth else {
            panic!("expected a depth");
        };
        assert_eq!(lines[0].uci_moves, vec!["e2e4"]);

        let done = proc.handle_line("bestmove e2e4", || true);
        let LineEvent::BestMove { lines } = done else {
            panic!("expected bestmove");
        };
        assert_eq!(lines.len(), 1);
        assert!(!proc.running);
        assert_eq!(proc.logs.to_vec().len(), 2);
    }
}
