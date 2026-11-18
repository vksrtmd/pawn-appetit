//! The one step every write to a game database goes through.
//!
//! A writer runs its SQL inside [`apply`] and names the [`Change`] it made.
//! `apply` then does what that change owes the data derived from the games
//! (stored counts, orphaned players, events and sites), in the same
//! transaction as the writer's SQL: a failure anywhere rolls all of it back.
//! It takes a connection, not the app state.

use diesel::{connection::SimpleConnection, prelude::*};

use super::{
    core, game_index,
    models::{NewGame, UpdateGame},
    schema::{games, players},
};
use crate::error::{Error, Result};

const GAMES_DELETE_DUPLICATES: &str =
    include_str!("../../../database/queries/games/delete_duplicates.sql");

/// What a writer did to the database.
#[derive(Debug, Clone, Copy)]
pub enum Change<'a> {
    /// Games were inserted (a bulk import or a single game).
    GamesInserted,
    /// These games were edited in place.
    GamesEdited(&'a [i32]),
    /// Games were deleted.
    GamesDeleted,
    /// One player's games were moved to another and the player row removed.
    PlayersMerged,
}

/// Runs `write` and then what `change` owes, in one transaction.
pub fn apply<T>(
    conn: &mut SqliteConnection,
    change: Change<'_>,
    write: impl FnOnce(&mut SqliteConnection) -> Result<T>,
) -> Result<T> {
    conn.transaction::<_, Error, _>(|conn| {
        let out = write(conn)?;
        settle(conn, change)?;
        Ok(out)
    })
}

/// The derived data a change owes. Later work adds its step here:
/// - per-file cache invalidation (#150): runs for every change.
fn settle(conn: &mut SqliteConnection, change: Change<'_>) -> Result<()> {
    match change {
        // A skipped duplicate can leave an orphan; sweeping every row of a
        // large import for it costs more than it is worth, so only count.
        Change::GamesInserted => {}
        Change::GamesEdited(ids) => {
            for &id in ids {
                game_index::reindex_game(conn, id)?;
            }
            sweep_orphans(conn)?
        }
        Change::GamesDeleted | Change::PlayersMerged => sweep_orphans(conn)?,
    }
    core::refresh_info_counts(conn)
}

/// Removes players, events and sites no game references, keeping the
/// "Unknown" row (ID 0).
fn sweep_orphans(conn: &mut SqliteConnection) -> Result<()> {
    conn.batch_execute(
        "
        DELETE FROM Players WHERE ID != 0 AND ID NOT IN (
            SELECT WhiteID FROM Games UNION SELECT BlackID FROM Games
        );
        DELETE FROM Events WHERE ID != 0 AND ID NOT IN (SELECT EventID FROM Games);
        DELETE FROM Sites WHERE ID != 0 AND ID NOT IN (SELECT SiteID FROM Games);
        ",
    )?;
    Ok(())
}

/// Adds one game and returns its row. Bulk imports go through `apply` instead.
#[allow(dead_code)]
pub fn add_game(conn: &mut SqliteConnection, game: NewGame) -> Result<super::models::Game> {
    apply(conn, Change::GamesInserted, |conn| {
        core::add_game(conn, game)
    })
}

pub fn update_game(conn: &mut SqliteConnection, id: i32, data: &UpdateGame) -> Result<()> {
    apply(conn, Change::GamesEdited(&[id]), |conn| {
        core::update_game(conn, id, data)
    })
}

pub fn remove_game(conn: &mut SqliteConnection, id: i32) -> Result<()> {
    apply(conn, Change::GamesDeleted, |conn| {
        diesel::delete(games::table.filter(games::id.eq(id))).execute(conn)?;
        Ok(())
    })
}

pub fn remove_duplicates(conn: &mut SqliteConnection) -> Result<()> {
    apply(conn, Change::GamesDeleted, |conn| {
        conn.batch_execute(GAMES_DELETE_DUPLICATES)?;
        Ok(())
    })
}

