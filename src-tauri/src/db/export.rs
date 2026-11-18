use std::{
    fs::OpenOptions,
    io::{BufWriter, Write},
    path::PathBuf,
};

use diesel::{connection::DefaultLoadingMode, prelude::*};
use shakmaty::{fen::Fen, CastlingMode, Chess, FromSetup};

use crate::{
    db::{
        connection::{get_db_or_create, ConnectionOptions},
        models::{Event, Game, Player, Site},
        pgn::GameTree,
        schema::{events, games, players, sites},
    },
    error::Result,
    AppState,
};

struct PgnGame {
    time: Option<String>,
    end_date: Option<String>,
    end_time: Option<String>,
    event: Option<String>,
    site: Option<String>,
    date: Option<String>,
    round: Option<String>,
    white: Option<String>,
    black: Option<String>,
    result: Option<String>,
    time_control: Option<String>,
    eco: Option<String>,
    white_elo: Option<String>,
    black_elo: Option<String>,
    ply_count: Option<String>,
    fen: Option<String>,
    moves: String,
}

/// Unrated games (stored as 0) are written as `-`, the PGN convention.
fn format_rating_tag(rating: i32) -> String {
    if rating > 0 {
        rating.to_string()
    } else {
        "-".to_string()
    }
}

/// Escapes `\` and `"` the way a PGN header value requires.
fn escape_header(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

impl PgnGame {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        let tag = |writer: &mut dyn Write, name: &str, value: &str| -> Result<()> {
            writeln!(writer, "[{name} \"{}\"]", escape_header(value))?;
            Ok(())
        };
        tag(writer, "Event", self.event.as_deref().unwrap_or(""))?;
        tag(writer, "Site", self.site.as_deref().unwrap_or(""))?;
        tag(writer, "Date", self.date.as_deref().unwrap_or(""))?;
        tag(writer, "Round", self.round.as_deref().unwrap_or(""))?;
        tag(writer, "White", self.white.as_deref().unwrap_or(""))?;
        tag(writer, "Black", self.black.as_deref().unwrap_or(""))?;
        if let Some(time) = self.time.as_deref() {
            tag(writer, "UTCTime", time)?;
        }
        if let Some(end_date) = self.end_date.as_deref() {
            tag(writer, "EndDate", end_date)?;
        }
        if let Some(end_time) = self.end_time.as_deref() {
            tag(writer, "EndTime", end_time)?;
        }
        tag(writer, "Result", self.result.as_deref().unwrap_or("*"))?;
        if let Some(time_control) = self.time_control.as_deref() {
            tag(writer, "TimeControl", time_control)?;
        }
        if let Some(eco) = self.eco.as_deref() {
            tag(writer, "ECO", eco)?;
        }
        if let Some(white_elo) = self.white_elo.as_deref() {
            writeln!(writer, "[WhiteElo \"{}\"]", white_elo)?;
        }
        if let Some(black_elo) = self.black_elo.as_deref() {
            writeln!(writer, "[BlackElo \"{}\"]", black_elo)?;
        }
        if let Some(ply_count) = self.ply_count.as_deref() {
            writeln!(writer, "[PlyCount \"{}\"]", ply_count)?;
        }
        if let Some(fen) = self.fen.as_deref() {
            writeln!(writer, "[SetUp \"1\"]")?;
            tag(writer, "FEN", fen)?;
        }
        writeln!(writer)?;
        writer.write_all(self.moves.as_bytes())?;
        match self.result.as_deref() {
            Some("1-0") => writeln!(writer, "1-0"),
            Some("0-1") => writeln!(writer, "0-1"),
            Some("1/2-1/2") => writeln!(writer, "1/2-1/2"),
            _ => writeln!(writer, "*"),
        }?;
        writeln!(writer)?;
        Ok(())
    }
}

