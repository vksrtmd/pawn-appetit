use super::{
    models::{Event, Game, NewGame, NormalizedGame, Outcome, Player, Site, UpdateGame},
    ops::{create_event, create_player, create_site},
    pgn::{GameTree, Importer},
    schema::{events, games, info, players, sites},
};
use crate::error::Result;
use diesel::{connection::SimpleConnection, prelude::*};
use pgn_reader::BufferedReader;
use shakmaty::{fen::Fen, CastlingMode, Chess, FromSetup};
use std::str::FromStr;
use std::string::ToString;

const DATABASE_VERSION: &str = "1.0.0";
const CREATE_TABLES_SQL: &str = include_str!("../../../database/schema/core_tables.sql");
const INITIAL_DATA_SQL: &str = include_str!("../../../database/seeds/initial_data.sql");
#[cfg(test)]
const GAMES_CHECK_INDEXES: &str = include_str!("../../../database/queries/games/check_indexes.sql");

pub fn init_db(conn: &mut SqliteConnection, title: &str, description: &str) -> Result<()> {
    // Create tables
    conn.batch_execute(CREATE_TABLES_SQL)?;

    // Derived position-search index; also created on demand for older databases
    super::game_index::ensure_table(conn)?;

    // Insert initial seed data
    conn.batch_execute(INITIAL_DATA_SQL)?;

    // Insert database metadata
    set_info_value(conn, "Version", DATABASE_VERSION)?;
    set_info_value(conn, "Title", title)?;
    set_info_value(conn, "Description", description)?;

    Ok(())
}

pub fn set_info_value(conn: &mut SqliteConnection, name: &str, value: &str) -> Result<()> {
    diesel::insert_into(info::table)
        .values((info::name.eq(name), info::value.eq(value)))
        .on_conflict(info::name)
        .do_update()
        .set(info::value.eq(value))
        .execute(conn)?;
    Ok(())
}

/// Writes the row counts of each table to the Info table. Players, events and
/// sites include the "Unknown" placeholder row.
pub(super) fn refresh_info_counts(conn: &mut SqliteConnection) -> Result<()> {
    let counts = [
        ("GameCount", games::table.count().get_result::<i64>(conn)?),
        ("PlayerCount", players::table.count().get_result(conn)?),
        ("EventCount", events::table.count().get_result(conn)?),
        ("SiteCount", sites::table.count().get_result(conn)?),
    ];
    for (name, count) in counts {
        set_info_value(conn, name, &count.to_string())?;
    }
    Ok(())
}

pub struct DbCounts {
    pub games: i32,
    pub players: i32,
    pub events: i32,
}

/// Counts as stored in the Info table, without the "Unknown" placeholder in
/// players and events. Databases that never stored them are counted once and
/// the result is written back.
pub fn db_counts(conn: &mut SqliteConnection) -> Result<DbCounts> {
    let read = |conn: &mut SqliteConnection| -> Result<Option<[i32; 3]>> {
        let mut out = [0; 3];
        for (slot, name) in out
            .iter_mut()
            .zip(["GameCount", "PlayerCount", "EventCount"])
        {
            let value: Option<Option<String>> = info::table
                .filter(info::name.eq(name))
                .select(info::value)
                .first(conn)
                .optional()?;
            match value.flatten().and_then(|v| v.parse().ok()) {
                Some(n) => *slot = n,
                None => return Ok(None),
            }
        }
        Ok(Some(out))
    };
    let [games, players, events] = match read(conn)? {
        Some(counts) => counts,
        None => {
            refresh_info_counts(conn)?;
            read(conn)?.unwrap_or_default()
        }
    };
    Ok(DbCounts {
        games,
        players: (players - 1).max(0),
        events: (events - 1).max(0),
    })
}

pub fn normalize_game(
    game: Game,
    white: Player,
    black: Player,
    event: Event,
    site: Site,
) -> Result<NormalizedGame> {
    let fen: Fen = game
        .fen
        .map(|f| Fen::from_ascii(f.as_bytes()))
        .transpose()?
        .unwrap_or_default();

    let start = Chess::from_setup(fen.clone().into(), CastlingMode::Chess960)?;

    Ok(NormalizedGame {
        id: game.id,
        event: event.name.unwrap_or_default(),
        event_id: event.id,
        site: site.name.unwrap_or_default(),
        site_id: site.id,
        date: game.end_date.clone().or(game.date),
        time: game.end_time.clone().or(game.time),
        round: game.round,
        white: white.name.unwrap_or_default(),
        white_id: game.white_id,
        white_elo: game.white_elo,
        black: black.name.unwrap_or_default(),
        black_id: game.black_id,
        black_elo: game.black_elo,
        result: Outcome::from_str(&game.result.unwrap_or_default()).unwrap_or_default(),
        time_control: game.time_control,
        eco: game.eco,
        ply_count: game.ply_count,
        match_ply: None,
        match_fen: None,
        match_next_move: None,
        fen: fen.to_string(),
        moves: GameTree::from_bytes(&game.moves, Some(start.clone()))?
            .from_position(Some(start))
            .to_string(),
    })
}