pub fn remove_empty_games(conn: &mut SqliteConnection) -> Result<()> {
    apply(conn, Change::GamesDeleted, |conn| {
        diesel::delete(games::table.filter(games::ply_count.eq(0))).execute(conn)?;
        Ok(())
    })
}

/// Moves every game of `player1` to `player2` and removes `player1`.
pub fn merge_players(conn: &mut SqliteConnection, player1: i32, player2: i32) -> Result<()> {
    apply(conn, Change::PlayersMerged, |conn| {
        // Check if the players never played against each other
        let count: i64 = games::table
            .filter(games::white_id.eq(player1).and(games::black_id.eq(player2)))
            .or_filter(games::white_id.eq(player2).and(games::black_id.eq(player1)))
            .limit(1)
            .count()
            .get_result(conn)?;
        if count > 0 {
            return Err(Error::NotDistinctPlayers);
        }

        diesel::update(games::table.filter(games::white_id.eq(player1)))
            .set(games::white_id.eq(player2))
            .execute(conn)?;
        diesel::update(games::table.filter(games::black_id.eq(player1)))
            .set(games::black_id.eq(player2))
            .execute(conn)?;
        diesel::delete(players::table.filter(players::id.eq(player1))).execute(conn)?;
        Ok(())
    })
}

/// Removes the database file. The caller drops its pooled connection first.
/// There is no connection to settle; #150's cache invalidation goes here.
pub fn drop_database(path: &std::path::Path) -> Result<()> {
    std::fs::remove_file(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{
        core::{
            init_db,
            tests::{info_value, names, seed_game},
        },
        import::insert_to_db,
        models::Outcome,
        ops::create_player,
        pgn::Importer,
        schema::{events, sites},
    };
    use pgn_reader::BufferedReader;
    use std::path::PathBuf;

    /// A fresh database file in the temp dir, removed on drop.
    struct TempDb(PathBuf);
    impl TempDb {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "pawn-write-{tag}-{}-{:?}.db3",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_file(&path);
            TempDb(path)
        }
        fn open(&self) -> SqliteConnection {
            let mut conn = SqliteConnection::establish(self.0.to_str().unwrap()).unwrap();
            init_db(&mut conn, "T", "D").unwrap();
            conn
        }
    }
    impl Drop for TempDb {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn real(conn: &mut SqliteConnection) -> [i64; 4] {
        [
            games::table.count().get_result(conn).unwrap(),
            players::table.count().get_result(conn).unwrap(),
            events::table.count().get_result(conn).unwrap(),
            sites::table.count().get_result(conn).unwrap(),
        ]
    }

    fn stored(conn: &mut SqliteConnection) -> [i64; 4] {
        ["GameCount", "PlayerCount", "EventCount", "SiteCount"]
            .map(|n| info_value(conn, n).unwrap().parse().unwrap())
    }

    fn assert_counts_right(conn: &mut SqliteConnection) {
        assert_eq!(stored(conn), real(conn));
    }

    fn update_for(white: &str, black: &str) -> UpdateGame {
        UpdateGame {
            fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".into(),
            event: "Open".into(),
            site: "Open site".into(),
            date: None,
            time: None,
            round: None,
            white: white.into(),
            white_elo: None,
            black: black.into(),
            black_elo: None,
            result: Outcome::Unknown,
            time_control: None,
            eco: None,
            ply_count: None,
            moves: "1. e4 e5 *".into(),
        }
    }

    #[test]
    fn editing_a_game_to_a_new_player_raises_the_stored_player_count() {
        let db = TempDb::new("edit-raise");
        let mut conn = db.open();
        let a = seed_game(&mut conn, "Alice", "Bob", "Open");
        seed_game(&mut conn, "Alice", "Bob", "Cup");
        core::refresh_info_counts(&mut conn).unwrap();
        assert_eq!(info_value(&mut conn, "PlayerCount").as_deref(), Some("3"));
        update_game(&mut conn, a, &update_for("Alice", "Carol")).unwrap();
        // Bob stays (second game), Carol is new: 4 including Unknown.
        assert_eq!(info_value(&mut conn, "PlayerCount").as_deref(), Some("4"));
        update_game(&mut conn, a, &update_for("Alice", "Bob")).unwrap();
        // Carol is now unreferenced and swept: back to 3.
        assert_eq!(info_value(&mut conn, "PlayerCount").as_deref(), Some("3"));
        let (p, _, _) = names(&mut conn);
        assert_eq!(p, ["Unknown", "Alice", "Bob"]);
        assert_counts_right(&mut conn);
    }

    #[test]
    fn adding_a_single_game_updates_the_stored_game_count() {
        let db = TempDb::new("add");
        let mut conn = db.open();
        let white = create_player(&mut conn, "Alice").unwrap().id;
        let black = create_player(&mut conn, "Bob").unwrap().id;
        let game = NewGame {
            event_id: 0,
            site_id: 0,
            date: Some("2026.01.01"),
            time: Some("10:00:00"),
            end_date: None,
            end_time: None,
            round: None,
            white_id: white,
            white_elo: None,
            black_id: black,
            black_elo: None,
            white_material: 0,
            black_material: 0,
            result: Some("*"),
            time_control: None,
            eco: None,
            ply_count: 2,
            fen: None,
            moves: &[0u8],
            pawn_home: 0,
        };
        add_game(&mut conn, game).unwrap();
        assert_eq!(info_value(&mut conn, "GameCount").as_deref(), Some("1"));
        assert_counts_right(&mut conn);
    }

    #[test]
    fn a_bulk_import_leaves_stored_counts_equal_to_row_counts() {
        let db = TempDb::new("import");
        let mut conn = db.open();
        let pgn = r#"[Event "e"]
[Date "2026.09.09"]
[UTCTime "11:05:14"]
[White "bonfire123"]
[Black "capaloco"]
[Result "0-1"]

1. e4 e5 0-1
"#;
        apply(&mut conn, Change::GamesInserted, |conn| {
            let mut reader = BufferedReader::new_cursor(pgn);
            let mut importer = Importer::new(None);
            let game = reader.read_game(&mut importer).unwrap().flatten().unwrap();
            insert_to_db(conn, &game)
        })
        .unwrap();
        assert_eq!(info_value(&mut conn, "GameCount").as_deref(), Some("1"));
        assert_counts_right(&mut conn);
    }

    #[test]
    fn removing_a_game_leaves_stored_counts_equal_to_row_counts() {
        let db = TempDb::new("remove");
        let mut conn = db.open();
        seed_game(&mut conn, "Alice", "Bob", "Open");
        let gone = seed_game(&mut conn, "Carol", "Dave", "Cup");
        remove_game(&mut conn, gone).unwrap();
        assert_counts_right(&mut conn);
        assert_eq!(real(&mut conn), [1, 3, 2, 2]);
    }

    #[test]
    fn removing_duplicates_and_empty_games_leave_counts_right() {
        let db = TempDb::new("maint");
        let mut conn = db.open();
        seed_game(&mut conn, "Alice", "Bob", "Open");
        seed_game(&mut conn, "Alice", "Bob", "Dup");
        let empty = seed_game(&mut conn, "Carol", "Dave", "Cup");
        diesel::update(games::table.filter(games::id.eq(empty)))
            .set(games::ply_count.eq(0))
            .execute(&mut conn)
            .unwrap();
        remove_duplicates(&mut conn).unwrap();
        assert_counts_right(&mut conn);
        remove_empty_games(&mut conn).unwrap();
        assert_counts_right(&mut conn);
        assert_eq!(real(&mut conn), [1, 3, 2, 2]);
    }

    #[test]
    fn merging_players_leaves_stored_counts_equal_to_row_counts() {
        let db = TempDb::new("merge");
        let mut conn = db.open();
        seed_game(&mut conn, "Alice", "Bob", "Open");
        seed_game(&mut conn, "Carol", "Dave", "Cup");
        let alice = create_player(&mut conn, "Alice").unwrap().id;
        let carol = create_player(&mut conn, "Carol").unwrap().id;
        merge_players(&mut conn, alice, carol).unwrap();
        assert_counts_right(&mut conn);
        assert_eq!(real(&mut conn)[1], 4);
    }

    #[test]
    fn dropping_a_database_removes_its_file() {
        let db = TempDb::new("drop");
        drop(db.open());
        assert!(db.0.exists());
        drop_database(&db.0).unwrap();
        assert!(!db.0.exists());
    }

    #[test]
    fn a_failure_after_the_writer_sql_leaves_nothing_changed() {
        let db = TempDb::new("rollback");
        let mut conn = db.open();
        seed_game(&mut conn, "Alice", "Bob", "Open");
        let gone = seed_game(&mut conn, "Carol", "Dave", "Cup");
        core::refresh_info_counts(&mut conn).unwrap();
        let before = real(&mut conn);
        // The writer's SQL succeeds, then the count refresh fails (no Info
        // table): the delete and the orphan sweep must roll back too.
        let err = apply(&mut conn, Change::GamesDeleted, |conn| {
            diesel::delete(games::table.filter(games::id.eq(gone))).execute(conn)?;
            conn.batch_execute("DROP TABLE Info")?;
            Ok(())
        });
        assert!(err.is_err());
        assert_eq!(real(&mut conn), before);
        assert_eq!(stored(&mut conn), before);
    }

    #[test]
    fn a_writer_error_rolls_back_its_sql() {
        let db = TempDb::new("writer-err");
        let mut conn = db.open();
        let id = seed_game(&mut conn, "Alice", "Bob", "Open");
        let err = apply::<()>(&mut conn, Change::GamesDeleted, |conn| {
            diesel::delete(games::table.filter(games::id.eq(id))).execute(conn)?;
            Err(Error::NotDistinctPlayers)
        });
        assert!(err.is_err());
        assert_eq!(real(&mut conn)[0], 1);
    }

    const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    const KPK_FEN: &str = "4k3/8/8/8/8/8/4P3/4K3 w - - 0 1";

    fn update_with(fen: &str, moves: &str) -> UpdateGame {
        UpdateGame {
            fen: fen.into(),
            moves: moves.into(),
            ..update_for("Alice", "Bob")
        }
    }

    fn stored_fen(conn: &mut SqliteConnection, id: i32) -> Option<String> {
        games::table
            .filter(games::id.eq(id))
            .select(games::fen)
            .first(conn)
            .unwrap()
    }

    fn stored_start(conn: &mut SqliteConnection, id: i32) -> (Option<String>, Option<String>) {
        games::table
            .filter(games::id.eq(id))
            .select((games::date, games::time))
            .first(conn)
            .unwrap()
    }

    const ENDED_GAME: &str = r#"[Event "e"]
[Date "2026.09.09"]
[UTCTime "11:05:14"]
[EndDate "2026.09.10"]
[EndTime "01:02:03"]
[White "Alice"]
[Black "Bob"]
[Result "0-1"]

1. e4 e5 0-1
"#;

    fn import_source_game(conn: &mut SqliteConnection, pgn: &str) {
        apply(conn, Change::GamesInserted, |conn| {
            let mut reader = BufferedReader::new_cursor(pgn);
            let mut importer = Importer::new(None);
            let game = reader.read_game(&mut importer).unwrap().flatten().unwrap();
            insert_to_db(conn, &game)
        })
        .unwrap();
    }

    /// The update the board sends back for a stored game it loaded unchanged.
    fn resave(conn: &mut SqliteConnection, id: i32) -> UpdateGame {
        let game = core::get_game(conn, id).unwrap();
        UpdateGame {
            fen: game.fen,
            event: game.event,
            site: game.site,
            date: game.date,
            time: game.time,
            round: game.round,
            white: game.white,
            white_elo: game.white_elo,
            black: game.black,
            black_elo: game.black_elo,
            result: game.result,
            time_control: game.time_control,
            eco: game.eco,
            ply_count: None,
            moves: game.moves,
        }
    }

    #[test]
    fn a_game_from_a_fen_keeps_its_moves_after_a_save() {
        let db = TempDb::new("fen-moves");
        let mut conn = db.open();
        let id = seed_game(&mut conn, "Alice", "Bob", "Open");
        diesel::update(games::table)
            .set((
                games::white_material.eq(0),
                games::black_material.eq(0),
                games::pawn_home.eq(0),
            ))
            .execute(&mut conn)
            .unwrap();
        update_game(&mut conn, id, &update_with(KPK_FEN, "1. e4 Kd7 2. e5 *")).unwrap();
        let game = core::get_game(&mut conn, id).unwrap();
        assert_eq!(game.fen, KPK_FEN);
        assert_eq!(game.ply_count, Some(3));
        let blob: Vec<u8> = games::table
            .filter(games::id.eq(id))
            .select(games::moves)
            .first(&mut conn)
            .unwrap();
        assert_eq!(blob.len(), 3);
        assert!(
            game.moves.contains("e4") && game.moves.contains("Kd7") && game.moves.contains("e5"),
            "{}",
            game.moves
        );
    }

    #[test]
    fn a_game_at_the_standard_start_keeps_fen_null_after_a_save() {
        let db = TempDb::new("fen-null");
        let mut conn = db.open();
        let id = seed_game(&mut conn, "Alice", "Bob", "Open");
        update_game(&mut conn, id, &update_with(START_FEN, "1. e4 e5 *")).unwrap();
        assert_eq!(stored_fen(&mut conn, id), None);
        update_game(&mut conn, id, &update_with(KPK_FEN, "1. e4 Kd7 *")).unwrap();
        assert_eq!(stored_fen(&mut conn, id).as_deref(), Some(KPK_FEN));
    }

    #[test]
    fn saving_a_game_with_an_end_time_leaves_its_start_date_and_time() {
        let db = TempDb::new("end-time");
        let mut conn = db.open();
        import_source_game(&mut conn, ENDED_GAME);
        let id: i32 = games::table.select(games::id).first(&mut conn).unwrap();
        let update = resave(&mut conn, id);
        update_game(&mut conn, id, &update).unwrap();
        assert_eq!(
            stored_start(&mut conn, id),
            (Some("2026.09.09".into()), Some("11:05:14".into()))
        );
        // The same source game is still recognised as stored.
        import_source_game(&mut conn, ENDED_GAME);
        assert_eq!(real(&mut conn)[0], 1);
    }

    #[test]
    fn editing_the_date_of_a_saved_game_replaces_its_start_and_end() {
        let db = TempDb::new("edit-date");
        let mut conn = db.open();
        import_source_game(&mut conn, ENDED_GAME);
        let id: i32 = games::table.select(games::id).first(&mut conn).unwrap();
        let mut update = resave(&mut conn, id);
        update.date = Some("2026.10.01".into());
        update_game(&mut conn, id, &update).unwrap();
        assert_eq!(
            stored_start(&mut conn, id),
            (Some("2026.10.01".into()), Some("01:02:03".into()))
        );
        let game = core::get_game(&mut conn, id).unwrap();
        assert_eq!(game.date.as_deref(), Some("2026.10.01"));
    }

    #[test]
    fn position_search_finds_a_game_by_a_position_only_its_new_line_reaches() {
        use crate::db::{game_index, schema::game_index as gi};
        use shakmaty::Board;
        let db = TempDb::new("reindex");
        let mut conn = db.open();
        let id = seed_game(&mut conn, "Alice", "Bob", "Open");
        game_index::build_index(&mut conn, |_, _| {}).unwrap();
        let found = |conn: &mut SqliteConnection| -> Vec<i32> {
            let board = Board::from_ascii_board_fen(b"8/8/8/1B6/8/8/8/8").unwrap();
            let up_to = game_index::indexed_up_to(conn);
            let predicate = game_index::IndexFilter::new(&board, up_to)
                .to_sql()
                .unwrap();
            games::table
                .left_join(gi::table)
                .select(games::id)
                .filter(diesel::dsl::sql::<diesel::sql_types::Bool>(&predicate))
                .load(conn)
                .unwrap()
        };
        assert!(found(&mut conn).is_empty());
        update_game(
            &mut conn,
            id,
            &update_with(START_FEN, "1. e4 e5 2. Nf3 Nc6 3. Bb5 *"),
        )
        .unwrap();
        assert_eq!(found(&mut conn), [id]);
    }

    #[test]
    fn a_failing_update_game_changes_nothing() {
        let db = TempDb::new("update-rollback");
        let mut conn = db.open();
        let id = seed_game(&mut conn, "Alice", "Bob", "Open");
        let before = real(&mut conn);
        conn.batch_execute("DROP TABLE Info").unwrap();
        let err = update_game(&mut conn, id, &update_for("Carol", "Dave"));
        assert!(err.is_err());
        assert_eq!(real(&mut conn), before);
        let (white, _, _) = names(&mut conn);
        assert_eq!(white, ["Unknown", "Alice", "Bob"]);
    }

    #[test]
    fn a_failing_merge_players_changes_nothing() {
        let db = TempDb::new("merge-rollback");
        let mut conn = db.open();
        seed_game(&mut conn, "Alice", "Bob", "Open");
        seed_game(&mut conn, "Carol", "Dave", "Cup");
        let alice = create_player(&mut conn, "Alice").unwrap().id;
        let carol = create_player(&mut conn, "Carol").unwrap().id;
        let before = real(&mut conn);
        conn.batch_execute("DROP TABLE Info").unwrap();
        assert!(merge_players(&mut conn, alice, carol).is_err());
        assert_eq!(real(&mut conn), before);
        let alices: i64 = games::table
            .filter(games::white_id.eq(alice))
            .count()
            .get_result(&mut conn)
            .unwrap();
        assert_eq!(alices, 1);
    }

    const C960_FEN: &str = "3k4/8/8/8/8/8/8/1R1K2R1 w GB - 0 1";

    #[test]
    fn a_chess960_game_keeps_its_castling_move_after_a_save() {
        let db = TempDb::new("c960");
        let mut conn = db.open();
        let id = seed_game(&mut conn, "Alice", "Bob", "Open");
        diesel::update(games::table)
            .set((
                games::white_material.eq(0),
                games::black_material.eq(0),
                games::pawn_home.eq(0),
            ))
            .execute(&mut conn)
            .unwrap();
        update_game(&mut conn, id, &update_with(C960_FEN, "1. O-O Kc7 *")).unwrap();
        let game = core::get_game(&mut conn, id).unwrap();
        assert_eq!(game.ply_count, Some(2));
        assert!(
            game.moves.contains("O-O") && game.moves.contains("Kc7"),
            "{}",
            game.moves
        );
    }

    #[test]
    fn importing_a_chess960_game_keeps_its_moves() {
        let db = TempDb::new("c960-import");
        let mut conn = db.open();
        import_source_game(
            &mut conn,
            &format!(
                "[Variant \"Chess960\"]\n[FEN \"{C960_FEN}\"]\n[White \"a\"]\n[Black \"b\"]\n\n1. O-O Kc7 *\n"
            ),
        );
        assert_eq!(real(&mut conn)[0], 1);
    }
}