/// Writes every game of `db` as PGN. A row that cannot be read or a game
/// whose moves cannot be decoded stops the export with that error.
fn write_games(db: &mut SqliteConnection, writer: &mut impl Write) -> Result<()> {
    let (white_players, black_players) = diesel::alias!(players as white, players as black);
    let rows = games::table
        .inner_join(white_players.on(games::white_id.eq(white_players.field(players::id))))
        .inner_join(black_players.on(games::black_id.eq(black_players.field(players::id))))
        .inner_join(events::table.on(games::event_id.eq(events::id)))
        .inner_join(sites::table.on(games::site_id.eq(sites::id)))
        .load_iter::<(Game, Player, Player, Event, Site), DefaultLoadingMode>(db)?;
    for row in rows {
        let (game, white, black, event, site) = row?;
        let pgn = PgnGame {
            time: game.time,
            end_date: game.end_date,
            end_time: game.end_time,
            event: event.name,
            site: site.name,
            date: game.date,
            round: game.round,
            white: white.name,
            black: black.name,
            result: game.result,
            time_control: game.time_control,
            eco: game.eco,
            white_elo: game.white_elo.map(format_rating_tag),
            black_elo: game.black_elo.map(format_rating_tag),
            ply_count: game.ply_count.map(|e| e.to_string()),
            fen: game.fen.clone(),
            moves: GameTree::from_bytes(
                &game.moves,
                game.fen
                    .and_then(|fen| Fen::from_ascii(fen.as_bytes()).ok())
                    .and_then(|fen| Chess::from_setup(fen.into(), CastlingMode::Chess960).ok()),
            )?
            .to_string(),
        };
        pgn.write(writer)?;
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn export_to_pgn(
    file: PathBuf,
    dest_file: PathBuf,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    let db = &mut get_db_or_create(&state, file.to_str().unwrap(), ConnectionOptions::default())?;

    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(dest_file)?;

    let mut writer = BufWriter::new(file);
    write_games(db, &mut writer)?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{core, import::insert_to_db, pgn::Importer};
    use pgn_reader::BufferedReader;

    fn import(conn: &mut SqliteConnection, pgn: &str) {
        let mut reader = BufferedReader::new_cursor(pgn.as_bytes());
        let mut importer = Importer::new(None);
        while let Some(game) = reader.read_game(&mut importer).unwrap() {
            if let Some(game) = game {
                insert_to_db(conn, &game).unwrap();
            }
        }
    }

    fn exported(conn: &mut SqliteConnection) -> String {
        let mut bytes = Vec::new();
        write_games(conn, &mut bytes).unwrap();
        String::from_utf8(bytes).unwrap()
    }

    fn fresh() -> SqliteConnection {
        let mut conn = SqliteConnection::establish(":memory:").unwrap();
        core::init_db(&mut conn, "T", "D").unwrap();
        conn
    }

    #[test]
    fn exported_games_reimport_with_the_same_names_and_no_duplicate() {
        let mut conn = fresh();
        import(
            &mut conn,
            "[Event \"E\\\\x\"]\n[Date \"2026.09.09\"]\n[UTCTime \"11:05:14\"]\n[EndDate \"2026.09.10\"]\n[EndTime \"01:02:03\"]\n[White \"O\\\"Brien\"]\n[Black \"b\\\\c\"]\n\n1. e4 e5 *\n",
        );
        let pgn = exported(&mut conn);
        assert!(pgn.contains("[White \"O\\\"Brien\"]"), "{pgn}");
        assert!(pgn.contains("[Black \"b\\\\c\"]"), "{pgn}");
        assert!(pgn.contains("[UTCTime \"11:05:14\"]"), "{pgn}");
        assert!(pgn.contains("[EndDate \"2026.09.10\"]"), "{pgn}");
        assert!(pgn.contains("[EndTime \"01:02:03\"]"), "{pgn}");

        import(&mut conn, &pgn);
        let games: i64 = games::table.count().get_result(&mut conn).unwrap();
        assert_eq!(games, 1);
        let names: Vec<Option<String>> = players::table
            .filter(players::id.ne(0))
            .select(players::name)
            .order(players::name)
            .load(&mut conn)
            .unwrap();
        assert_eq!(
            names,
            vec![Some("O\"Brien".to_string()), Some("b\\c".to_string())]
        );
        let events: Vec<Option<String>> = events::table
            .filter(events::id.ne(0))
            .select(events::name)
            .load(&mut conn)
            .unwrap();
        assert_eq!(events, vec![Some("E\\x".to_string())]);
    }

    #[test]
    fn a_game_that_cannot_be_decoded_fails_the_export() {
        let mut conn = fresh();
        import(&mut conn, "[White \"a\"]\n[Black \"b\"]\n\n1. e4 e5 *\n");
        diesel::update(games::table)
            .set(games::moves.eq(vec![0xFFu8; 3]))
            .execute(&mut conn)
            .unwrap();
        let mut bytes = Vec::new();
        assert!(write_games(&mut conn, &mut bytes).is_err());
    }

    #[test]
    fn writes_core_tags_and_result() {
        let game = PgnGame {
            time: None,
            end_date: None,
            end_time: None,
            event: Some("Candidates".to_string()),
            site: Some("Toronto".to_string()),
            date: Some("2024.04.01".to_string()),
            round: Some("1".to_string()),
            white: Some("White".to_string()),
            black: Some("Black".to_string()),
            result: Some("1-0".to_string()),
            time_control: None,
            eco: Some("C42".to_string()),
            white_elo: Some("2750".to_string()),
            black_elo: None,
            ply_count: Some("3".to_string()),
            fen: None,
            moves: "1. e4 e5 2. Nf3 ".to_string(),
        };

        let mut bytes = Vec::new();
        game.write(&mut bytes).unwrap();
        let output = String::from_utf8(bytes).unwrap();

        assert!(output.contains("[Event \"Candidates\"]"));
        assert!(output.contains("[ECO \"C42\"]"));
        assert!(output.contains("1. e4 e5 2. Nf3 1-0"));
    }

    #[test]
    fn unrated_ratings_are_written_as_dash() {
        assert_eq!(format_rating_tag(0), "-");
        assert_eq!(format_rating_tag(-3), "-");
        assert_eq!(format_rating_tag(2750), "2750");
    }
}
