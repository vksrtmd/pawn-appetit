//! The one reader of the stored move format.
//!
//! A game's moves are stored as one byte per move (an index into the legal
//! moves of the position), interleaved with markers: a NAG is `NAG, value`, a
//! comment is `COMMENT, u64 big-endian length, UTF-8 bytes`, and a variation is
//! wrapped in `START_VARIATION ... END_VARIATION`. [`MoveBlobCursor`] walks
//! that, checking every length, so the tree decoder, position search and the
//! position index treat a malformed blob the same way: as an error.

use crate::error::{Error, Result};
use shakmaty::{Chess, Move, Position};

pub(super) const START_VARIATION: u8 = 254;
pub(super) const END_VARIATION: u8 = 253;
pub(super) const COMMENT: u8 = 252;
pub(super) const NAG: u8 = 251;

/// One element of a move blob.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Item<'a> {
    /// A move, already played on the cursor's position.
    Move(Move),
    Nag(u8),
    /// The raw comment bytes, not yet checked as UTF-8.
    Comment(&'a [u8]),
    StartVariation,
    EndVariation,
}

pub(super) struct MoveBlobCursor<'a> {
    bytes: &'a [u8],
    index: usize,
    position: Chess,
}

fn malformed(what: &str) -> Error {
    Error::MalformedMoveBlob(what.to_string())
}

impl<'a> MoveBlobCursor<'a> {
    pub(super) fn new(bytes: &'a [u8], start_position: Chess) -> Self {
        Self {
            bytes,
            index: 0,
            position: start_position,
        }
    }

    /// The position after the moves read so far.
    pub(super) fn position(&self) -> &Chess {
        &self.position
    }

    /// Continue from `position`, as when a variation branches off an earlier one.
    pub(super) fn set_position(&mut self, position: Chess) {
        self.position = position;
    }

    /// The next item, or `None` at the end of the blob.
    pub(super) fn next_item(&mut self) -> Result<Option<Item<'a>>> {
        let Some(&byte) = self.bytes.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        Ok(Some(match byte {
            NAG => Item::Nag(self.take(1, "NAG without a value")?[0]),
            COMMENT => {
                let length = self.take(8, "comment without a length")?;
                let length = u64::from_be_bytes(length.try_into().expect("took 8 bytes"));
                let length = usize::try_from(length)
                    .map_err(|_| malformed("comment length past the end"))?;
                Item::Comment(self.take(length, "comment length past the end")?)
            }
            START_VARIATION => Item::StartVariation,
            END_VARIATION => Item::EndVariation,
            move_byte => {
                let chess_move = self
                    .position
                    .legal_moves()
                    .get(move_byte as usize)
                    .cloned()
                    .ok_or_else(|| malformed("move byte out of range"))?;
                self.position.play_unchecked(&chess_move);
                Item::Move(chess_move)
            }
        }))
    }

    /// The next main-line move, skipping NAGs, comments and whole variations.
    /// `None` at the end of the blob or of the enclosing variation.
    pub(super) fn next_main_move(&mut self) -> Result<Option<Move>> {
        loop {
            match self.next_item()? {
                Some(Item::Move(m)) => return Ok(Some(m)),
                Some(Item::StartVariation) => self.skip_variation()?,
                Some(Item::EndVariation) | None => return Ok(None),
                Some(Item::Nag(_) | Item::Comment(_)) => {}
            }
        }
    }

    /// Skip the rest of a variation whose start was just read, nested ones
    /// included. Moves are not played, so only the framing is checked.
    pub(super) fn skip_variation(&mut self) -> Result<()> {
        let mut depth = 1usize;
        while depth > 0 {
            let Some(&byte) = self.bytes.get(self.index) else {
                return Err(malformed("unterminated variation"));
            };
            match byte {
                START_VARIATION => depth += 1,
                END_VARIATION => depth -= 1,
                NAG | COMMENT => {
                    // Reuse the item reader for the bounds checks; a move byte
                    // is never read here, so the position is untouched.
                    self.next_item()?;
                    continue;
                }
                _ => {}
            }
            self.index += 1;
        }
        Ok(())
    }

    fn take(&mut self, len: usize, what: &str) -> Result<&'a [u8]> {
        let end = self
            .index
            .checked_add(len)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| malformed(what))?;
        let taken = &self.bytes[self.index..end];
        self.index = end;
        Ok(taken)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comment(text: &[u8]) -> Vec<u8> {
        let mut blob = vec![COMMENT];
        blob.extend((text.len() as u64).to_be_bytes());
        blob.extend(text);
        blob
    }

    fn drain(blob: &[u8]) -> Result<Vec<Item<'_>>> {
        let mut cursor = MoveBlobCursor::new(blob, Chess::default());
        let mut items = Vec::new();
        while let Some(item) = cursor.next_item()? {
            items.push(item);
        }
        Ok(items)
    }

    #[test]
    fn cursor_yields_moves_nags_comments_and_variations() {
        let mut blob = vec![12, NAG, 3, START_VARIATION, 0, END_VARIATION];
        blob.extend(comment(b"hi"));
        blob.push(0);
        let items = drain(&blob).unwrap();
        assert!(matches!(items[0], Item::Move(_)));
        assert_eq!(items[1], Item::Nag(3));
        assert_eq!(items[2], Item::StartVariation);
        assert!(matches!(items[3], Item::Move(_)));
        assert_eq!(items[4], Item::EndVariation);
        assert_eq!(items[5], Item::Comment(b"hi"));
        assert!(matches!(items[6], Item::Move(_)));
        assert_eq!(items.len(), 7);
    }

    fn malformed_blobs() -> Vec<(&'static str, Vec<u8>)> {
        let mut past_end = vec![12, COMMENT];
        past_end.extend(100u64.to_be_bytes());
        past_end.extend(b"short");
        let mut huge = vec![12, COMMENT];
        huge.extend(u64::MAX.to_be_bytes());
        vec![
            ("trailing NAG", vec![12, NAG]),
            ("comment past end", past_end),
            ("comment u64::MAX", huge),
            ("truncated comment length", vec![COMMENT, 0, 0]),
            ("move out of range", vec![12, 200]),
        ]
    }

    #[test]
    fn malformed_blobs_return_an_error_when_read() {
        for (name, blob) in malformed_blobs() {
            assert!(
                matches!(drain(&blob), Err(Error::MalformedMoveBlob(_))),
                "{name}"
            );
        }
    }

    #[test]
    fn main_line_reader_errors_on_the_same_blobs() {
        for (name, blob) in malformed_blobs() {
            let mut cursor = MoveBlobCursor::new(&blob, Chess::default());
            let result = (|| {
                while cursor.next_main_move()?.is_some() {}
                Ok::<_, Error>(())
            })();
            assert!(matches!(result, Err(Error::MalformedMoveBlob(_))), "{name}");
        }
    }

    #[test]
    fn a_skipped_variation_must_be_terminated() {
        let blob = [12, START_VARIATION, 0];
        let mut cursor = MoveBlobCursor::new(&blob, Chess::default());
        assert!(cursor.next_main_move().is_err());
    }

    #[test]
    fn skipping_a_variation_ignores_marker_bytes_inside_comments() {
        let mut blob = vec![12, START_VARIATION];
        blob.extend(comment(&[END_VARIATION]));
        blob.extend([END_VARIATION, 0]);
        let mut cursor = MoveBlobCursor::new(&blob, Chess::default());
        assert!(cursor.next_main_move().unwrap().is_some());
        assert!(cursor.next_main_move().unwrap().is_some());
        assert!(cursor.next_main_move().unwrap().is_none());
    }
}
