use std::path::PathBuf;

use crate::{
    db::{cache::SEARCH_CACHE, connection::get_db_or_create, write},
    error::Result,
    AppState,
};

pub(super) fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> impl std::future::Future<Output = Result<T>> {
    let handle = tauri::async_runtime::spawn_blocking(work);
    async move {
        handle
            .await
            .map_err(|e| crate::error::Error::Io(std::io::Error::other(e)))?
    }
}

#[tauri::command]
#[specta::specta]
pub async fn delete_database(file: PathBuf, state: tauri::State<'_, AppState>) -> Result<()> {
    let path_str = file.to_str().unwrap();
    state.connection_pool.remove(path_str);

    write::drop_database(&file)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_duplicated_games(file: PathBuf, app: tauri::AppHandle) -> Result<()> {
    use tauri::Manager;
    blocking(move || {
        let state = app.state::<AppState>();
        let mut db = get_db_or_create(&state, file.to_str().unwrap())?;
        write::remove_duplicates(&mut db)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn delete_empty_games(file: PathBuf, app: tauri::AppHandle) -> Result<()> {
    use tauri::Manager;
    blocking(move || {
        let state = app.state::<AppState>();
        let mut db = get_db_or_create(&state, file.to_str().unwrap())?;
        write::remove_empty_games(&mut db)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub fn clear_games() {
    SEARCH_CACHE.clear();
}
