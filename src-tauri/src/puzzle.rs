use std::{
    collections::VecDeque,
    fs::File,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    sync::Mutex,
};

use diesel::{
    connection::SimpleConnection, insert_into, BoolExpressionMethods, Connection,
    ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl,
};
use once_cell::sync::Lazy;
use rand::Rng;
use serde::Serialize;
use specta::Type;
use tauri::{path::BaseDirectory, Emitter, Manager};

use crate::{
    db::{puzzles, Puzzle},
    error::Error,
};

/// What a cached batch of puzzles was loaded for. A call with another key
/// discards the batch.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PuzzleKey {
    file: String,
    min_rating: u16,
    max_rating: u16,
    random: bool,
}

/// Cache for puzzles to reduce database queries
#[derive(Debug)]
struct PuzzleCache {
    /// Puzzles loaded and not served yet
    cache: VecDeque<Puzzle>,
    /// What `cache` and `after` belong to
    key: Option<PuzzleKey>,
    /// In-order mode: the (rating, id) of the last puzzle loaded; the next
    /// batch starts after it
    after: Option<(i32, i32)>,
    /// Maximum number of puzzles to cache at once
    cache_size: usize,
}

impl PuzzleCache {
    /// Create a new puzzle cache with default settings
    fn new() -> Self {
        Self {
            cache: VecDeque::new(),
            key: None,
            after: None,
            cache_size: 20,
        }
    }

    /// The next puzzle for `file` within the rating range.
    ///
    /// In random mode puzzles are drawn at random. In order mode they come by
    /// rating, then id, continuing where the last batch ended and starting
    /// over after the last puzzle. The batch is reloaded when it runs out or
    /// when the file, the rating range or the mode changes.
    ///
    /// Returns `Error::NoPuzzles` when nothing matches.
    fn next(
        &mut self,
        file: &str,
        min_rating: u16,
        max_rating: u16,
        random: bool,
    ) -> Result<Puzzle, Error> {
        let key = PuzzleKey {
            file: file.to_string(),
            min_rating,
            max_rating,
            random,
        };
        if self.key.as_ref() != Some(&key) {
            self.cache.clear();
            self.after = None;
            self.key = Some(key);
        }
        if self.cache.is_empty() {
            let mut db = diesel::SqliteConnection::establish(file)?;
            let batch = if random {
                self.load_random(&mut db, min_rating, max_rating)?
            } else {
                self.load_in_order(&mut db, min_rating, max_rating)?
            };
            self.cache = batch.into();
        }
        self.cache.pop_front().ok_or(Error::NoPuzzles)
    }

