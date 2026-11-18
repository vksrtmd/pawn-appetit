//! Engine process management for UCI chess engines.
//!
//! This module provides the `EngineManager` struct, which manages engine processes, handles best-move queries,
//! and spawns background tasks for engine output parsing and progress reporting.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use log::info;
use tauri_specta::Event;
use tokio::sync::Mutex;

use crate::error::Error;
use crate::AppState;

use super::process::{EngineProcess, LineEvent};
use super::registry::EngineLines;
use super::types::{BestMoves, EngineOptions, GoMode};

/// Manager for UCI engine processes, handling best-move queries and process lifecycle.
pub struct EngineManager<'a> {
    state: tauri::State<'a, AppState>,
}

impl<'a> EngineManager<'a> {
    /// Create a new `EngineManager` with the given application state.
    pub fn new(state: tauri::State<'a, AppState>) -> Self {
        Self { state }
    }

    /// Get best moves from the engine for a given position and options.
    ///
    /// If an engine process is already running for the given key, it will reuse or update it as needed.
    /// Otherwise, it spawns a new process and background reader task.
    ///
    /// # Arguments
    /// * `id` - Unique analysis session identifier.
    /// * `engine` - Path to the UCI engine binary.
    /// * `tab` - Tab identifier for engine process grouping.
    /// * `owner` - Tab whose closing ends the engine; `tab` when `None`.
    /// * `go_mode` - Engine search mode (depth, time, etc).
    /// * `options` - Engine options (FEN, moves, etc).
    /// * `app` - Tauri app handle for event emission.
    ///
    /// # Returns
    /// Optionally returns the last progress and best moves if already available.
    ///
    /// # Errors
    /// Returns `Error` if engine operations fail.
    #[allow(clippy::too_many_arguments)]
    pub async fn get_best_moves(
        &self,
        id: String,
        engine: String,
        tab: String,
        owner: Option<String>,
        go_mode: GoMode,
        options: EngineOptions,
        app: tauri::AppHandle,
    ) -> Result<Option<(f32, Vec<super::types::BestMoves>)>, Error> {
        let key = (tab.clone(), engine.clone());
        let registry = self.state.engines.clone();
        let owner = owner.unwrap_or_else(|| tab.clone());
        let started = registry
            .get_or_start(key.clone(), owner, PathBuf::from(&engine))
            .await?;

        let mut process = started.process.lock().await;
        if started.reader.is_none() {
            // Reusing a running engine: return the cached result when nothing changed.
            if options == process.options && go_mode == process.go_mode && process.is_searching() {
                return Ok(Some((
                    process.last_progress,
                    process.last_best_moves.clone(),
                )));
            }
            // Otherwise stop and reconfigure it once it has answered `isready`.
            process.stop().await?;
            drop(process);
            wait_until_stopped(&started.process).await?;
            if started.slot.is_closed() {
                return Err(Error::EngineKilled);
            }
            process = started.process.lock().await;
        }
        process.set_options(options).await?;
        process.go(&go_mode).await?;
        drop(process);

        // The creating call also starts the reader task, so engines run concurrently.
        let Some(mut reader) = started.reader else {
            return Ok(None);
        };
        let slot = started.slot;
        let process = started.process;
        tokio::spawn(async move {
            info!(
                "Engine loop started: tab={} engine={} session={}",
                key.0,
                key.1,
                process.lock().await.lifecycle.session_id
            );
            pump(&process, reader, |proc, progress, best_lines, bestmove| {
                super::types::BestMovesPayload {
                    best_lines,
                    engine: id.clone(),
                    tab: tab.clone(),
                    fen: proc.options.fen.clone(),
                    moves: proc.options.moves.clone(),
                    progress,
                    bestmove,
                }
                .emit(&app)
                .ok();
            })
            .await;
            info!("Engine process finished: tab: {}, engine: {}", key.0, key.1);
            process.lock().await.lifecycle.mark_stopped();
            registry.remove(&key, &slot);
        });

        Ok(None)
    }
}