/// Creates a new game in the database, and returns the game's ID.
pub(super) fn add_game(conn: &mut SqliteConnection, game: NewGame) -> Result<Game> {
    use crate::db::schema::games;

    Ok(diesel::insert_or_ignore_into(games::table)
        .values(&game)
        .get_result(conn)?)
}

pub fn get_game(conn: &mut SqliteConnection, id: i32) -> Result<NormalizedGame> {
    let (white_players, black_players) = diesel::alias!(players as white, players as black);
    let (game, white, black, event, site): (Game, Player, Player, Event, Site) = games::table
        .inner_join(white_players.on(games::white_id.eq(white_players.field(players::id))))
        .inner_join(black_players.on(games::black_id.eq(black_players.field(players::id))))
        .inner_join(events::table.on(games::event_id.eq(events::id)))
        .inner_join(sites::table.on(games::site_id.eq(sites::id)))
        .filter(games::id.eq(id))
        .first(conn)?;

    normalize_game(game, white, black, event, site)
}

/// Saves an edited game. Moves are read from the start position `data.fen`
/// names, so a game that starts from a FEN keeps its moves; `FEN` stays NULL at
/// the standard start, as the importer stores it. `data.date` and `data.time`
/// are the date and time the game is shown with (the end ones when it has
/// them): unchanged, they leave the stored start `Date`/`UTCTime` alone, and
/// edited, they become the game's date and time with the end ones cleared.
pub(super) fn update_game(conn: &mut SqliteConnection, id: i32, data: &UpdateGame) -> Result<()> {
    let start_fen = Fen::from_ascii(data.fen.as_bytes())?;
    let mut pgn = String::new();
    if start_fen != Fen::default() {
        pgn.push_str(&format!("[FEN \"{}\"]\n\n", start_fen));
    }
    pgn.push_str(&data.moves);

    let mut reader = BufferedReader::new_cursor(&pgn);
    let mut importer = Importer::new(None);
    let game = reader
        .read_game(&mut importer)?
        .flatten()
        .ok_or(crate::error::Error::NoMovesFound)?;
    let ply_count = game.tree.count_main_line_moves() as i32;

    let (stored_date, stored_time, end_date, end_time): (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = games::table
        .filter(games::id.eq(id))
        .select((games::date, games::time, games::end_date, games::end_time))
        .first(conn)?;
    let shown_date = end_date.clone().or_else(|| stored_date.clone());
    let shown_time = end_time.clone().or_else(|| stored_time.clone());
    let (date, time, end_date, end_time) = if data.date == shown_date && data.time == shown_time {
        (stored_date, stored_time, end_date, end_time)
    } else {
        (data.date.clone(), data.time.clone(), None, None)
    };

    diesel::update(games::dsl::games)
        .filter(games::id.eq(id))
        .set((
            games::fen.eq(&game.fen),
            games::event_id.eq(create_event(conn, &data.event)?.id),
            games::date.eq(date),
            games::time.eq(time),
            games::end_date.eq(end_date),
            games::end_time.eq(end_time),
            games::round.eq(&data.round),
            games::site_id.eq(create_site(conn, &data.site)?.id),
            games::white_id.eq(create_player(conn, &data.white)?.id),
            games::white_elo.eq(data.white_elo),
            games::black_id.eq(create_player(conn, &data.black)?.id),
            games::black_elo.eq(data.black_elo),
            games::result.eq(data.result.to_string()),
            games::time_control.eq(&data.time_control),
            games::eco.eq(&data.eco),
            games::ply_count.eq(ply_count),
            games::moves.eq(&game.moves),
        ))
        .execute(conn)?;

    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use diesel::{sql_query, sql_types::Text};
    use serde::Serialize;

    fn test_db() -> SqliteConnection {
        let mut conn = SqliteConnection::establish(":memory:").unwrap();
        init_db(&mut conn, "Test", "Test").unwrap();

        conn
    }

    #[derive(QueryableByName, Debug, Serialize)]
    struct IndexInfo {
        #[diesel(sql_type = Text, column_name = "name")]
        _name: String,
    }

    #[test]
    fn test_add_game() {
        let mut db = test_db();

        let query = sql_query(GAMES_CHECK_INDEXES);
        let indexes: Vec<IndexInfo> = query.load(&mut db).unwrap();
        assert!(indexes.is_empty());
    }

    pub(crate) fn info_value(conn: &mut SqliteConnection, name: &str) -> Option<String> {
        use crate::db::schema::info;
        info::table
            .filter(info::name.eq(name))
            .select(info::value)
            .first::<Option<String>>(conn)
            .optional()
            .unwrap()
            .flatten()
    }

    /// Adds a game between two named players with its own event and site.
    /// Games with the same players share date, result and ply count.
    pub(crate) fn seed_game(
        conn: &mut SqliteConnection,
        white: &str,
        black: &str,
        event: &str,
    ) -> i32 {
        let white_id = create_player(conn, white).unwrap().id;
        let black_id = create_player(conn, black).unwrap().id;
        let event_id = create_event(conn, event).unwrap().id;
        let site_id = create_site(conn, &format!("{event} site")).unwrap().id;
        diesel::insert_into(games::table)
            .values((
                games::white_id.eq(white_id),
                games::black_id.eq(black_id),
                games::event_id.eq(event_id),
                games::site_id.eq(site_id),
                games::date.eq("2026.01.01"),
                games::time.eq("10:00:00"),
                games::ply_count.eq(2),
                games::result.eq("*"),
                games::moves.eq(vec![0u8]),
            ))
            .execute(conn)
            .unwrap();
        games::table
            .select(games::id)
            .order(games::id.desc())
            .first(conn)
            .unwrap()
    }

    /// Names of players, events and sites, placeholder row included.
    pub(crate) fn names(conn: &mut SqliteConnection) -> (Vec<String>, Vec<String>, Vec<String>) {
        let flat = |v: Vec<Option<String>>| v.into_iter().flatten().collect();
        (
            flat(
                players::table
                    .select(players::name)
                    .order(players::id)
                    .load(conn)
                    .unwrap(),
            ),
            flat(
                events::table
                    .select(events::name)
                    .order(events::id)
                    .load(conn)
                    .unwrap(),
            ),
            flat(
                sites::table
                    .select(sites::name)
                    .order(sites::id)
                    .load(conn)
                    .unwrap(),
            ),
        )
    }

    #[test]
    fn a_game_with_a_bad_stored_fen_is_an_error_not_a_panic() {
        let mut conn = test_db();
        seed_game(&mut conn, "Alice", "Bob", "Open");
        diesel::update(games::table)
            .set(games::fen.eq("not a fen"))
            .execute(&mut conn)
            .unwrap();
        let id: i32 = games::table.select(games::id).first(&mut conn).unwrap();
        assert!(get_game(&mut conn, id).is_err());
    }

    #[test]
    fn init_db_stores_a_title_with_quotes_and_sql() {
        let mut conn = SqliteConnection::establish(":memory:").unwrap();
        let description = r#"it's "x"); DROP TABLE Games; --"#;
        init_db(&mut conn, r#"My "best" games"#, description).unwrap();
        assert_eq!(
            info_value(&mut conn, "Title").as_deref(),
            Some(r#"My "best" games"#)
        );
        assert_eq!(
            info_value(&mut conn, "Description").as_deref(),
            Some(description)
        );
        assert_eq!(
            games::table.count().get_result::<i64>(&mut conn).unwrap(),
            0
        );
    }

    #[test]
    fn db_counts_exclude_the_unknown_placeholder() {
        let mut conn = test_db();
        seed_game(&mut conn, "Alice", "Bob", "Open");
        refresh_info_counts(&mut conn).unwrap();
        let c = db_counts(&mut conn).unwrap();
        assert_eq!((c.games, c.players, c.events), (1, 2, 1));
    }

    #[test]
    fn db_counts_count_rows_and_store_them_when_info_rows_are_missing() {
        let mut conn = test_db();
        seed_game(&mut conn, "Alice", "Bob", "Open");
        let c = db_counts(&mut conn).unwrap();
        assert_eq!((c.games, c.players, c.events), (1, 2, 1));
        assert_eq!(info_value(&mut conn, "GameCount").as_deref(), Some("1"));
    }

    #[test]
    fn db_counts_read_the_stored_values_instead_of_counting() {
        let mut conn = test_db();
        seed_game(&mut conn, "Alice", "Bob", "Open");
        for (n, v) in [
            ("GameCount", "7"),
            ("PlayerCount", "5"),
            ("EventCount", "3"),
        ] {
            set_info_value(&mut conn, n, v).unwrap();
        }
        let c = db_counts(&mut conn).unwrap();
        assert_eq!((c.games, c.players, c.events), (7, 4, 2));
    }
}
