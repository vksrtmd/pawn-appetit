//! Tauri command handlers for chess engine management and analysis.
//!
//! This module exposes async functions as Tauri commands for engine process control, game analysis, and engine configuration.
//! It acts as the bridge between the frontend and backend chess logic.

use std::path::PathBuf;

use crate::error::Error;
use crate::AppState;

use super::analysis::GameAnalysisService;
use super::manager::EngineManager;
use super::types::*;

/// Kill every engine process of a tab, and any Game Review it is running. `tab` matches the
/// tab id an engine was started for, or its owner, exactly.
#[tauri::command]
#[specta::specta]
pub async fn kill_engines(tab: String, state: tauri::State<'_, AppState>) -> Result<(), Error> {
    state.engines.kill_owner(&tab).await
}

/// Kill a specific engine process by engine name and tab.
#[tauri::command]
#[specta::specta]
pub async fn kill_engine(
    engine: String,
    tab: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), Error> {
    state.engines.kill_key(&(tab, engine)).await
}

/// Stop a specific engine process (without killing it) by engine name and tab.
#[tauri::command]
#[specta::specta]
pub async fn stop_engine(
    engine: String,
    tab: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), Error> {
    if let Some(process) = state.engines.get(&(tab, engine)) {
        process.lock().await.stop().await?;
    }
    Ok(())
}

/// Retrieve logs for a specific engine process.
#[tauri::command]
#[specta::specta]
pub async fn get_engine_logs(
    engine: String,
    tab: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<EngineLog>, Error> {
    match state.engines.get(&(tab, engine)) {
        Some(process) => Ok(process.lock().await.logs.to_vec()),
        None => Ok(Vec::new()),
    }
}

/// Get best moves from the engine for a given position and options.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn get_best_moves(
    id: String,
    engine: String,
    tab: String,
    go_mode: GoMode,
    options: EngineOptions,
    owner: Option<String>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Option<(f32, Vec<BestMoves>)>, Error> {
    EngineManager::new(state)
        .get_best_moves(id, engine, tab, owner, go_mode, options, app)
        .await
}

/// Analyze a game using the engine, returning move-by-move analysis.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn analyze_game(
    id: String,
    engine: String,
    go_mode: GoMode,
    options: AnalysisOptions,
    uci_options: Vec<EngineOption>,
    owner: Option<String>,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<Vec<MoveAnalysis>, Error> {
    let owner = owner.unwrap_or_else(|| id.clone());
    GameAnalysisService::analyze_game(id, engine, go_mode, options, uci_options, owner, state, app)
        .await
}

/// Cancel the running game review `id`: its engine process is killed and `analyze_game` returns
/// `Error::AnalysisCancelled`. Does nothing when no such review is running.
#[tauri::command]
#[specta::specta]
pub async fn cancel_analysis(id: String, state: tauri::State<'_, AppState>) -> Result<(), Error> {
    state.engines.kill_owner(&id).await
}

/// Query a UCI engine for its configuration (name and options).
#[tauri::command]
#[specta::specta]
pub async fn get_engine_config(path: PathBuf) -> Result<EngineConfig, Error> {
    super::uci::read_engine_config(path, super::uci::UCI_TIMEOUT).await
}