    fn in_range(
        min_rating: u16,
        max_rating: u16,
    ) -> puzzles::BoxedQuery<'static, diesel::sqlite::Sqlite> {
        puzzles::table
            .filter(puzzles::rating.le(max_rating as i32))
            .filter(puzzles::rating.ge(min_rating as i32))
            .into_boxed()
    }

    fn load_in_order(
        &mut self,
        db: &mut diesel::SqliteConnection,
        min_rating: u16,
        max_rating: u16,
    ) -> Result<Vec<Puzzle>, Error> {
        let limit = self.cache_size as i64;
        let mut load = |after: Option<(i32, i32)>| -> Result<Vec<Puzzle>, Error> {
            let mut query = Self::in_range(min_rating, max_rating);
            if let Some((rating, id)) = after {
                query = query.filter(
                    puzzles::rating
                        .gt(rating)
                        .or(puzzles::rating.eq(rating).and(puzzles::id.gt(id))),
                );
            }
            Ok(query
                .order_by(puzzles::rating.asc())
                .then_order_by(puzzles::id.asc())
                .limit(limit)
                .load::<Puzzle>(db)?)
        };
        let mut batch = load(self.after)?;
        if batch.is_empty() && self.after.is_some() {
            batch = load(None)?;
        }
        self.after = batch.last().map(|p| (p.rating, p.id));
        Ok(batch)
    }

    /// Probes random ids instead of sorting the whole table by `RANDOM()`:
    /// each probe takes the first matching puzzle at or after a random id,
    /// wrapping to the start.
    fn load_random(
        &self,
        db: &mut diesel::SqliteConnection,
        min_rating: u16,
        max_rating: u16,
    ) -> Result<Vec<Puzzle>, Error> {
        use diesel::dsl::{max, min};
        let (low, high): (Option<i32>, Option<i32>) = puzzles::table
            .select((min(puzzles::id), max(puzzles::id)))
            .first(db)?;
        let (Some(low), Some(high)) = (low, high) else {
            return Ok(Vec::new());
        };
        let mut rng = rand::thread_rng();
        let mut batch: Vec<Puzzle> = Vec::new();
        for _ in 0..self.cache_size {
            let start = rng.gen_range(low..=high);
            let first = |db: &mut diesel::SqliteConnection, from: Option<i32>| {
                let mut query = Self::in_range(min_rating, max_rating);
                if let Some(from) = from {
                    query = query.filter(puzzles::id.ge(from));
                }
                query
                    .order_by(puzzles::id.asc())
                    .first::<Puzzle>(db)
                    .optional()
            };
            let found = match first(db, Some(start))? {
                Some(p) => Some(p),
                None => first(db, None)?,
            };
            match found {
                Some(p) if !batch.iter().any(|q| q.id == p.id) => batch.push(p),
                Some(_) => {}
                None => break,
            }
        }
        Ok(batch)
    }
}

/// Gets a puzzle from the database within the specified rating range
///
/// A cache keeps a batch of puzzles per file, rating range and mode, so most
/// calls do not touch the database. It runs on a blocking thread.
///
/// # Arguments
/// * `file` - Path to the puzzle database
/// * `min_rating` - Minimum puzzle rating to include
/// * `max_rating` - Maximum puzzle rating to include
/// * `random` - Pick puzzles at random; otherwise serve them in order
///
/// # Returns
/// * `Ok(Puzzle)` if a puzzle was found
/// * `Err(Error::NoPuzzles)` if no puzzles match the criteria
/// * Other errors if there was a problem accessing the database
#[tauri::command]
#[specta::specta]
pub async fn get_puzzle(
    file: String,
    min_rating: u16,
    max_rating: u16,
    random: bool,
) -> Result<Puzzle, Error> {
    static PUZZLE_CACHE: Lazy<Mutex<PuzzleCache>> = Lazy::new(|| Mutex::new(PuzzleCache::new()));

    tauri::async_runtime::spawn_blocking(move || {
        let mut cache = PUZZLE_CACHE
            .lock()
            .map_err(|e| Error::MutexLockFailed(format!("Failed to lock puzzle cache: {}", e)))?;
        cache.next(&file, min_rating, max_rating, random)
    })
    .await
    .map_err(|e| Error::IoDetail(std::io::Error::other(e)))?
}

/// Gets the minimum and maximum rating range from a puzzle database
///
/// This function queries the database to find the lowest and highest puzzle ratings.
///
/// # Arguments
/// * `file` - Path to the puzzle database
///
/// # Returns
/// * `Ok((min_rating, max_rating))` with the rating range
/// * `Err(Error)` if there was a problem accessing the database
#[tauri::command]
#[specta::specta]
pub fn get_puzzle_rating_range(file: String) -> Result<(u16, u16), Error> {
    let mut db = diesel::SqliteConnection::establish(&file)?;

    let min_rating = puzzles::table
        .select(diesel::dsl::min(puzzles::rating))
        .first::<Option<i32>>(&mut db)?
        .unwrap_or(0) as u16;

    let max_rating = puzzles::table
        .select(diesel::dsl::max(puzzles::rating))
        .first::<Option<i32>>(&mut db)?
        .unwrap_or(0) as u16;

    Ok((min_rating, max_rating))
}

