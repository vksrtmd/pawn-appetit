use std::{
    fs::File,
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use crate::{error::Error, AppState};

const GAME_OFFSET_FREQ: usize = 100;

/// Tracks whether a line scan is inside a `{ ... }` comment, so that a line
/// starting with `[` inside a comment is not mistaken for a header.
/// `{` is literal inside a comment, `}` only terminates one, and a `;` outside
/// a comment starts a comment that ends at the newline.
#[derive(Default)]
pub(crate) struct CommentTracker {
    in_comment: bool,
}

impl CommentTracker {
    /// Feeds one line; returns true when it is a header line (starts with `[`
    /// outside a comment). Header lines are not scanned: their quoted values
    /// may contain braces.
    pub(crate) fn feed(&mut self, line: &str) -> bool {
        if !self.in_comment && line.starts_with('[') {
            return true;
        }
        for c in line.chars() {
            match (self.in_comment, c) {
                (false, '{') => self.in_comment = true,
                (true, '}') => self.in_comment = false,
                (false, ';') => break,
                _ => {}
            }
        }
        false
    }
}

/// Finds where one game ends and the next begins. Blank and `%` escape lines
/// before the first header belong to no game.
#[derive(Default)]
struct GameSplitter {
    comments: CommentTracker,
    /// A header or movetext line has been seen in the current game.
    any: bool,
    /// A non-header line has been seen after a header (or movetext without one).
    body: bool,
}

impl GameSplitter {
    /// Feeds one line; returns true when it is the first header of a new game.
    fn feed(&mut self, line: &str) -> bool {
        if self.comments.feed(line) {
            let boundary = self.body;
            self.body = false;
            self.any = true;
            return boundary;
        }
        let t = line.trim();
        if self.any || (!t.is_empty() && !t.starts_with('%')) {
            self.any = true;
            self.body = true;
        }
        false
    }
}

struct PgnParser {
    reader: BufReader<File>,
    line: String,
    game: String,
    start: u64,
}

impl PgnParser {
    fn new(file: File) -> Self {
        let mut reader = BufReader::new(file);
        let start = ignore_bom(&mut reader).unwrap_or(0);
        Self {
            reader,
            line: String::new(),
            game: String::new(),
            start,
        }
    }

    fn position(&mut self) -> io::Result<u64> {
        self.reader.stream_position()
    }

    /// The one offset scan: reads the whole file from the start and returns
    /// the number of games and the byte offset of every `GAME_OFFSET_FREQ`th
    /// game.
    fn scan_offsets(&mut self) -> io::Result<(usize, Vec<u64>)> {
        self.reader.seek(SeekFrom::Start(self.start))?;
        let mut offsets = Vec::new();
        let mut count = 0;
        loop {
            let skipped = self.skip_games(GAME_OFFSET_FREQ)?;
            count += skipped;
            if skipped < GAME_OFFSET_FREQ {
                break;
            }
            offsets.push(self.position()?);
        }
        Ok((count, offsets))
    }

    /// Positions the reader at the start of game `n`, scanning the file for
    /// offsets when none are stored for it.
    fn offset_by_index(&mut self, n: usize, state: &AppState, file: &str) -> io::Result<()> {
        if !state.pgn_offsets.contains_key(file) {
            let (_, offsets) = self.scan_offsets()?;
            state.pgn_offsets.insert(file.to_string(), offsets);
        }
        let offset_index = n / GAME_OFFSET_FREQ;
        let stored = state.pgn_offsets.get(file).and_then(|o| {
            offset_index
                .checked_sub(1)
                .map_or(Some(self.start), |i| o.get(i).copied())
        });
        match stored {
            Some(offset) => {
                self.reader.seek(SeekFrom::Start(offset))?;
                self.skip_games(n % GAME_OFFSET_FREQ)?;
            }
            None => {
                self.reader.seek(SeekFrom::Start(self.start))?;
                self.skip_games(n)?;
            }
        }
        Ok(())
    }

    /// Skips up to n games and returns how many it skipped. Stops at the first
    /// header of the next game, or at the end of the file.
    fn skip_games(&mut self, n: usize) -> io::Result<usize> {
        if n == 0 {
            return Ok(0);
        }

        let mut count = 0;
        let mut splitter = GameSplitter::default();
        loop {
            self.line.clear();
            let bytes = self.reader.read_line(&mut self.line)?;
            if bytes == 0 {
                if splitter.any {
                    count += 1;
                }
                return Ok(count);
            }
            if splitter.feed(&self.line) {
                count += 1;
                if count == n {
                    self.reader.seek_relative(-(bytes as i64))?;
                    return Ok(count);
                }
            }
        }
    }

    fn read_game(&mut self) -> io::Result<String> {
        let mut splitter = GameSplitter::default();
        self.game.clear();
        self.line.clear();

        loop {
            let bytes = self.reader.read_line(&mut self.line)?;
            if bytes == 0 {
                break;
            }
            if splitter.feed(&self.line) {
                self.reader.seek_relative(-(bytes as i64))?;
                break;
            }
            self.game.push_str(&self.line);
            self.line.clear();
        }
        Ok(self.game.clone())
    }
}

fn ignore_bom(reader: &mut BufReader<File>) -> io::Result<u64> {
    let mut bom = [0; 3];
    reader.read_exact(&mut bom)?;
    if bom != [0xEF, 0xBB, 0xBF] {
        reader.seek(SeekFrom::Start(0))?;
        return Ok(0);
    }
    Ok(3)
}

/// Writes a new version of `file` through `fill` into a temp file in the same
/// folder, syncs it and renames it over the original, so a failure leaves the
/// original untouched.
fn rewrite_atomic(file: &Path, fill: impl FnOnce(&mut File) -> io::Result<()>) -> io::Result<()> {
    let dir = match file.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("."),
    };
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    fill(tmp.as_file_mut())?;
    tmp.as_file().sync_all()?;
    if let Ok(meta) = std::fs::metadata(file) {
        tmp.as_file().set_permissions(meta.permissions())?;
    }
    tmp.persist(file).map_err(|e| e.error)?;
    Ok(())
}

