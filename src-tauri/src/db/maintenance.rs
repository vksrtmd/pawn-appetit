use std::{fs::remove_file, path::PathBuf, sync::Arc};

use diesel::{connection::SimpleConnection, prelude::*, SqliteConnection};

use crate::{
    db::{
        connection::{get_db_or_create, ConnectionOptions},
        core,
        schema::games,
    },
    error::Result,
    AppState,
};

const GAMES_DELETE_DUPLICATES: &str =
    include_str!("../../../database/queries/games/delete_duplicates.sql");

#[tauri::command]
#[specta::specta]
pub async fn delete_database(file: PathBuf, state: tauri::State<'_, AppState>) -> Result<()> {
    let pool = &state.connection_pool;
    let path_str = file.to_str().unwrap();
    pool.remove(path_str);

    remove_file(path_str)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_duplicated_games(
    file: PathBuf,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    let db = &mut get_db_or_create(&state, file.to_str().unwrap(), ConnectionOptions::default())?;

    remove_duplicates(db)
}

fn remove_duplicates(db: &mut SqliteConnection) -> Result<()> {
    db.batch_execute(GAMES_DELETE_DUPLICATES)?;
    core::delete_orphaned_data(db)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_empty_games(file: PathBuf, state: tauri::State<'_, AppState>) -> Result<()> {
    let db = &mut get_db_or_create(&state, file.to_str().unwrap(), ConnectionOptions::default())?;

    remove_empty_games(db)
}

fn remove_empty_games(db: &mut SqliteConnection) -> Result<()> {
    diesel::delete(games::table.filter(games::ply_count.eq(0))).execute(db)?;
    core::delete_orphaned_data(db)
}

#[tauri::command]
#[specta::specta]
pub fn clear_games(state: tauri::State<'_, AppState>) {
    let mut state = state.db_cache.lock().unwrap();
    *state = Arc::new(Vec::new());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::core::{
        init_db,
        tests::{info_value, names, seed_game},
    };

    fn test_db() -> SqliteConnection {
        let mut conn = SqliteConnection::establish(":memory:").unwrap();
        init_db(&mut conn, "T", "D").unwrap();
        conn
    }

    #[test]
    fn deleting_duplicates_removes_orphans_and_updates_counts() {
        let mut conn = test_db();
        seed_game(&mut conn, "Alice", "Bob", "Open");
        seed_game(&mut conn, "Alice", "Bob", "Dup");
        remove_duplicates(&mut conn).unwrap();
        let (p, e, s) = names(&mut conn);
        assert_eq!(p, ["Unknown", "Alice", "Bob"]);
        assert_eq!(e, ["Unknown", "Open"]);
        assert_eq!(s, ["Unknown", "Open site"]);
        assert_eq!(info_value(&mut conn, "GameCount").as_deref(), Some("1"));
        assert_eq!(info_value(&mut conn, "EventCount").as_deref(), Some("2"));
    }

    #[test]
    fn deleting_empty_games_removes_orphans_and_updates_counts() {
        let mut conn = test_db();
        seed_game(&mut conn, "Alice", "Bob", "Open");
        let empty = seed_game(&mut conn, "Carol", "Dave", "Cup");
        diesel::update(games::table.filter(games::id.eq(empty)))
            .set(games::ply_count.eq(0))
            .execute(&mut conn)
            .unwrap();
        remove_empty_games(&mut conn).unwrap();
        let (p, e, s) = names(&mut conn);
        assert_eq!(p, ["Unknown", "Alice", "Bob"]);
        assert_eq!(e, ["Unknown", "Open"]);
        assert_eq!(s, ["Unknown", "Open site"]);
        assert_eq!(info_value(&mut conn, "GameCount").as_deref(), Some("1"));
        assert_eq!(info_value(&mut conn, "PlayerCount").as_deref(), Some("3"));
    }
}