/// Information about a puzzle database
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PuzzleDatabaseInfo {
    /// The title of the puzzle database (derived from filename)
    title: String,
    /// Description of the puzzle database (currently not populated)
    /// TODO: Consider adding a way to store and retrieve database descriptions
    description: String,
    /// Number of puzzles in the database
    puzzle_count: i32,
    /// Size of the database file in bytes
    storage_size: i64,
    /// Full path to the database file
    path: String,
}

/// Gets information about a puzzle database
///
/// This function retrieves metadata about a puzzle database, including:
/// - The title (derived from the filename)
/// - The number of puzzles in the database
/// - The size of the database file
/// - The full path to the database file
///
/// # Arguments
/// * `file` - Relative path to the puzzle database within the app's data directory
/// * `app` - Tauri app handle used to resolve the full path
///
/// # Returns
/// * `Ok(PuzzleDatabaseInfo)` with the database information
/// * `Err(Error)` if there was a problem accessing the database or file
#[tauri::command]
#[specta::specta]
pub async fn get_puzzle_db_info(
    file: PathBuf,
    app: tauri::AppHandle,
) -> Result<PuzzleDatabaseInfo, Error> {
    // Ensure we're working with a relative path by checking if it's absolute
    let file_path = if file.is_absolute() {
        // If it's already absolute, use it directly
        file
    } else {
        // Otherwise, resolve it relative to the db directory in AppData
        let db_path = PathBuf::from("puzzles").join(file);
        app.path().resolve(db_path, BaseDirectory::AppData)?
    };

    let mut db = diesel::SqliteConnection::establish(&file_path.to_string_lossy())?;

    // Check if the puzzles table exists and get count safely
    let puzzle_count = match puzzles::table.count().get_result::<i64>(&mut db) {
        Ok(count) => count as i32,
        Err(diesel::result::Error::DatabaseError(kind, info)) => {
            // Check if the error is related to missing table
            if info.message().contains("no such table") {
                // Table doesn't exist - this could be an uninitialized database file
                // For safety, we return 0 instead of auto-initializing
                0
            } else {
                // For other database errors, propagate them
                return Err(Error::from(diesel::result::Error::DatabaseError(
                    kind, info,
                )));
            }
        }
        Err(e) => {
            // For other errors, propagate them
            return Err(Error::from(e));
        }
    };

    let storage_size = file_path.metadata()?.len() as i64;
    let filename = file_path
        .file_name()
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid path: no filename",
            )
        })?
        .to_string_lossy();

    Ok(PuzzleDatabaseInfo {
        title: filename.to_string(),
        description: "".to_string(),
        puzzle_count,
        storage_size,
        path: file_path.to_string_lossy().to_string(),
    })
}

/// Imports puzzles from a local file into a new puzzle database
///
/// This function can handle different types of puzzle files:
/// - PGN files containing puzzles (with FEN positions and solution moves)
/// - Existing puzzle database files (.db, .db3)
/// - Compressed files (.zst)
///
/// # Arguments
/// * `source_file` - Path to the source puzzle file
/// * `db_path` - Path where the new puzzle database should be created
/// * `title` - Title for the puzzle database
/// * `description` - Optional description for the puzzle database
/// * `app` - Tauri app handle for progress events
///
/// # Returns
/// * `Ok(())` if import was successful
/// * `Err(Error)` if there was a problem importing the file
#[tauri::command]
#[specta::specta]
pub async fn import_puzzle_file(
    source_file: PathBuf,
    db_path: PathBuf,
    title: String,
    description: Option<String>,
    app: tauri::AppHandle,
) -> Result<(), Error> {
    let description = description.unwrap_or_default();

    // Check if source file exists
    if !source_file.exists() {
        return Err(Error::IoDetail(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Source file not found",
        )));
    }

    // Create parent directory for the database if it doesn't exist
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let extension = source_file.extension().and_then(|ext| ext.to_str());

    match extension {
        Some("db") | Some("db3") => {
            // Copy existing puzzle database
            copy_puzzle_database(&source_file, &db_path, &title, &description).await
        }
        Some("pgn") | Some("zst") => {
            import_puzzles_from_pgn(&source_file, &db_path, &title, &description, &app).await
        }
        _ => Err(Error::UnsupportedFileFormat(format!(
            "Unsupported file format: {:?}",
            extension
        ))),
    }
}