/// Replaces game `n` with `replacement`, or removes it when `None`.
fn splice_game(
    file: &Path,
    n: usize,
    replacement: Option<&str>,
    state: &AppState,
) -> io::Result<()> {
    if !file.exists() {
        File::create(file)?;
    }
    let mut parser = PgnParser::new(File::open(file)?);
    parser.offset_by_index(n, state, file.to_string_lossy().as_ref())?;
    let game_start = parser.position()?;
    parser.skip_games(1)?;

    let result = rewrite_atomic(file, |out| {
        let mut src = File::open(file)?;
        io::copy(&mut (&mut src).take(game_start), out)?;
        if let Some(pgn) = replacement {
            out.write_all(pgn.as_bytes())?;
        }
        io::copy(&mut parser.reader, out)?;
        Ok(())
    });
    // Offsets after the changed game moved; the next lookup rescans.
    state.pgn_offsets.remove(file.to_string_lossy().as_ref());
    result
}

fn count_games(file: &Path, state: &AppState) -> io::Result<usize> {
    let mut parser = PgnParser::new(File::open(file)?);
    let (count, offsets) = parser.scan_offsets()?;
    state
        .pgn_offsets
        .insert(file.to_string_lossy().to_string(), offsets);
    Ok(count)
}

fn read_range(file: &Path, start: usize, end: usize, state: &AppState) -> io::Result<Vec<String>> {
    let mut parser = PgnParser::new(File::open(file)?);
    parser.offset_by_index(start, state, file.to_string_lossy().as_ref())?;

    let mut games = Vec::with_capacity((end + 1).saturating_sub(start));
    for _ in start..=end {
        let game = parser.read_game()?;
        if game.is_empty() {
            break;
        }
        games.push(game);
    }
    Ok(games)
}