/// How long an engine gets to answer `isready` after `stop`.
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// Wait until a stopping engine has answered `isready` (the reader task handles that line).
///
/// # Errors
/// `Error::EngineTimeout` when it has not answered within [`STOP_TIMEOUT`].
pub(crate) async fn wait_until_stopped(process: &Mutex<EngineProcess>) -> Result<(), Error> {
    let end = Instant::now() + STOP_TIMEOUT;
    while process.lock().await.lifecycle.is_stopping() {
        if Instant::now() >= end {
            return Err(Error::EngineTimeout("stop".to_string()));
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Ok(())
}

/// Read the engine's output until it ends, calling `emit(process, progress, lines, bestmove)`
/// for each depth that is due (at most 5 a second) and for the final `bestmove`, which
/// carries progress 100 and the engine's move. Nothing else reports a search as done.
pub(crate) async fn pump(
    process: &Mutex<EngineProcess>,
    mut reader: EngineLines,
    mut emit: impl FnMut(&EngineProcess, f64, Vec<BestMoves>, Option<String>),
) {
    // Limit event emission rate to avoid UI flooding.
    let lim =
        governor::RateLimiter::direct(governor::Quota::per_second(nonzero_ext::nonzero!(5u32)));
    while let Ok(Some(line)) = reader.next_line().await {
        let mut proc = process.lock().await;
        match proc.handle_line(&line, || lim.check().is_ok()) {
            LineEvent::Depth { progress, lines } => emit(&proc, progress, lines, None),
            LineEvent::BestMove { lines, bestmove } => emit(&proc, 100.0, lines, bestmove),
            LineEvent::Other => {}
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::chess::registry::EngineRegistry;
    use crate::chess::test_support::script;
    use std::sync::Arc;

    const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    /// Answers `stop` only after 300 ms, like an engine with a big hash. Its first search
    /// ends with `bestmove e2e4` when stopped; the second one answers `bestmove d2d4` itself.
    const SLOW_STOP: &str = r#"n=0
while read l; do
  case "$l" in
    uci) echo uciok;;
    isready) echo readyok;;
    go*) n=$((n+1))
         echo "info depth 1 score cp 10 multipv 1 pv e2e4"
         [ $n -eq 2 ] && { sleep 0.2; echo "bestmove d2d4"; };;
    stop) sleep 0.3; echo "bestmove e2e4";;
  esac
done
"#;

    struct Rig {
        process: Arc<Mutex<EngineProcess>>,
        finals: Arc<std::sync::Mutex<Vec<Option<String>>>>,
        _dir: tempfile::TempDir,
    }

    async fn rig(body: &str) -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let path = script(dir.path(), "engine.sh", body);
        let started = EngineRegistry::default()
            .get_or_start(("tab".into(), "e".into()), "tab".into(), path)
            .await
            .unwrap();
        let finals = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (process, sink) = (started.process.clone(), finals.clone());
        let reader = started.reader.unwrap();
        tokio::spawn(async move {
            pump(&process, reader, |_, progress, _, bestmove| {
                if progress >= 100.0 {
                    sink.lock().unwrap().push(bestmove);
                }
            })
            .await
        });
        Rig {
            process: started.process,
            finals,
            _dir: dir,
        }
    }

    async fn search(rig: &Rig) {
        let mut p = rig.process.lock().await;
        p.set_options(EngineOptions {
            fen: START.to_string(),
            moves: vec![],
            extra_options: vec![],
        })
        .await
        .unwrap();
        p.go(&GoMode::Infinite).await.unwrap();
    }

    async fn stop(rig: &Rig) {
        rig.process.lock().await.stop().await.unwrap();
        wait_until_stopped(&rig.process).await.unwrap();
    }

    #[tokio::test]
    async fn a_late_bestmove_of_a_stopped_search_does_not_end_the_next_search() {
        let rig = rig(SLOW_STOP).await;
        search(&rig).await;
        stop(&rig).await;
        assert!(!rig.process.lock().await.is_searching());

        search(&rig).await;
        // The stopped search's `bestmove e2e4` came before readyok; only the new one counts.
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(rig.process.lock().await.is_searching(), "ended too early");
        let finals = rig.finals.clone();
        assert!(
            crate::chess::test_support::eventually(2, || !finals.lock().unwrap().is_empty()).await
        );
        assert_eq!(*rig.finals.lock().unwrap(), vec![Some("d2d4".to_string())]);
    }

    #[tokio::test]
    async fn stop_ends_a_search_whose_engine_answers_late() {
        let rig = rig(SLOW_STOP).await;
        search(&rig).await;
        let started = Instant::now();
        stop(&rig).await;
        assert!(started.elapsed() >= Duration::from_millis(300));
        assert!(!rig.process.lock().await.is_searching());
        assert!(rig.finals.lock().unwrap().is_empty(), "stopped is not done");
    }

    #[tokio::test]
    async fn the_final_payload_carries_the_engines_bestmove_not_its_top_line() {
        let rig = rig(r#"while read l; do
  case "$l" in
    uci) echo uciok;;
    isready) echo readyok;;
    go*) echo "info depth 1 score cp 10 multipv 1 pv e2e4"; echo "bestmove d2d4";;
  esac
done
"#)
        .await;
        search(&rig).await;
        let finals = rig.finals.clone();
        assert!(
            crate::chess::test_support::eventually(2, || !finals.lock().unwrap().is_empty()).await
        );
        assert_eq!(*rig.finals.lock().unwrap(), vec![Some("d2d4".to_string())]);
    }
}