/// Copies an existing puzzle database to a new location
async fn copy_puzzle_database(
    source_file: &PathBuf,
    db_path: &Path,
    _title: &str,
    _description: &str,
) -> Result<(), Error> {
    // Copy the source database file to the destination path
    std::fs::copy(source_file, db_path).map_err(|e| {
        Error::IoDetail(std::io::Error::new(
            e.kind(),
            format!("Failed to copy database: {}", e),
        ))
    })?;
    Ok(())
}

/// Imports puzzles from a PGN file, or a zstd-compressed one.
async fn import_puzzles_from_pgn(
    source_file: &PathBuf,
    db_path: &Path,
    title: &str,
    description: &str,
    app: &tauri::AppHandle,
) -> Result<(), Error> {
    let file = File::open(source_file).map_err(|e| {
        Error::IoDetail(std::io::Error::new(
            e.kind(),
            format!("Failed to open file '{}': {}", source_file.display(), e),
        ))
    })?;
    let reader: Box<dyn Read> = if source_file.extension().and_then(|e| e.to_str()) == Some("zst") {
        Box::new(zstd::Decoder::new(file)?)
    } else {
        Box::new(file)
    };
    import_puzzles(reader, db_path, title, description, |processed| {
        let _ = app.emit("import_puzzle_progress", processed);
    })
    .map_err(|e| match e {
        Error::NoPuzzles => Error::IoDetail(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("No valid puzzles found in file '{}'", source_file.display()),
        )),
        e => e,
    })
}

/// Number of puzzles inserted per transaction.
const IMPORT_BATCH_SIZE: usize = 1000;

/// Reads puzzles from `reader` into a new puzzle database at `db_path`,
/// a batch at a time, and calls `on_progress` with the count inserted after
/// each batch. A failure, or a file without puzzles (`Error::NoPuzzles`),
/// removes the database this call created.
fn import_puzzles(
    reader: impl Read,
    db_path: &Path,
    title: &str,
    description: &str,
    mut on_progress: impl FnMut(usize),
) -> Result<(), Error> {
    let existed = db_path.exists();
    let result = (|| {
        create_puzzle_database(db_path, title, description)?;
        let mut db = diesel::SqliteConnection::establish(&db_path.to_string_lossy())?;
        let mut batch: Vec<NewPuzzle> = Vec::with_capacity(IMPORT_BATCH_SIZE);
        let mut inserted = 0;
        let mut flush = |batch: &mut Vec<NewPuzzle>| -> Result<(), Error> {
            db.transaction::<_, Error, _>(|db| {
                for puzzle in batch.iter() {
                    insert_into(puzzles::table).values(puzzle).execute(db)?;
                }
                Ok(())
            })?;
            inserted += batch.len();
            batch.clear();
            on_progress(inserted);
            Ok(())
        };
        for_each_puzzle(reader, |puzzle| {
            batch.push(puzzle);
            if batch.len() == IMPORT_BATCH_SIZE {
                flush(&mut batch)?;
            }
            Ok(())
        })?;
        if !batch.is_empty() {
            flush(&mut batch)?;
        }
        if inserted == 0 {
            return Err(Error::NoPuzzles);
        }
        Ok(())
    })();
    if result.is_err() && !existed {
        let _ = std::fs::remove_file(db_path);
    }
    result
}

