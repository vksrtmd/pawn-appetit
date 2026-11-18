//! Engine process management for UCI chess engines.
//!
//! This module provides the `EngineManager` struct, which manages engine processes, handles best-move queries,
//! and spawns background tasks for engine output parsing and progress reporting.

use std::path::PathBuf;

use log::{debug, info};
use tauri_specta::Event;

use crate::error::Error;
use crate::AppState;

use super::process::LineEvent;
use super::types::{EngineOptions, GoMode};

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
            if options == process.options && go_mode == process.go_mode && process.running {
                return Ok(Some((
                    process.last_progress,
                    process.last_best_moves.clone(),
                )));
            }
            // Otherwise stop and reconfigure it (it should answer `stop` quickly).
            process.stop().await?;
            drop(process);
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
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
            // Limit event emission rate to avoid UI flooding.
            let lim = governor::RateLimiter::direct(governor::Quota::per_second(
                nonzero_ext::nonzero!(5u32),
            ));
            while let Ok(Some(line)) = reader.next_line().await {
                debug!("[engine-stdout tab={} engine={}] {}", key.0, key.1, line);
                let mut proc = process.lock().await;
                let payload = match proc.handle_line(&line, || lim.check().is_ok()) {
                    LineEvent::Depth { progress, lines } => Some((progress, lines)),
                    LineEvent::BestMove { lines } => Some((100.0, lines)),
                    LineEvent::Other => None,
                };
                if let Some((progress, best_lines)) = payload {
                    super::types::BestMovesPayload {
                        best_lines,
                        engine: id.clone(),
                        tab: tab.clone(),
                        fen: proc.options.fen.clone(),
                        moves: proc.options.moves.clone(),
                        progress,
                    }
                    .emit(&app)
                    .ok();
                }
            }
            info!("Engine process finished: tab: {}, engine: {}", key.0, key.1);
            process.lock().await.lifecycle.mark_stopped();
            registry.remove(&key, &slot);
        });

        Ok(None)
    }
}
