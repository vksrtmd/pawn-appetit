//! Reads stay correct after writes, and a file's pool and caches are its own.

use std::{ops::ControlFlow, path::PathBuf};

use diesel::{prelude::*, sql_types::Text, QueryableByName};

use super::{
    cache::SEARCH_CACHE,
    connection::{get_db_or_create, open_pool, ConnectionOptions, GAME_DATE_EXPR, GAME_TIME_EXPR},
    core::{
        init_db,
        tests::{names, seed_game},
    },
    import_pgn,
    models::{Outcome, UpdateGame},
    schema::games,
    search::{position_in_db, run_search, scan_games_in},
    write, GameQueryJs, PositionQueryJs,
};
use crate::AppState;

const E4_GAME: &str = "[Event \"e\"]\n[Date \"2026.01.01\"]\n[White \"Alice\"]\n[Black \"Bob\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n\n";
const D4_GAME: &str = "[Event \"d\"]\n[Date \"2026.01.02\"]\n[White \"Carol\"]\n[Black \"Dave\"]\n[Result \"0-1\"]\n\n1. d4 d5 0-1\n\n";
const AFTER_E4: &str = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
const AFTER_D4: &str = "rnbqkbnr/pppppppp/8/8/3P4/8/PPP1PPPP/RNBQKBNR b KQkq - 0 1";