/// Creates a new puzzle database with the proper schema
fn create_puzzle_database(db_path: &Path, _title: &str, _description: &str) -> Result<(), Error> {
    let mut db = diesel::SqliteConnection::establish(&db_path.to_string_lossy())?;

    // Load the schema from external SQL files
    const PUZZLES_TABLES: &str = include_str!("../../database/schema/puzzles_tables.sql");
    const PUZZLES_INDEXES: &str = include_str!("../../database/indexes/puzzles_indexes.sql");

    // Create the puzzles table using the external schema
    db.batch_execute(PUZZLES_TABLES)?;

    // Create the indexes
    db.batch_execute(PUZZLES_INDEXES)?;

    Ok(())
}

/// Ensures that a database file has the proper puzzle schema initialized
///
/// This function checks if the puzzles table exists and creates it if missing.
/// This is useful for validating and repairing database files that may be
/// empty or corrupted.
///
/// # Arguments
/// * `db_path` - Path to the database file
///
/// # Returns
/// * `Ok(())` if the schema exists or was successfully created
/// * `Err(Error)` if there was a problem initializing the schema
#[allow(dead_code)]
fn ensure_puzzle_schema(db_path: &Path) -> Result<(), Error> {
    let mut db = diesel::SqliteConnection::establish(&db_path.to_string_lossy())?;

    // Check if puzzles table exists by trying to query it
    match puzzles::table.count().get_result::<i64>(&mut db) {
        Ok(_) => {
            // Table exists and is queryable
            Ok(())
        }
        Err(diesel::result::Error::DatabaseError(kind, info)) => {
            // Check if the error is related to missing table
            if info.message().contains("no such table") {
                // Table doesn't exist, create it
                const PUZZLES_TABLES: &str =
                    include_str!("../../database/schema/puzzles_tables.sql");
                const PUZZLES_INDEXES: &str =
                    include_str!("../../database/indexes/puzzles_indexes.sql");

                db.batch_execute(PUZZLES_TABLES)?;
                db.batch_execute(PUZZLES_INDEXES)?;
                Ok(())
            } else {
                // Other database error
                Err(Error::from(diesel::result::Error::DatabaseError(
                    kind, info,
                )))
            }
        }
        Err(e) => {
            // Other database error
            Err(Error::from(e))
        }
    }
}

/// Reads a PGN stream line by line and passes each complete puzzle to `found`.
/// Invalid UTF-8 is replaced, not rejected.
fn for_each_puzzle<R: Read>(
    reader: R,
    mut found: impl FnMut(NewPuzzle) -> Result<(), Error>,
) -> Result<(), Error> {
    let mut reader = BufReader::new(reader);
    let mut current_puzzle = NewPuzzle::default();
    let mut in_puzzle = false;
    let mut comments = crate::pgn::CommentTracker::default();
    let mut raw = Vec::new();

    loop {
        raw.clear();
        if reader.read_until(b'\n', &mut raw)? == 0 {
            break;
        }
        let content = String::from_utf8_lossy(&raw);
        let line = content.trim();

        if line.is_empty() {
            if in_puzzle && current_puzzle.is_complete() {
                found(std::mem::take(&mut current_puzzle))?;
                in_puzzle = false;
            }
            continue;
        }

        let is_header = comments.feed(line);

        if is_header && line.ends_with(']') {
            // Parse PGN headers
            if let Some((key, value)) = parse_pgn_header(line) {
                match key.as_str() {
                    "FEN" => {
                        current_puzzle.fen = value;
                        in_puzzle = true;
                    }
                    "Solution" | "Moves" => {
                        current_puzzle.moves = value;
                    }
                    "Rating" | "Elo" => {
                        if let Ok(rating) = value.parse::<i32>() {
                            current_puzzle.rating = rating;
                        }
                    }
                    "Popularity" => {
                        if let Ok(popularity) = value.parse::<i32>() {
                            current_puzzle.popularity = popularity;
                        }
                    }
                    "NbPlays" => {
                        if let Ok(nb_plays) = value.parse::<i32>() {
                            current_puzzle.nb_plays = nb_plays;
                        }
                    }
                    _ => {}
                }
            }
        } else if !is_header && in_puzzle && current_puzzle.moves.is_empty() {
            // If we have a non-header line and no moves yet, treat it as moves
            current_puzzle.moves = line.to_string();
        }
    }

    // The last puzzle, if complete
    if in_puzzle && current_puzzle.is_complete() {
        found(current_puzzle)?;
    }
    Ok(())
}

