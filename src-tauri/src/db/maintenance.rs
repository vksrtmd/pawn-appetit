use std::{path::PathBuf, sync::Arc};

use crate::{
    db::{
        connection::{get_db_or_create, ConnectionOptions},
        write,
    },
    error::Result,
    AppState,
};

#[tauri::command]
#[specta::specta]
pub async fn delete_database(file: PathBuf, state: tauri::State<'_, AppState>) -> Result<()> {
    let pool = &state.connection_pool;
    let path_str = file.to_str().unwrap();
    pool.remove(path_str);

    write::drop_database(std::path::Path::new(path_str))
}

#[tauri::command]
#[specta::specta]
pub async fn delete_duplicated_games(
    file: PathBuf,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    let db = &mut get_db_or_create(&state, file.to_str().unwrap(), ConnectionOptions::default())?;

    write::remove_duplicates(db)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_empty_games(file: PathBuf, state: tauri::State<'_, AppState>) -> Result<()> {
    let db = &mut get_db_or_create(&state, file.to_str().unwrap(), ConnectionOptions::default())?;

    write::remove_empty_games(db)
}

#[tauri::command]
#[specta::specta]
pub fn clear_games(state: tauri::State<'_, AppState>) {
    let mut state = state.db_cache.lock().unwrap();
    *state = Arc::new(Vec::new());
}