#[tauri::command]
#[specta::specta]
pub async fn count_pgn_games(
    file: PathBuf,
    state: tauri::State<'_, AppState>,
) -> Result<i32, Error> {
    Ok(count_games(&file, &state)? as i32)
}

#[tauri::command]
#[specta::specta]
pub async fn read_games(
    file: PathBuf,
    start: i32,
    end: i32,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>, Error> {
    Ok(read_games_between(&file, start, end, &state)?)
}

/// The games `start..=end` of `file`; none when the range is empty or starts below 0
/// (an empty file asks for `0..=-1`).
fn read_games_between(
    file: &Path,
    start: i32,
    end: i32,
    state: &AppState,
) -> io::Result<Vec<String>> {
    if start < 0 || end < start {
        return Ok(Vec::new());
    }
    read_range(file, start as usize, end as usize, state)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_game(
    file: PathBuf,
    n: i32,
    state: tauri::State<'_, AppState>,
) -> Result<(), Error> {
    Ok(splice_game(&file, n as usize, None, &state)?)
}

#[tauri::command]
#[specta::specta]
pub async fn write_game(
    file: PathBuf,
    n: i32,
    pgn: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), Error> {
    Ok(splice_game(&file, n as usize, Some(&pgn), &state)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAME_WITH_COMMENT: &str =
        "[Event \"A\"]\n\n1. e4 {first line\n[not a header]\nlast line} e5 *\n\n";
    const SECOND: &str = "[Event \"B\"]\n\n1. d4 d5 *\n\n";

    fn parser(content: &str) -> PgnParser {
        let mut f = tempfile::tempfile().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.seek(SeekFrom::Start(0)).unwrap();
        PgnParser::new(f)
    }

    fn count(content: &str) -> usize {
        let mut p = parser(content);
        p.scan_offsets().unwrap().0
    }

    fn read_first(content: &str) -> String {
        parser(content).read_game().unwrap()
    }

    #[test]
    fn counts_a_game_with_a_bracket_line_in_a_comment_as_one() {
        assert_eq!(count(GAME_WITH_COMMENT), 1);
    }

    #[test]
    fn reads_a_game_with_a_bracket_line_in_a_comment_intact() {
        assert_eq!(read_first(GAME_WITH_COMMENT), GAME_WITH_COMMENT);
    }

    #[test]
    fn still_splits_two_games_after_a_comment_with_a_bracket_line() {
        let both = format!("{GAME_WITH_COMMENT}{SECOND}");
        assert_eq!(count(&both), 2);
        assert_eq!(read_first(&both), GAME_WITH_COMMENT);
    }

    #[test]
    fn open_brace_inside_a_comment_is_literal() {
        let g = "[Event \"A\"]\n\n1. e4 {a { b\n[x]\n} e5 *\n\n";
        let both = format!("{g}{SECOND}");
        assert_eq!(count(&both), 2);
        assert_eq!(read_first(&both), g);
    }

    #[test]
    fn braces_after_a_semicolon_comment_do_not_open_a_comment() {
        let g = "[Event \"A\"]\n\n1. e4 ; note {\ne5 *\n\n";
        let both = format!("{g}{SECOND}");
        assert_eq!(count(&both), 2);
        assert_eq!(read_first(&both), g);
    }

    #[test]
    fn semicolon_inside_a_brace_comment_is_literal() {
        let g = "[Event \"A\"]\n\n1. e4 {a ; b\n[x]\n} e5 *\n\n";
        assert_eq!(count(g), 1);
        assert_eq!(read_first(g), g);
    }

    #[test]
    fn braces_in_header_values_do_not_open_a_comment() {
        let g = "[Event \"A {\"]\n\n1. e4 e5 *\n\n";
        let both = format!("{g}{SECOND}");
        assert_eq!(count(&both), 2);
    }

    fn games(n: usize) -> String {
        (0..n)
            .map(|i| format!("[Event \"G{i}\"]\n[Site \"S\"]\n\n1. e4 e5 {{c{i}}} *\n\n"))
            .collect()
    }

    fn game_n(i: usize) -> String {
        games(i + 1).split_off(games(i).len())
    }

    fn pgn_file(content: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.pgn");
        std::fs::write(&path, content).unwrap();
        (dir, path)
    }

    #[test]
    fn writes_game_zero_to_a_file_never_counted() {
        let (_d, path) = pgn_file("");
        let state = AppState::default();
        splice_game(&path, 0, Some(&game_n(0)), &state).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), game_n(0));
    }

    #[test]
    fn an_empty_range_reads_no_games() {
        let (_d, path) = pgn_file("");
        let state = AppState::default();
        assert!(read_games_between(&path, 0, -1, &state).unwrap().is_empty());
        assert!(read_games_between(&path, 0, -5, &state).unwrap().is_empty());
        let (_d, path) = pgn_file(&games(3));
        assert!(read_games_between(&path, 2, 1, &state).unwrap().is_empty());
        assert_eq!(
            read_games_between(&path, 1, 1, &state).unwrap(),
            vec![game_n(1)]
        );
    }

    #[test]
    fn reads_from_a_file_never_counted() {
        let (_d, path) = pgn_file(&games(250));
        let got = read_range(&path, 150, 150, &AppState::default()).unwrap();
        assert_eq!(got, vec![game_n(150)]);
    }

    #[test]
    fn delete_then_read_and_save_hit_the_right_game_in_300() {
        let (_d, path) = pgn_file(&games(300));
        let state = AppState::default();
        assert_eq!(count_games(&path, &state).unwrap(), 300);
        splice_game(&path, 5, None, &state).unwrap();
        // Game 150 now is the original game 151.
        assert_eq!(
            read_range(&path, 150, 150, &state).unwrap(),
            vec![game_n(151)]
        );
        let replacement = "[Event \"New\"]\n\n1. d4 *\n\n";
        splice_game(&path, 150, Some(replacement), &state).unwrap();
        let mut expected: Vec<String> = (0..300).map(game_n).collect();
        expected.remove(5);
        expected[150] = replacement.to_string();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), expected.concat());
        assert_eq!(count_games(&path, &state).unwrap(), 299);
    }

    #[test]
    fn a_failed_rewrite_leaves_the_original_and_no_temp_file() {
        let original = games(3);
        let (dir, path) = pgn_file(&original);
        let err = rewrite_atomic(&path, |out| {
            out.write_all(b"partial")?;
            Err(io::Error::other("disk full"))
        });
        assert!(err.is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn a_file_starting_with_a_blank_line_counts_one_game_and_game_zero_is_it() {
        for lead in ["\n", "%escape\n\n"] {
            let g = format!("{lead}{}", game_n(0));
            assert_eq!(count(&g), 1);
            let (_d, path) = pgn_file(&g);
            let state = AppState::default();
            assert_eq!(read_range(&path, 0, 0, &state).unwrap(), vec![g.clone()]);
            splice_game(&path, 0, Some("[Event \"N\"]\n\n*\n\n"), &state).unwrap();
            assert_eq!(count_games(&path, &state).unwrap(), 1);
        }
    }

    #[test]
    fn games_after_the_first_keep_their_first_header_line() {
        let (_d, path) = pgn_file(&games(3));
        let got = read_range(&path, 0, 2, &AppState::default()).unwrap();
        assert_eq!(got, vec![game_n(0), game_n(1), game_n(2)]);
    }

    #[test]
    fn counts_header_only_games_separately() {
        assert_eq!(count("[A \"1\"]\n\n[A \"2\"]\n\n"), 2);
    }
}