/// Parses a PGN header line and returns the key-value pair
fn parse_pgn_header(line: &str) -> Option<(String, String)> {
    if !line.starts_with('[') || !line.ends_with(']') {
        return None;
    }

    let content = &line[1..line.len() - 1];
    let mut parts = content.splitn(2, ' ');

    let key = parts.next()?.to_string();
    let value = parts.next()?;

    // Remove quotes if present
    let value = if value.starts_with('"') && value.ends_with('"') {
        &value[1..value.len() - 1]
    } else {
        value
    };

    Some((key, value.to_string()))
}

/// Represents a new puzzle to be inserted into the database
#[derive(diesel::Insertable, Default)]
#[diesel(table_name = puzzles)]
struct NewPuzzle {
    fen: String,
    moves: String,
    rating: i32,
    rating_deviation: i32,
    popularity: i32,
    nb_plays: i32,
}

impl NewPuzzle {
    fn is_complete(&self) -> bool {
        !self.fen.is_empty() && !self.moves.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(pgn: &str) -> Vec<NewPuzzle> {
        let mut found = Vec::new();
        for_each_puzzle(pgn.as_bytes(), |p| {
            found.push(p);
            Ok(())
        })
        .unwrap();
        found
    }

    #[test]
    fn bracket_line_inside_a_comment_is_not_a_puzzle_header() {
        let pgn = "[FEN \"8/8/8/8/8/8/8/K6k w - - 0 1\"]\n[Moves \"a1a2\"]\n[Rating \"1500\"]\n{note\n[Rating \"9\"]\n}\n";
        let puzzles = parse(pgn);
        assert_eq!(puzzles.len(), 1);
        assert_eq!(puzzles[0].rating, 1500);
    }

    /// `(rating, id)` pairs inserted in id order, in a new database file.
    fn puzzle_db(dir: &Path, name: &str, ratings: &[i32]) -> String {
        let path = dir.join(name);
        create_puzzle_database(&path, "", "").unwrap();
        let mut db = diesel::SqliteConnection::establish(path.to_str().unwrap()).unwrap();
        for rating in ratings {
            insert_into(puzzles::table)
                .values(NewPuzzle {
                    fen: "8/8/8/8/8/8/8/K6k w - - 0 1".to_string(),
                    moves: "a1a2".to_string(),
                    rating: *rating,
                    ..Default::default()
                })
                .execute(&mut db)
                .unwrap();
        }
        path.to_str().unwrap().to_string()
    }

    fn take(cache: &mut PuzzleCache, file: &str, random: bool, n: usize) -> Vec<i32> {
        (0..n)
            .map(|_| cache.next(file, 0, 3000, random).unwrap().id)
            .collect()
    }

    #[test]
    fn in_order_refills_move_forward() {
        let dir = tempfile::tempdir().unwrap();
        let file = puzzle_db(dir.path(), "p.db", &[1000; 45]);
        let mut cache = PuzzleCache::new();
        let first = take(&mut cache, &file, false, 20);
        let second = take(&mut cache, &file, false, 20);
        assert_eq!(first, (1..=20).collect::<Vec<_>>());
        assert_eq!(second, (21..=40).collect::<Vec<_>>());
        // The rest, then back to the start.
        assert_eq!(
            take(&mut cache, &file, false, 6),
            vec![41, 42, 43, 44, 45, 1]
        );
    }

    #[test]
    fn in_order_serves_by_rating_then_id() {
        let dir = tempfile::tempdir().unwrap();
        let file = puzzle_db(dir.path(), "p.db", &[1500, 1200, 1500, 1200]);
        let mut cache = PuzzleCache::new();
        assert_eq!(take(&mut cache, &file, false, 4), vec![2, 4, 1, 3]);
    }

    #[test]
    fn switching_the_puzzle_file_switches_the_puzzles() {
        let dir = tempfile::tempdir().unwrap();
        let small = puzzle_db(dir.path(), "a.db", &[1000; 2]);
        let big = puzzle_db(dir.path(), "b.db", &[2000; 5]);
        let mut cache = PuzzleCache::new();
        for random in [false, true] {
            assert!(cache.next(&small, 0, 3000, random).unwrap().rating == 1000);
            assert!(cache.next(&big, 0, 3000, random).unwrap().rating == 2000);
        }
    }

    #[test]
    fn random_mode_serves_puzzles_in_range_and_none_when_empty() {
        let dir = tempfile::tempdir().unwrap();
        let file = puzzle_db(dir.path(), "p.db", &[1000, 2000, 1000, 2000, 3000]);
        let mut cache = PuzzleCache::new();
        for _ in 0..50 {
            let p = cache.next(&file, 1500, 2500, true).unwrap();
            assert_eq!(p.rating, 2000);
        }
        assert!(matches!(
            cache.next(&file, 5000, 6000, true),
            Err(Error::NoPuzzles)
        ));
        let empty = puzzle_db(dir.path(), "e.db", &[]);
        assert!(matches!(
            cache.next(&empty, 0, 3000, true),
            Err(Error::NoPuzzles)
        ));
    }

    fn pgn_of(count: usize) -> String {
        (0..count)
            .map(|i| {
                format!(
                    "[FEN \"8/8/8/8/8/8/8/K6k w - - 0 1\"]\n[Moves \"a1a2\"]\n[Rating \"{}\"]\n\n",
                    1000 + i
                )
            })
            .collect()
    }

    #[test]
    fn import_inserts_in_batches() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("p.db");
        let mut progress = Vec::new();
        import_puzzles(pgn_of(2500).as_bytes(), &db_path, "", "", |n| {
            progress.push(n)
        })
        .unwrap();
        assert_eq!(progress, vec![1000, 2000, 2500]);
        let mut db = diesel::SqliteConnection::establish(db_path.to_str().unwrap()).unwrap();
        assert_eq!(
            puzzles::table.count().get_result::<i64>(&mut db).unwrap(),
            2500
        );
    }

