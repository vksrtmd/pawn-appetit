use std::{
    fs::{File, OpenOptions},
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::PathBuf,
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

    fn offset_by_index(&mut self, n: usize, state: &AppState, file: &str) -> io::Result<()> {
        let offset_index = n / GAME_OFFSET_FREQ;
        let n_left = n % GAME_OFFSET_FREQ;

        if let Some(pgn_offsets) = state.pgn_offsets.get(file) {
            let offset = if offset_index == 0 {
                self.start
            } else if offset_index <= pgn_offsets.len() {
                pgn_offsets[offset_index - 1]
            } else {
                // If offset_index is out of bounds, start from beginning
                self.reader.seek(SeekFrom::Start(self.start))?;
                self.skip_games(n)?;
                return Ok(());
            };

            self.reader.seek(SeekFrom::Start(offset))?;
            self.skip_games(n_left)?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "PGN offsets not found for file",
            ));
        }

        Ok(())
    }

    /// Skip n games, and return the number of bytes read
    fn skip_games(&mut self, n: usize) -> io::Result<usize> {
        if n == 0 {
            return Ok(0);
        }

        let mut new_game = false;
        let mut skipped = 0;
        let mut count = 0;
        let mut line = String::new();
        let mut comments = CommentTracker::default();

        loop {
            line.clear();
            let bytes = self.reader.read_line(&mut line)?;
            skipped += bytes;

            if bytes == 0 {
                break;
            }

            if comments.feed(&line) {
                if new_game {
                    count += 1;
                    if count == n {
                        self.reader.seek(SeekFrom::Current(-(bytes as i64)))?;
                        break;
                    }
                    new_game = false;
                }
            } else {
                new_game = true;
            }
        }
        Ok(skipped)
    }

    fn read_game(&mut self) -> io::Result<String> {
        let mut new_game = false;
        let mut comments = CommentTracker::default();
        self.game.clear();
        self.line.clear();

        loop {
            let bytes = self.reader.read_line(&mut self.line)?;
            if bytes == 0 {
                break;
            }

            if comments.feed(&self.line) {
                if new_game {
                    break;
                }
            } else {
                new_game = true;
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

#[tauri::command]
#[specta::specta]
pub async fn count_pgn_games(
    file: PathBuf,
    state: tauri::State<'_, AppState>,
) -> Result<i32, Error> {
    let files_string = file.to_string_lossy().to_string();

    let file = File::open(&file)?;

    let mut parser = PgnParser::new(file.try_clone()?);

    let mut offsets = Vec::new();

    let mut count = 0;

    while let Ok(skipped) = parser.skip_games(1) {
        if skipped == 0 {
            break;
        }
        count += 1;
        if count % GAME_OFFSET_FREQ as i32 == 0 {
            let cur_pos = parser.position()?;
            offsets.push(cur_pos);
        }
    }

    state.pgn_offsets.insert(files_string, offsets);
    Ok(count)
}

#[tauri::command]
#[specta::specta]
pub async fn read_games(
    file: PathBuf,
    start: i32,
    end: i32,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>, Error> {
    let file_r = File::open(&file)?;
    let file_str = file.to_string_lossy();
    let mut parser = PgnParser::new(file_r);

    parser.offset_by_index(start as usize, &state, &file_str)?;

    let capacity = (end - start + 1).max(0) as usize;
    let mut games: Vec<String> = Vec::with_capacity(capacity);

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
pub async fn delete_game(
    file: PathBuf,
    n: i32,
    state: tauri::State<'_, AppState>,
) -> Result<(), Error> {
    let file_r = File::open(&file)?;

    let mut parser = PgnParser::new(file_r.try_clone()?);

    parser.offset_by_index(n as usize, &state, file.to_string_lossy().as_ref())?;

    let starting_bytes = parser.position()?;

    parser.skip_games(1)?;

    let mut file_w = OpenOptions::new().write(true).open(file)?;

    file_w.seek(SeekFrom::Start(starting_bytes))?;

    write_to_end(&mut parser.reader, &mut file_w)?;
    Ok(())
}

fn write_to_end<R: Read>(reader: &mut R, writer: &mut File) -> io::Result<()> {
    io::copy(reader, writer)?;
    let end = writer.stream_position()?;
    writer.set_len(end)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn write_game(
    file: PathBuf,
    n: i32,
    pgn: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), Error> {
    if !file.exists() {
        File::create(&file)?;
    }

    let file_r = File::open(&file)?;
    let mut file_w = OpenOptions::new().write(true).open(&file)?;

    let mut tmpf = tempfile::tempfile()?;
    io::copy(&mut file_r.try_clone()?, &mut tmpf)?;

    let mut parser = PgnParser::new(file_r.try_clone()?);

    parser.offset_by_index(n as usize, &state, file.to_string_lossy().as_ref())?;

    tmpf.seek(SeekFrom::Start(parser.position()?))?;
    tmpf.write_all(pgn.as_bytes())?;

    parser.skip_games(1)?;

    write_to_end(&mut parser.reader, &mut tmpf)?;

    tmpf.seek(SeekFrom::Start(0))?;

    write_to_end(&mut tmpf, &mut file_w)?;

    Ok(())
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
        let mut n = 0;
        while p.skip_games(1).unwrap() != 0 {
            n += 1;
        }
        n
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
}