/// A scratch directory removed on drop.
struct Dir(PathBuf);
impl Dir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("pawn-fresh-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Dir(dir)
    }
    fn db(&self, name: &str) -> PathBuf {
        self.0.join(format!("{name}.db3"))
    }
    /// Imports `pgn` into the database `name` (new or existing).
    fn import(&self, name: &str, pgn: &str) -> PathBuf {
        let source = self.0.join(format!("{name}-{}.pgn", rand_suffix()));
        std::fs::write(&source, pgn).unwrap();
        let db = self.db(name);
        import_pgn(&source, &db, None, "T", "D", |_| {}).unwrap();
        db
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn rand_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

fn query(fen: &str) -> GameQueryJs {
    GameQueryJs::new().position(PositionQueryJs {
        fen: fen.into(),
        type_: "partial".into(),
        forbidden_squares: None,
        exact_pieces: None,
    })
}

/// How many games the position search finds for `fen` in `file`.
fn found(state: &AppState, file: &PathBuf, fen: &str) -> usize {
    let q = query(fen);
    let position = super::search::convert_position_query(q.position.clone().unwrap()).unwrap();
    let (stats, games) = run_search(state, file, &q, &position, &|| false, &|_| {}).unwrap();
    let tallied: i32 = stats.iter().map(|s| s.white + s.black + s.draw).sum();
    assert_eq!(tallied as usize, games.len(), "stats and games disagree");
    games.len()
}

fn game_ids(file: &PathBuf) -> Vec<i32> {
    let mut conn = SqliteConnection::establish(file.to_str().unwrap()).unwrap();
    games::table
        .select(games::id)
        .order(games::id)
        .load(&mut conn)
        .unwrap()
}

#[derive(QueryableByName)]
struct Pragma {
    #[diesel(sql_type = Text)]
    value: String,
}
fn pragma(conn: &mut SqliteConnection, name: &str) -> String {
    let rows: Vec<Pragma> = diesel::sql_query(format!(
        "SELECT CAST(\"{name}\" AS TEXT) AS value FROM pragma_{name}"
    ))
    .load(conn)
    .unwrap();
    rows.into_iter().next().unwrap().value
}

#[test]
fn an_edit_after_an_import_into_a_new_file_runs_with_a_journal_and_foreign_keys() {
    let dir = Dir::new("pragmas");
    let file = dir.import("new", E4_GAME);
    let state = AppState::default();
    let mut conn = get_db_or_create(&state, file.to_str().unwrap()).unwrap();
    assert_eq!(pragma(&mut conn, "journal_mode"), "delete");
    assert_eq!(pragma(&mut conn, "foreign_keys"), "1");
    // The importer's own connection keeps its fast settings.
    let mut importer = ConnectionOptions::import()
        .connect(file.to_str().unwrap())
        .unwrap();
    assert_eq!(pragma(&mut importer, "journal_mode"), "off");
    assert_eq!(pragma(&mut importer, "foreign_keys"), "0");
}

#[test]
fn the_pool_opens_connections_on_demand() {
    let dir = Dir::new("lazy");
    let file = dir.import("lazy", E4_GAME);
    let pool = open_pool(file.to_str().unwrap()).unwrap();
    assert!(pool.state().connections <= 1);
}

#[test]
fn a_search_counts_a_game_added_after_the_first_search() {
    let dir = Dir::new("add");
    let file = dir.import("a", E4_GAME);
    let state = AppState::default();
    assert_eq!(found(&state, &file, AFTER_E4), 1);
    dir.import("a", E4_GAME);
    assert_eq!(found(&state, &file, AFTER_E4), 2);
}

#[test]
fn a_search_drops_a_deleted_game() {
    let dir = Dir::new("delete");
    let file = dir.import("a", &format!("{E4_GAME}{E4_GAME}"));
    let state = AppState::default();
    assert_eq!(found(&state, &file, AFTER_E4), 2);
    let mut conn = get_db_or_create(&state, file.to_str().unwrap()).unwrap();
    write::remove_game(&mut conn, game_ids(&file)[0]).unwrap();
    assert_eq!(found(&state, &file, AFTER_E4), 1);
}

#[test]
fn a_search_follows_an_edited_game() {
    let dir = Dir::new("update");
    let file = dir.import("a", E4_GAME);
    let state = AppState::default();
    assert_eq!(found(&state, &file, AFTER_E4), 1);
    let mut conn = get_db_or_create(&state, file.to_str().unwrap()).unwrap();
    let update = UpdateGame {
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".into(),
        event: "e".into(),
        site: "s".into(),
        date: None,
        time: None,
        round: None,
        white: "Alice".into(),
        white_elo: None,
        black: "Bob".into(),
        black_elo: None,
        result: Outcome::WhiteWin,
        time_control: None,
        eco: None,
        ply_count: None,
        moves: "1. d4 d5 *".into(),
    };
    write::update_game(&mut conn, game_ids(&file)[0], &update).unwrap();
    assert_eq!(found(&state, &file, AFTER_E4), 0);
    assert_eq!(found(&state, &file, AFTER_D4), 1);
}

#[test]
fn merging_players_clears_the_rows_and_results_of_that_file() {
    let dir = Dir::new("merge");
    let file = dir.import("a", &format!("{E4_GAME}{D4_GAME}"));
    let state = AppState::default();
    found(&state, &file, AFTER_E4);
    assert!(SEARCH_CACHE.rows(&file).is_some());
    assert!(SEARCH_CACHE.result(&query(AFTER_E4), &file).is_some());
    let mut conn = get_db_or_create(&state, file.to_str().unwrap()).unwrap();
    write::merge_players(&mut conn, 1, 3).unwrap();
    assert!(SEARCH_CACHE.rows(&file).is_none());
    assert!(SEARCH_CACHE.result(&query(AFTER_E4), &file).is_none());
}

#[test]
fn deleting_a_database_clears_its_caches() {
    let dir = Dir::new("drop");
    let file = dir.import("a", E4_GAME);
    let state = AppState::default();
    found(&state, &file, AFTER_E4);
    write::drop_database(&file).unwrap();
    assert!(SEARCH_CACHE.rows(&file).is_none());
    assert!(SEARCH_CACHE.result(&query(AFTER_E4), &file).is_none());
}

#[test]
fn two_databases_searched_in_turn_return_their_own_results() {
    let dir = Dir::new("two");
    let a = dir.import("a", E4_GAME);
    let b = dir.import("b", D4_GAME);
    let state = AppState::default();
    assert_eq!(found(&state, &a, AFTER_E4), 1);
    assert_eq!(found(&state, &b, AFTER_E4), 0);
    assert_eq!(found(&state, &b, AFTER_D4), 1);
    assert_eq!(found(&state, &a, AFTER_D4), 0);
}

#[test]
fn a_novelty_check_completes_while_panel_searches_hold_every_permit() {
    let dir = Dir::new("novelty");
    let file = dir.import("a", E4_GAME);
    let state = AppState::default();
    let _running: Vec<_> = (0..2)
        .map(|_| state.new_request.clone().try_acquire_owned().unwrap())
        .collect();
    assert_eq!(state.new_request.available_permits(), 0);
    assert!(position_in_db(&state, &file, &query(AFTER_E4)).unwrap());
    assert!(!position_in_db(&state, &file, &query(AFTER_D4)).unwrap());
}

#[test]
fn a_scan_stops_when_the_visitor_breaks_and_does_not_hold_the_cache_lock() {
    let dir = Dir::new("scan");
    let file = dir.import("a", &format!("{E4_GAME}{E4_GAME}{E4_GAME}"));
    let state = AppState::default();
    let q = query(AFTER_E4);
    let position = super::search::convert_position_query(q.position.clone().unwrap()).unwrap();
    let visits = |stop_at_first: bool| {
        let mut visits = 0;
        scan_games_in(
            &state,
            &file,
            &q,
            &position,
            (0, 1),
            &|| false,
            &|_| {},
            |rows| {
                // Would deadlock if the scan held the cache lock.
                SEARCH_CACHE.generation(&file);
                visits += 1;
                assert_eq!(rows.len(), 1);
                if stop_at_first {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            },
        )
        .unwrap();
        visits
    };
    assert_eq!(visits(true), 1);
    assert_eq!(visits(false), 3);
}

#[derive(QueryableByName)]
struct PlanRow {
    #[diesel(sql_type = Text)]
    detail: String,
}

fn plan(conn: &mut SqliteConnection, sql: &str) -> Vec<String> {
    diesel::sql_query(format!("EXPLAIN QUERY PLAN {sql}"))
        .load::<PlanRow>(conn)
        .unwrap()
        .into_iter()
        .map(|r| r.detail)
        .collect()
}

#[test]
fn the_game_list_date_sort_and_date_filters_use_an_index() {
    let dir = Dir::new("plan");
    let file = dir.import("a", &format!("{E4_GAME}{D4_GAME}"));
    let state = AppState::default();
    let mut conn = get_db_or_create(&state, file.to_str().unwrap()).unwrap();
    let joins = "FROM Games \
        JOIN Players w ON Games.WhiteID = w.ID JOIN Players b ON Games.BlackID = b.ID \
        JOIN Events ON Games.EventID = Events.ID JOIN Sites ON Games.SiteID = Sites.ID";
    let sorted = format!(
        "SELECT Games.ID {joins} ORDER BY {GAME_DATE_EXPR} DESC, {GAME_TIME_EXPR} DESC LIMIT 10"
    );
    let filtered = format!(
        "SELECT Games.ID {joins} WHERE {GAME_DATE_EXPR} >= '2026.01.01' AND {GAME_DATE_EXPR} <= '2026.12.31' \
         ORDER BY {GAME_DATE_EXPR} DESC, {GAME_TIME_EXPR} DESC LIMIT 10"
    );
    for sql in [sorted, filtered] {
        let steps = plan(&mut conn, &sql);
        let games_step = steps.iter().find(|d| d.contains(" Games")).unwrap();
        assert!(
            games_step.contains("USING"),
            "full scan of Games: {steps:?}"
        );
        assert!(
            steps.iter().all(|d| !d.contains("USE TEMP B-TREE")),
            "sorted in memory: {steps:?}"
        );
    }
}

#[test]
fn deleting_one_game_removes_only_its_own_orphans() {
    let dir = Dir::new("orphans");
    let file = dir.db("a");
    let mut conn = SqliteConnection::establish(file.to_str().unwrap()).unwrap();
    init_db(&mut conn, "T", "D").unwrap();
    seed_game(&mut conn, "Alice", "Bob", "Open");
    let gone = seed_game(&mut conn, "Alice", "Carol", "Cup");
    // Unreferenced already: a full sweep would take it, the per-game one must not.
    super::ops::create_player(&mut conn, "Eve").unwrap();
    write::remove_game(&mut conn, gone).unwrap();
    let (players, events, sites) = names(&mut conn);
    assert_eq!(players, ["Unknown", "Alice", "Bob", "Eve"]);
    assert_eq!(events, ["Unknown", "Open"]);
    assert_eq!(sites, ["Unknown", "Open site"]);
}