    /// Serves `data`, then fails.
    struct BrokenAfter(std::io::Cursor<Vec<u8>>);
    impl Read for BrokenAfter {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            match self.0.read(buf)? {
                0 => Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "cut",
                )),
                n => Ok(n),
            }
        }
    }

    #[test]
    fn a_failed_import_removes_the_half_built_database() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("p.db");
        let reader = BrokenAfter(std::io::Cursor::new(pgn_of(1500).into_bytes()));
        assert!(import_puzzles(reader, &db_path, "", "", |_| {}).is_err());
        assert!(!db_path.exists());
    }

    #[test]
    fn a_failed_import_keeps_a_database_that_was_already_there() {
        let dir = tempfile::tempdir().unwrap();
        let file = puzzle_db(dir.path(), "p.db", &[1000]);
        let reader = BrokenAfter(std::io::Cursor::new(Vec::new()));
        assert!(import_puzzles(reader, Path::new(&file), "", "", |_| {}).is_err());
        assert!(Path::new(&file).exists());
    }

    #[test]
    fn a_file_without_puzzles_removes_the_database() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("p.db");
        let result = import_puzzles("nothing\n".as_bytes(), &db_path, "", "", |_| {});
        assert!(matches!(result, Err(Error::NoPuzzles)));
        assert!(!db_path.exists());
    }
}
