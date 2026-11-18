//! Position search functionality
//!
//! This module handles searching for chess positions in game databases.
//! It supports both exact position matching and partial position matching.

use diesel::prelude::*;
use log::info;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use shakmaty::{
    fen::Fen, san::SanPlus, uci::UciMove, Bitboard, ByColor, ByRole, Chess, Color, EnPassantMode,
    FromSetup, Position, Role, Setup, Square,
};
use specta::Type;
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};
use tauri::Emitter;

use crate::db::game_index::IndexFilter;
use crate::{
    db::{
        get_db_or_create, get_pawn_home,
        models::*,
        normalize_games,
        pgn::{get_material_count, MaterialCount},
        schema::*,
        ConnectionOptions, GameSort, Sides, SortDirection,
    },
    error::Error,
    AppState, GameData,
};

use super::move_blob::MoveBlobCursor;
use super::GameQueryJs;

/// Data for exact position matching
/// Requires the position to match exactly including turn, castling rights, etc.
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ExactData {
    pawn_home: u16,
    material: MaterialCount,
    position: Chess,
}

/// Data for partial position matching
/// Only checks if the specified pieces are present, ignoring other pieces
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct PartialData {
    piece_positions: Setup,
    material: MaterialCount,
    /// Squares that must hold no piece at all, of either colour.
    ///
    /// The query board's empty squares are "don't care" as they always were;
    /// these are the ones the caller promoted to "must be empty". Empty by
    /// default, which is plain subset matching.
    forbidden: Bitboard,
    /// Exact number of each piece the matched position must hold.
    ///
    /// `ANY_COUNT` for a piece means it is unconstrained.
    exact_counts: ByColor<ByRole<u8>>,
}

/// A sentinel no real position can reach, representing an unrestricted count.
const ANY_COUNT: u8 = u8::MAX;

/// Every piece unconstrained.
fn any_counts() -> ByColor<ByRole<u8>> {
    ByColor::new_with(|_| ByRole::new_with(|_| ANY_COUNT))
}

/// Whether every selected piece count equals the position's material.
fn has_exact_counts(counts: ByColor<ByRole<u8>>, expected: ByColor<ByRole<u8>>) -> bool {
    let by_colour = counts.zip(expected);
    for (count, expected) in [by_colour.white, by_colour.black] {
        for &(count, expected) in count.zip(expected).iter() {
            if expected != ANY_COUNT && count != expected {
                return false;
            }
        }
    }
    true
}

/// Query type for searching positions
/// - Exact: Match the position exactly
/// - Partial: Match only specified pieces (subset matching)
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub enum PositionQuery {
    Exact(ExactData),
    Partial(PartialData),
}

impl PositionQuery {
    pub fn exact_from_fen(fen: &str) -> Result<PositionQuery, Error> {
        let position: Chess =
            Fen::from_ascii(fen.as_bytes())?.into_position(shakmaty::CastlingMode::Chess960)?;
        let pawn_home = get_pawn_home(position.board());
        let material = get_material_count(position.board());
        Ok(PositionQuery::Exact(ExactData {
            pawn_home,
            material,
            position,
        }))
    }

    /// A partial query requiring only that the placed pieces are present.
    ///
    /// Test-only: the runtime path goes through [`Self::partial_from_fen_with`],
    /// which this is the unconstrained case of.
    #[cfg(test)]
    pub fn partial_from_fen(fen: &str) -> Result<PositionQuery, Error> {
        Self::partial_from_fen_with(fen, Bitboard::EMPTY, any_counts())
    }

    /// A partial query whose `forbidden` squares must hold no piece at all.
    ///
    /// The query board's other empty squares stay "don't care", as they
    /// always were. Forbidding only narrows the query, so the index
    /// pre-filter built from the placements stays valid untouched.
    pub fn partial_from_fen_with(
        fen: &str,
        forbidden: Bitboard,
        exact_counts: ByColor<ByRole<u8>>,
    ) -> Result<PositionQuery, Error> {
        let fen = Fen::from_ascii(fen.as_bytes())?;
        let setup = fen.into_setup();
        let material = get_material_count(&setup.board);
        Ok(PositionQuery::Partial(PartialData {
            piece_positions: setup,
            material,
            forbidden,
            exact_counts,
        }))
    }
}

/// Turn square indices (0 = a1 … 63 = h8) into a bitboard.
fn squares_from_indices(indices: &[u8]) -> Result<Bitboard, Error> {
    let mut board = Bitboard::EMPTY;
    for &index in indices {
        let square = Square::try_from(index).map_err(|_| Error::InvalidSquareIndex(index))?;
        board.add(square);
    }
    Ok(board)
}

#[derive(Debug, Clone, Deserialize, Type, PartialEq, Eq, Hash)]
pub struct PositionQueryJs {
    pub fen: String,
    pub type_: String,
    /// Partial only: square indices (0 = a1 … 63 = h8) that must be empty.
    ///
    /// Omitted or empty reproduces plain partial matching exactly.
    #[serde(default)]
    pub forbidden_squares: Option<Vec<u8>>,
    /// Partial only: the exact number of a piece the matched position must
    /// hold. Pieces not listed are unconstrained.
    #[serde(default)]
    pub exact_pieces: Option<Vec<PieceCountJs>>,
}

/// An exact count for one piece in the matched position.
#[derive(Debug, Clone, Deserialize, Type, PartialEq, Eq, Hash)]
pub struct PieceCountJs {
    /// `"white"` or `"black"`.
    pub color: String,
    /// `"pawn"`, `"knight"`, `"bishop"`, `"rook"`, `"queen"` or `"king"`.
    pub role: String,
    pub count: u8,
}

fn parse_color(name: &str) -> Option<Color> {
    match name {
        "white" => Some(Color::White),
        "black" => Some(Color::Black),
        _ => None,
    }
}

fn parse_role(name: &str) -> Option<Role> {
    match name {
        "pawn" => Some(Role::Pawn),
        "knight" => Some(Role::Knight),
        "bishop" => Some(Role::Bishop),
        "rook" => Some(Role::Rook),
        "queen" => Some(Role::Queen),
        "king" => Some(Role::King),
        _ => None,
    }
}

/// Turn selected counts into a full table, leaving unlisted pieces free.
fn exact_counts_from_js(counts: &[PieceCountJs]) -> Result<ByColor<ByRole<u8>>, Error> {
    let mut table = any_counts();
    for piece in counts {
        let color = parse_color(&piece.color)
            .ok_or_else(|| Error::InvalidPieceLimit(piece.color.clone()))?;
        let role =
            parse_role(&piece.role).ok_or_else(|| Error::InvalidPieceLimit(piece.role.clone()))?;
        *table.get_mut(color).get_mut(role) = piece.count;
    }
    Ok(table)
}

fn exact_counts_cover_query(
    query_counts: ByColor<ByRole<u8>>,
    exact_counts: ByColor<ByRole<u8>>,
) -> bool {
    let by_colour = query_counts.zip(exact_counts);
    for (placed, exact) in [by_colour.white, by_colour.black] {
        for &(placed, exact) in placed.zip(exact).iter() {
            if exact != ANY_COUNT && exact < placed {
                return false;
            }
        }
    }
    true
}

/// Convert JavaScript position query to internal format
#[inline(always)]
fn convert_position_query(query: PositionQueryJs) -> Result<PositionQuery, Error> {
    match query.type_.as_str() {
        "exact" => PositionQuery::exact_from_fen(&query.fen),
        "partial" => {
            let forbidden = match &query.forbidden_squares {
                Some(indices) => squares_from_indices(indices)?,
                None => Bitboard::EMPTY,
            };
            let exact_counts = match &query.exact_pieces {
                Some(counts) => exact_counts_from_js(counts)?,
                None => any_counts(),
            };
            let query_board = Fen::from_ascii(query.fen.as_bytes())?.into_setup().board;
            if !exact_counts_cover_query(query_board.material(), exact_counts) {
                return Err(Error::InvalidMaterialCount);
            }
            PositionQuery::partial_from_fen_with(&query.fen, forbidden, exact_counts)
        }
        _ => unreachable!(),
    }
}

impl PositionQuery {
    /// Check if a chess position matches this query
    #[inline(always)]
    fn matches(&self, position: &Chess) -> bool {
        match self {
            PositionQuery::Exact(ref data) => {
                // Check turn and board position exactly
                data.position.turn() == position.turn() && data.position.board() == position.board()
            }
            PositionQuery::Partial(ref data) => {
                let query_board = &data.piece_positions.board;
                let tested_board = position.board();

                // Absence first: one bitboard test, and it rejects far more
                // positions than the containment checks do.
                if (tested_board.occupied() & data.forbidden).any() {
                    return false;
                }

                if !has_exact_counts(tested_board.material(), data.exact_counts) {
                    return false;
                }

                // Check each piece type (kings first for efficiency)
                is_contained(tested_board.kings(), query_board.kings())
                    && is_contained(tested_board.queens(), query_board.queens())
                    && is_contained(tested_board.rooks(), query_board.rooks())
                    && is_contained(tested_board.bishops(), query_board.bishops())
                    && is_contained(tested_board.knights(), query_board.knights())
                    && is_contained(tested_board.pawns(), query_board.pawns())
                    && is_contained(tested_board.white(), query_board.white())
                    && is_contained(tested_board.black(), query_board.black())
            }
        }
    }

    /// Check if current position has enough material to match the query
    #[inline(always)]
    fn has_sufficient_material(&self, current_material: &MaterialCount) -> bool {
        let target_material = match self {
            PositionQuery::Exact(ref data) => &data.material,
            PositionQuery::Partial(ref data) => &data.material,
        };

        // Current position must have at least as much material as target
        current_material.white >= target_material.white
            && current_material.black >= target_material.black
    }

    fn is_reachable_by(&self, material: &MaterialCount, pawn_home: u16) -> bool {
        match self {
            PositionQuery::Exact(ref data) => {
                is_end_reachable(data.pawn_home, pawn_home)
                    && is_material_reachable(&data.material, material)
            }
            PositionQuery::Partial(ref data) => is_material_reachable(&data.material, material),
        }
    }

    /// The pre-filter that admits every game able to contain this query.
    ///
    /// Exact queries also carry material and pawn-structure bounds, mirroring
    /// [`Self::can_reach`]; partial queries cannot, since the matched position
    /// may hold pieces the query never mentions.
    fn index_filter(&self, indexed_up_to: i32) -> IndexFilter {
        match self {
            PositionQuery::Exact(data) => IndexFilter::new(data.position.board(), indexed_up_to)
                .with_exact_bounds(data.pawn_home, data.material.white, data.material.black),
            PositionQuery::Partial(data) => {
                IndexFilter::new(&data.piece_positions.board, indexed_up_to)
            }
        }
    }

    fn can_reach(&self, material: &MaterialCount, pawn_home: u16) -> bool {
        match self {
            PositionQuery::Exact(ref data) => {
                is_end_reachable(pawn_home, data.pawn_home)
                    && is_material_reachable(material, &data.material)
            }
            PositionQuery::Partial(_) => true,
        }
    }
}

/// Check if target pawn structure can be reached from current position
fn is_end_reachable(end: u16, pos: u16) -> bool {
    end & !pos == 0
}

/// Check if target material count can be reached from current material
fn is_material_reachable(end: &MaterialCount, pos: &MaterialCount) -> bool {
    end.white <= pos.white && end.black <= pos.black
}

/// Check if all pieces in subset are also in container
fn is_contained(container: Bitboard, subset: Bitboard) -> bool {
    container & subset == subset
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct PositionStats {
    #[serde(rename = "move")]
    pub move_: String,
    pub white: i32,
    pub draw: i32,
    pub black: i32,
}

/// Parses chess moves from binary format one at a time
/// Avoids loading entire game tree into memory
pub(super) struct MoveStream<'a> {
    cursor: MoveBlobCursor<'a>,
}

impl<'a> MoveStream<'a> {
    pub(super) fn new(bytes: &'a [u8], start_position: Chess) -> Self {
        Self {
            cursor: MoveBlobCursor::new(bytes, start_position),
        }
    }

    /// The position after the moves consumed so far.
    pub(super) fn position(&self) -> &Chess {
        self.cursor.position()
    }

    /// Play the next main-line move, returning whether one was found.
    ///
    /// Unlike [`Self::next_move`] this neither clones the position nor builds
    /// SAN, so it is the cheap choice for callers that only need each board.
    /// A malformed blob is an error, so callers can skip the whole game.
    pub(super) fn advance(&mut self) -> Result<bool, Error> {
        Ok(self.cursor.next_main_move()?.is_some())
    }

    pub(super) fn next_move(&mut self) -> Result<Option<PlayedMove>, Error> {
        let before = self.cursor.position().clone();
        Ok(self.cursor.next_main_move()?.map(|chess_move| {
            // Read off the squares before playing: SanPlus consumes
            // the move into the position and the origin is then gone.
            let uci = UciMove::from_standard(&chess_move).to_string();
            let mut played = before;
            let san = SanPlus::from_move_and_play_unchecked(&mut played, &chess_move);
            PlayedMove {
                san: san.to_string(),
                uci,
            }
        }))
    }
}

/// The notation of a single move read out of a game's move blob.
///
/// The position it produced is left in the stream rather than returned, since
/// every caller that wants notation already has the stream to hand and most
/// plies are walked with [`MoveStream::advance`], which builds neither.
pub(super) struct PlayedMove {
    /// Standard algebraic notation, as the statistics table groups by it.
    pub san: String,
    /// Origin and destination squares, for callers that draw the move.
    pub uci: String,
}

/// Where a game first matches a query.
pub struct PositionMatch {
    /// Half-moves to replay to reach the match; 0 is the starting position.
    pub ply: i32,
    /// The matched position itself, for callers that want to show it.
    pub position: Chess,
    /// The move played from it, or `*` at the end of the game.
    pub next_move: String,
    /// The same move as origin/destination squares, empty at the end of the
    /// game. Callers draw it; they should not have to re-parse the notation.
    pub next_move_uci: String,
}

/// A game that matched, kept until its full details are loaded.
struct MatchedGame {
    id: i32,
    /// Half-moves to replay to reach the match.
    ply: i32,
    /// The matched position, serialised only for games actually kept.
    fen: String,
    /// The move played from the match, as origin/destination squares.
    next_move_uci: String,
    /// The higher of the two players' Elo, 0 when neither is recorded.
    rating: i32,
}

/// The rating a game is ranked by: the higher player's Elo, 0 when unrecorded.
fn game_rating(white_elo: Option<i32>, black_elo: Option<i32>) -> i32 {
    white_elo.unwrap_or(0).max(black_elo.unwrap_or(0))
}

impl MatchedGame {
    /// Better games sort higher; on equal rating the lower id does.
    fn rank(&self) -> (i32, Reverse<i32>) {
        (self.rating, Reverse(self.id))
    }
}

impl PartialEq for MatchedGame {
    fn eq(&self, other: &Self) -> bool {
        self.rank() == other.rank()
    }
}
impl Eq for MatchedGame {}
impl PartialOrd for MatchedGame {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for MatchedGame {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.rank().cmp(&other.rank())
    }
}

/// The best-rated matches seen so far, at most `capacity` of them.
///
/// Keeping the first matches by id would make the returned "top games" the
/// oldest ones. A min-heap lets the worst kept game be evicted in O(log n).
struct TopGames {
    heap: BinaryHeap<Reverse<MatchedGame>>,
    capacity: usize,
}

impl Default for TopGames {
    fn default() -> Self {
        Self::with_capacity(MAX_RETURNED_GAMES)
    }
}

impl TopGames {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            heap: BinaryHeap::new(),
            capacity,
        }
    }

    /// Whether a game with this rating and id would be kept, so callers can
    /// skip building the rest of a game that would be dropped.
    fn wants(&self, rating: i32, id: i32) -> bool {
        self.heap.len() < self.capacity
            || self
                .heap
                .peek()
                .is_some_and(|worst| (rating, Reverse(id)) > worst.0.rank())
    }

    fn push(&mut self, game: MatchedGame) {
        if !self.wants(game.rating, game.id) {
            return;
        }
        if self.heap.len() >= self.capacity {
            self.heap.pop();
        }
        self.heap.push(Reverse(game));
    }

    fn merge(&mut self, other: TopGames) {
        for Reverse(game) in other.heap {
            self.push(game);
        }
    }

    fn len(&self) -> usize {
        self.heap.len()
    }

    #[cfg(test)]
    fn iter_ids(&self) -> Vec<i32> {
        self.heap.iter().map(|g| g.0.id).collect()
    }

    fn into_vec(self) -> Vec<MatchedGame> {
        self.heap.into_iter().map(|Reverse(game)| game).collect()
    }
}

/// Find where a game first matches, and what was played from there.
fn get_move_after_match(
    move_blob: &[u8],
    fen: &Option<String>,
    query: &PositionQuery,
) -> Result<Option<PositionMatch>, Error> {
    let start_position = if let Some(fen) = fen {
        let fen = Fen::from_ascii(fen.as_bytes())?;
        Chess::from_setup(fen.into_setup(), shakmaty::CastlingMode::Chess960)?
    } else {
        Chess::default()
    };

    // Check if starting position already matches
    if query.matches(&start_position) {
        let matched = start_position.clone();
        let mut stream = MoveStream::new(move_blob, start_position);
        let (next_move, next_move_uci) = stream
            .next_move()?
            .map(|played| (played.san, played.uci))
            .unwrap_or_else(|| ("*".to_string(), String::new()));
        return Ok(Some(PositionMatch {
            ply: 0,
            position: matched,
            next_move,
            next_move_uci,
        }));
    }

    // Check each position in the game
    let mut stream = MoveStream::new(move_blob, start_position);
    let mut ply = 0i32;

    // Walked with `advance` rather than `next_move`: every ply of every
    // scanned game passes through here, and notation is only ever wanted for
    // the one position that matches. Building it eagerly costs a position
    // clone and two string allocations per ply, all of them discarded.
    while stream.advance()? {
        ply += 1;
        // Quick material check first
        let board = stream.position().board();
        let material = get_material_count(board);

        if !query.has_sufficient_material(&material) {
            continue;
        }

        let pawn_home = get_pawn_home(board);
        if !query.is_reachable_by(&material, pawn_home) {
            return Ok(None); // Position is unreachable
        }

        // Check for position match
        if query.matches(stream.position()) {
            let current_position = stream.position().clone();
            let (next_move, next_move_uci) = stream
                .next_move()?
                .map(|played| (played.san, played.uci))
                .unwrap_or_else(|| ("*".to_string(), String::new()));
            return Ok(Some(PositionMatch {
                ply,
                position: current_position,
                next_move,
                next_move_uci,
            }));
        }
    }

    Ok(None)
}

/// Most games returned to the caller, however many actually matched.
///
/// The same on every code path: the batched and in-memory scans used to
/// disagree, so identical queries returned different numbers of games
/// depending on which one ran.
const MAX_RETURNED_GAMES: usize = 1000;

#[derive(Clone, serde::Serialize)]
pub struct ProgressPayload {
    pub progress: f64,
    pub id: String,
    pub finished: bool,
}

/// Get total number of games in database
fn get_total_game_count(state: &tauri::State<'_, AppState>, file: &Path) -> Result<i64, Error> {
    let db = &mut get_db_or_create(state, file.to_str().unwrap(), ConnectionOptions::default())?;
    use diesel::dsl::count_star;

    let total_count: i64 = games::table.select(count_star()).first(db)?;

    Ok(total_count)
}

/// Load games from database in batches
fn load_games_batch(
    state: &tauri::State<'_, AppState>,
    file: &Path,
    offset: i64,
    limit: i64,
) -> Result<Vec<SearchGameRow>, Error> {
    let db = &mut get_db_or_create(state, file.to_str().unwrap(), ConnectionOptions::default())?;

    let games = games::table
        .select((
            games::id,
            games::white_id,
            games::black_id,
            games::date,
            games::result,
            games::moves,
            games::fen,
            games::pawn_home,
            games::white_material,
            games::black_material,
            games::white_elo,
            games::black_elo,
        ))
        .offset(offset)
        .limit(limit)
        .load(db)?;

    Ok(games)
}

/// A game row as the search path consumes it.
///
/// Id, white id, black id, date, result, moves, starting FEN, pawn home,
/// white material, black material, white Elo, black Elo.
type SearchGameRow = (
    i32,
    i32,
    i32,
    Option<String>,
    Option<String>,
    Vec<u8>,
    Option<String>,
    i32,
    i32,
    i32,
    Option<i32>,
    Option<i32>,
);

/// Load the next games with an id above `after_id`, applying `filter` if given.
///
/// Keyset pagination rather than `OFFSET`, because a filtered scan would
/// otherwise re-walk every skipped row on each batch.
fn load_filtered_games_batch(
    state: &tauri::State<'_, AppState>,
    file: &Path,
    after_id: i32,
    limit: i64,
    filter: Option<&str>,
    query: &GameQueryJs,
) -> Result<Vec<SearchGameRow>, Error> {
    let db = &mut get_db_or_create(state, file.to_str().unwrap(), ConnectionOptions::default())?;

    let mut sql_query = games::table
        .left_join(game_index::table)
        .select((
            games::id,
            games::white_id,
            games::black_id,
            games::date,
            games::result,
            games::moves,
            games::fen,
            games::pawn_home,
            games::white_material,
            games::black_material,
            games::white_elo,
            games::black_elo,
        ))
        .filter(games::id.gt(after_id))
        .order(games::id.asc())
        .limit(limit)
        .into_boxed();

    if let Some(predicate) = filter {
        sql_query = sql_query.filter(diesel::dsl::sql::<diesel::sql_types::Bool>(predicate));
    }

    // Player, date and result go to SQLite rather than being re-checked per
    // row in Rust. Unlike the piece masks these are plain comparisons on
    // indexed columns, so the planner can seek instead of scanning: a query
    // naming a player drops from every game in the file to that player's few
    // thousand. Nulls fall out here too — SQL comparisons against NULL are
    // never true, so a game with no recorded date or result no longer slips
    // through a filter that asked for one.
    match query.sides {
        Some(Sides::Any) => {
            if let Some(player) = query.player1.or(query.player2) {
                sql_query =
                    sql_query.filter(games::white_id.eq(player).or(games::black_id.eq(player)));
            }
        }
        _ => {
            if let Some(player) = query.player1 {
                sql_query = sql_query.filter(games::white_id.eq(player));
            }
            if let Some(player) = query.player2 {
                sql_query = sql_query.filter(games::black_id.eq(player));
            }
        }
    }

    if let Some(start) = &query.start_date {
        sql_query = sql_query.filter(games::date.ge(start.clone()));
    }
    if let Some(end) = &query.end_date {
        sql_query = sql_query.filter(games::date.le(end.clone()));
    }

    if let Some(wanted) = query.wanted_result.as_deref() {
        match wanted {
            "whitewon" => sql_query = sql_query.filter(games::result.eq("1-0")),
            "blackwon" => sql_query = sql_query.filter(games::result.eq("0-1")),
            "draw" => sql_query = sql_query.filter(games::result.eq("1/2-1/2")),
            _ => {}
        }
    }

    let games = sql_query.load(db)?;

    Ok(games)
}

/// Check if game matches basic filters (player, date, result)
#[inline(always)]
fn matches_basic_filters(
    white_id: i32,
    black_id: i32,
    date: &Option<String>,
    result: &Option<String>,
    query: &GameQueryJs,
) -> bool {
    // Check player filters. `Sides::Any` asks for the player on either side,
    // so the two seats are an OR rather than the usual pair of constraints.
    match query.sides {
        Some(Sides::Any) => {
            if let Some(player) = query.player1.or(query.player2) {
                if player != white_id && player != black_id {
                    return false;
                }
            }
        }
        _ => {
            if let Some(player1) = query.player1 {
                if player1 != white_id {
                    return false;
                }
            }

            if let Some(player2) = query.player2 {
                if player2 != black_id {
                    return false;
                }
            }
        }
    }

    // Check result filter. A game with no recorded result satisfies no
    // particular one: asking for White wins and being handed a game whose
    // outcome is unknown is a miss, not a pass.
    if let Some(wanted_result) = &query.wanted_result {
        let wanted = match wanted_result.as_str() {
            "whitewon" => Some("1-0"),
            "blackwon" => Some("0-1"),
            "draw" => Some("1/2-1/2"),
            _ => None,
        };
        if let Some(wanted) = wanted {
            if result.as_deref() != Some(wanted) {
                return false;
            }
        }
    }

    // Check date filters. Undated games are excluded once a range is asked
    // for, on the same reasoning as results.
    if let Some(start_date) = &query.start_date {
        match date {
            Some(game_date) if game_date >= start_date => {}
            _ => return false,
        }
    }

    if let Some(end_date) = &query.end_date {
        match date {
            Some(game_date) if game_date <= end_date => {}
            _ => return false,
        }
    }

    true
}

/// A game that contains the query, as one game's check reports it.
struct GameHit {
    id: i32,
    rating: i32,
    /// The move played from the match, with this one game's result tallied.
    entry: PositionStats,
    found: PositionMatch,
}

/// What checking one game against a query found.
enum GameCheck {
    /// Failed the player, result or date filters.
    Filtered,
    /// Its end material or pawn structure cannot contain the query, so its
    /// moves were not decoded.
    Unreachable,
    /// Decoded, and the query never occurs (or the blob is malformed).
    Miss,
    Hit(GameHit),
}

/// Check one game against the query: filters, then the cheap reachability
/// reject, then the move blob.
fn check_game(row: &GameData, query: &GameQueryJs, position: &PositionQuery) -> GameCheck {
    let (
        id,
        white_id,
        black_id,
        date,
        result,
        moves,
        fen,
        pawn_home,
        white_material,
        black_material,
        white_elo,
        black_elo,
    ) = row;

    if !matches_basic_filters(*white_id, *black_id, date, result, query) {
        return GameCheck::Filtered;
    }

    let end_material: MaterialCount = ByColor {
        white: *white_material as u8,
        black: *black_material as u8,
    };
    if !position.can_reach(&end_material, *pawn_home as u16) {
        return GameCheck::Unreachable;
    }

    let Ok(Some(found)) = get_move_after_match(moves, fen, position) else {
        return GameCheck::Miss;
    };

    let mut entry = PositionStats {
        move_: found.next_move.clone(),
        white: 0,
        black: 0,
        draw: 0,
    };
    match result.as_deref() {
        Some("1-0") => entry.white += 1,
        Some("0-1") => entry.black += 1,
        Some("1/2-1/2") => entry.draw += 1,
        _ => (), // Unknown results don't count
    }
    GameCheck::Hit(GameHit {
        id: *id,
        rating: game_rating(*white_elo, *black_elo),
        entry,
        found,
    })
}

/// The result of checking some games; two of them merge into one.
#[derive(Default)]
struct SearchResults {
    position_stats: HashMap<String, PositionStats>,
    top_games: TopGames,
    processed: usize,
    passed_filters: usize,
    unreachable: usize,
}

impl SearchResults {
    fn record(&mut self, check: GameCheck) {
        self.processed += 1;
        match check {
            GameCheck::Filtered => {}
            GameCheck::Unreachable => {
                self.passed_filters += 1;
                self.unreachable += 1;
            }
            GameCheck::Miss => self.passed_filters += 1,
            GameCheck::Hit(hit) => {
                self.passed_filters += 1;
                if self.top_games.wants(hit.rating, hit.id) {
                    self.top_games.push(MatchedGame {
                        id: hit.id,
                        rating: hit.rating,
                        ply: hit.found.ply,
                        fen: Fen::from_position(hit.found.position, EnPassantMode::Legal)
                            .to_string(),
                        next_move_uci: hit.found.next_move_uci,
                    });
                }
                self.add_stats(hit.entry);
            }
        }
    }

    fn add_stats(&mut self, entry: PositionStats) {
        let total = self
            .position_stats
            .entry(entry.move_.clone())
            .or_insert_with(|| PositionStats {
                move_: entry.move_.clone(),
                white: 0,
                black: 0,
                draw: 0,
            });
        total.white += entry.white;
        total.black += entry.black;
        total.draw += entry.draw;
    }

    fn merge(mut self, other: SearchResults) -> SearchResults {
        for entry in other.position_stats.into_values() {
            self.add_stats(entry);
        }
        self.top_games.merge(other.top_games);
        self.processed += other.processed;
        self.passed_filters += other.passed_filters;
        self.unreachable += other.unreachable;
        self
    }
}

/// Check every row in parallel and merge the partial results. Rows seen after
/// `cancelled` turns true are skipped.
fn search_rows(
    rows: &[GameData],
    query: &GameQueryJs,
    position: &PositionQuery,
    cancelled: &(impl Fn() -> bool + Sync),
) -> SearchResults {
    rows.par_iter()
        .fold(SearchResults::default, |mut acc, row| {
            if !cancelled() {
                acc.record(check_game(row, query, position));
            }
            acc
        })
        .reduce(SearchResults::default, SearchResults::merge)
}

/// Calculate search progress as percentage
#[inline(always)]
fn calculate_batch_progress(processed: usize, total: usize) -> f64 {
    (processed as f64 / total as f64 * 100.0).min(100.0)
}

/// Search for chess positions in the database
/// Returns position statistics and matching games
#[tauri::command]
#[specta::specta]
pub async fn search_position(
    file: PathBuf,
    query: GameQueryJs,
    app: tauri::AppHandle,
    tab_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(Vec<PositionStats>, Vec<NormalizedGame>), Error> {
    let start = Instant::now();
    info!("Starting position search for tab: {}", tab_id);

    // Convert position query if present - do this first to validate the query
    let position_query = match &query.position {
        Some(pos_query) => {
            info!("Processing position query with FEN: {}", pos_query.fen);
            let converted = convert_position_query(pos_query.clone())?;

            // Debug: Log target position material and pawn structure
            match &converted {
                PositionQuery::Exact(data) => {
                    info!(
                        "Target position (EXACT): material={:?}, pawn_home={}",
                        data.material, data.pawn_home
                    );
                }
                PositionQuery::Partial(data) => {
                    info!("Target position (PARTIAL): material={:?}", data.material);
                }
            }

            Some(converted)
        }
        None => return Err(Error::NoMatchFound), // Position search requires a position
    };

    let position_query = position_query.unwrap();

    // Cache management with LRU
    const DISABLE_CACHE: bool = false;

    if !DISABLE_CACHE {
        let cache_key = (query.clone(), file.clone());

        // Return cached results if available
        // LRU cache automatically updates access order on get()
        let mut cache = state.line_cache.lock().unwrap();
        if let Some(cached_result) = cache.get(&cache_key) {
            info!(
                "Using cached results: {} stats, {} games",
                cached_result.0.len(),
                cached_result.1.len()
            );
            return Ok(cached_result.clone());
        }
        // LRU cache automatically evicts least recently used entries when capacity is reached
        // No need for manual size checking and clearing
    }

    // Handle request cancellation
    let permit = state.new_request.acquire().await.unwrap();
    if state.new_request.available_permits() == 0 {
        drop(permit);
        return Err(Error::SearchStopped);
    }

    // Decide between cached data or batch processing
    let (use_cached_data, total_games, cached_games) = {
        let games_cache = state.db_cache.lock().unwrap();
        let use_cached = !games_cache.is_empty();
        if use_cached {
            let cached_games = games_cache.clone();
            let total = cached_games.len();
            (true, total, Some(cached_games))
        } else {
            drop(games_cache);
            let total = get_total_game_count(&state, &file)? as usize;
            (false, total, None)
        }
    };

    info!(
        "Starting optimized position analysis on {} games with parallel processing",
        total_games
    );

    // Both paths fold rows with `search_rows` and merge with `SearchResults::merge`;
    // they differ only in where the rows come from.
    let cancelled = || state.new_request.available_permits() == 0;
    let results: SearchResults;

    if use_cached_data {
        let games = cached_games.unwrap();
        results = search_rows(&games, &query, &position_query, &cancelled);

        info!(
            "Cached data processing complete: {} games processed, {} passed basic filters, {} rejected by position pre-filter, {} matches found",
            results.processed,
            results.passed_filters,
            results.unreachable,
            results.top_games.len()
        );

        // Emit progress update after batch completion (main thread, no mutex overhead)
        let _ = app.emit(
            "search_progress",
            ProgressPayload {
                progress: 100.0,
                id: tab_id.clone(),
                finished: false,
            },
        );
    } else {
        // Process large datasets in batches to manage memory
        const BATCH_SIZE: i64 = 30000;
        let mut offset = 0;

        // Reject games that cannot hold the query before reading their moves.
        let (index_filter, max_game_id) = {
            let db = &mut get_db_or_create(
                &state,
                file.to_str().unwrap(),
                ConnectionOptions::default(),
            )?;
            let up_to = crate::db::game_index::indexed_up_to(db);
            let max_id: Option<i32> = games::table.select(diesel::dsl::max(games::id)).first(db)?;
            (
                position_query.index_filter(up_to).to_sql(),
                max_id.unwrap_or(0),
            )
        };

        if let Some(predicate) = &index_filter {
            info!("Using position index pre-filter: {predicate}");
        } else {
            info!("No position index available, scanning every game");
        }

        // Collect results from all batches
        let mut global = SearchResults::default();
        let mut cursor = 0i32;

        loop {
            // Check for cancellation
            if cancelled() {
                drop(permit);
                return Err(Error::SearchStopped);
            }

            // Load batch
            let batch = load_filtered_games_batch(
                &state,
                &file,
                cursor,
                BATCH_SIZE,
                index_filter.as_deref(),
                &query,
            )?;
            if batch.is_empty() {
                break;
            }

            cursor = batch.last().map(|(id, ..)| *id).unwrap_or(cursor);

            info!(
                "Processing batch: {} games (up to id {})",
                batch.len(),
                cursor
            );

            global = global.merge(search_rows(&batch, &query, &position_query, &cancelled));

            offset += BATCH_SIZE;

            // Progress tracks position in the id range, since a filtered scan
            // skips an unknown number of games per batch.
            let progress = calculate_batch_progress(cursor as usize, max_game_id.max(1) as usize);
            let _ = app.emit(
                "search_progress",
                ProgressPayload {
                    progress,
                    id: tab_id.clone(),
                    finished: false,
                },
            );

            // For first batch, populate cache if it's reasonable size
            if offset == BATCH_SIZE && total_games < 50000 {
                info!(
                    "Caching games for future searches (small dataset: {} games)",
                    batch.len()
                );
                let mut cache = state.db_cache.lock().unwrap();
                if cache.is_empty() {
                    // Load all games into cache since dataset is manageable
                    let all_games = load_games_batch(&state, &file, 0, i64::MAX)?;
                    *cache = Arc::new(all_games);
                }
            }
        }

        info!("Batch processing complete: {} games processed, {} passed basic filters, {} rejected by position pre-filter, {} matches found",
              global.processed, global.passed_filters, global.unreachable, global.top_games.len());
        results = global;
    }

    let position_stats = results.position_stats;
    let matched_game_ids = results.top_games.into_vec();

    info!(
        "Position search completed in {:?}. Found {} unique moves from {} games.",
        start.elapsed(),
        position_stats.len(),
        matched_game_ids.len()
    );

    // Final cancellation check
    if state.new_request.available_permits() == 0 {
        drop(permit);
        return Err(Error::SearchStopped);
    }

    // Convert results
    let openings: Vec<PositionStats> = position_stats.into_values().collect();

    // Load full game details for matched games
    let match_plies: HashMap<i32, i32> = matched_game_ids
        .iter()
        .map(|matched| (matched.id, matched.ply))
        .collect();
    let match_fens: HashMap<i32, String> = matched_game_ids
        .iter()
        .map(|matched| (matched.id, matched.fen.clone()))
        .collect();
    // Empty at the end of a game, where there is no move to draw.
    let match_next_moves: HashMap<i32, String> = matched_game_ids
        .iter()
        .filter(|matched| !matched.next_move_uci.is_empty())
        .map(|matched| (matched.id, matched.next_move_uci.clone()))
        .collect();
    let matched_ids: Vec<i32> = matched_game_ids.iter().map(|matched| matched.id).collect();

    let mut normalized_games = if !matched_game_ids.is_empty() {
        let db =
            &mut get_db_or_create(&state, file.to_str().unwrap(), ConnectionOptions::default())?;

        let (white_players, black_players) = diesel::alias!(players as white, players as black);
        let mut query_builder = games::table
            .inner_join(white_players.on(games::white_id.eq(white_players.field(players::id))))
            .inner_join(black_players.on(games::black_id.eq(black_players.field(players::id))))
            .inner_join(events::table.on(games::event_id.eq(events::id)))
            .inner_join(sites::table.on(games::site_id.eq(sites::id)))
            .filter(games::id.eq_any(&matched_ids))
            .into_boxed();

        // Apply sorting from query options (except AverageElo which we'll handle in Rust)
        let query_options = query.options.as_ref();
        if let Some(options) = query_options {
            query_builder = match options.sort {
                GameSort::Id => match options.direction {
                    SortDirection::Asc => query_builder.order(games::id.asc()),
                    SortDirection::Desc => query_builder.order(games::id.desc()),
                },
                GameSort::Date => match options.direction {
                    SortDirection::Asc => {
                        query_builder.order((games::date.asc(), games::time.asc()))
                    }
                    SortDirection::Desc => {
                        query_builder.order((games::date.desc(), games::time.desc()))
                    }
                },
                GameSort::WhiteElo => match options.direction {
                    SortDirection::Asc => query_builder.order(games::white_elo.asc()),
                    SortDirection::Desc => query_builder.order(games::white_elo.desc()),
                },
                GameSort::BlackElo => match options.direction {
                    SortDirection::Asc => query_builder.order(games::black_elo.asc()),
                    SortDirection::Desc => query_builder.order(games::black_elo.desc()),
                },
                GameSort::PlyCount => match options.direction {
                    SortDirection::Asc => query_builder.order(games::ply_count.asc()),
                    SortDirection::Desc => query_builder.order(games::ply_count.desc()),
                },
                GameSort::AverageElo => {
                    // AverageElo will be sorted in Rust after calculating
                    query_builder
                }
            };
        }

        let detailed_games: Vec<(Game, Player, Player, Event, Site)> = query_builder.load(db)?;
        let mut games = normalize_games(detailed_games)?;
        // Tell the caller where each game matched, so it can open at that move.
        for game in &mut games {
            game.match_ply = match_plies.get(&game.id).copied();
            game.match_fen = match_fens.get(&game.id).cloned();
            game.match_next_move = match_next_moves.get(&game.id).cloned();
        }
        games
    } else {
        Vec::new()
    };

    // Sort by average ELO if needed (calculated in Rust)
    let query_options = query.options.as_ref();
    let should_sort_by_avg_elo = query_options
        .map(|opt| matches!(opt.sort, GameSort::AverageElo))
        .unwrap_or(true); // Default to AverageElo if no options provided

    let sort_direction = query_options
        .map(|opt| opt.direction.clone())
        .unwrap_or(SortDirection::Desc); // Default to Desc if no options provided

    if should_sort_by_avg_elo {
        normalized_games.sort_by(|a, b| {
            // Calculate average ELO: (white_elo + black_elo) / 2, rounded
            // If only one ELO is available, use that one
            // If neither is available, treat as 0 for sorting purposes
            let a_avg = match (a.white_elo, a.black_elo) {
                (Some(white), Some(black)) => {
                    // Round the average (same as Math.round in TypeScript)
                    let sum = white + black;
                    Some((sum + 1) / 2) // This is equivalent to rounding for integers
                }
                (Some(elo), None) | (None, Some(elo)) => Some(elo),
                (None, None) => None,
            };
            let b_avg = match (b.white_elo, b.black_elo) {
                (Some(white), Some(black)) => {
                    let sum = white + black;
                    Some((sum + 1) / 2)
                }
                (Some(elo), None) | (None, Some(elo)) => Some(elo),
                (None, None) => None,
            };

            // For sorting, treat None as 0 (lowest priority)
            let a_val = a_avg.unwrap_or(0);
            let b_val = b_avg.unwrap_or(0);

            match sort_direction {
                SortDirection::Asc => a_val.cmp(&b_val),
                SortDirection::Desc => b_val.cmp(&a_val), // Descending: higher ELO first
            }
        });
    }

    // Cache results (unless caching is disabled for debugging)
    let result = (openings.clone(), normalized_games.clone());
    if !DISABLE_CACHE {
        let cache_key = (query.clone(), file.clone());
        // LRU cache automatically evicts least recently used entry if at capacity
        state
            .line_cache
            .lock()
            .unwrap()
            .push(cache_key.clone(), result.clone());
        info!(
            "Cached position search results for FEN '{}': {} position stats, {} games",
            cache_key
                .0
                .position
                .as_ref()
                .map(|p| p.fen.as_str())
                .unwrap_or("None"),
            openings.len(),
            normalized_games.len()
        );
    } else {
        info!(
            "CACHE DISABLED: Not caching results ({} position stats, {} games)",
            openings.len(),
            normalized_games.len()
        );
    }

    // Emit completion
    let _ = app.emit(
        "search_progress",
        ProgressPayload {
            progress: 100.0,
            id: tab_id,
            finished: true,
        },
    );

    drop(permit);

    // Log total search time for performance monitoring
    info!(
        "Position search completed in total time: {:?} for FEN: '{}'",
        start.elapsed(),
        query
            .position
            .as_ref()
            .map(|p| p.fen.as_str())
            .unwrap_or("None")
    );

    Ok(result)
}

/// Check if a position exists in the database (without full search)
pub async fn is_position_in_db(
    file: PathBuf,
    query: GameQueryJs,
    state: tauri::State<'_, AppState>,
) -> Result<bool, Error> {
    let db = &mut get_db_or_create(&state, file.to_str().unwrap(), ConnectionOptions::default())?;

    // Log the position query for debugging
    if let Some(pos_query) = &query.position {
        info!(
            "Checking if position exists in DB with FEN: {}",
            pos_query.fen
        );
    }

    if let Some(pos) = state
        .line_cache
        .lock()
        .unwrap()
        .get(&(query.clone(), file.clone()))
    {
        info!(
            "Using cached result for position existence check: {}",
            !pos.0.is_empty()
        );
        return Ok(!pos.0.is_empty());
    }

    // start counting the time
    let start = Instant::now();
    info!("start loading games");

    let permit = state.new_request.acquire().await.unwrap();
    let mut games = state.db_cache.lock().unwrap();

    if games.is_empty() {
        let all_games: Vec<GameData> = games::table
            .select((
                games::id,
                games::white_id,
                games::black_id,
                games::date,
                games::result,
                games::moves,
                games::fen,
                games::pawn_home,
                games::white_material,
                games::black_material,
                games::white_elo,
                games::black_elo,
            ))
            .load(db)?;
        *games = Arc::new(all_games);

        info!("got {} games: {:?}", games.len(), start.elapsed());
    }

    let exists = games.par_iter().any(
        |(
            _id,
            _white_id,
            _black_id,
            _date,
            _result,
            game,
            fen,
            end_pawn_home,
            white_material,
            black_material,
            _white_elo,
            _black_elo,
        )| {
            if state.new_request.available_permits() == 0 {
                return false;
            }
            let end_material: MaterialCount = ByColor {
                white: *white_material as u8,
                black: *black_material as u8,
            };
            if let Some(position_query) = &query.position {
                let position_query =
                    convert_position_query(position_query.clone()).expect("Invalid position query");
                position_query.can_reach(&end_material, *end_pawn_home as u16)
                    && get_move_after_match(game, fen, &position_query)
                        .unwrap_or(None)
                        .is_some()
            } else {
                false
            }
        },
    );
    info!("finished search in {:?}", start.elapsed());
    if state.new_request.available_permits() == 0 {
        drop(permit);
        return Err(Error::SearchStopped);
    }

    if !exists {
        info!("Position not found in DB, caching empty result");
        state
            .line_cache
            .lock()
            .unwrap()
            .push((query, file), (vec![], vec![]));
    } else {
        info!("Position found in DB");
    }

    drop(permit);
    Ok(exists)
}

#[cfg(test)]
mod index_filter_tests {
    use super::*;
    use diesel::Connection;

    fn open(path: &str) -> SqliteConnection {
        SqliteConnection::establish(&format!("file:{path}?mode=ro")).unwrap()
    }

    /// A pawn structure plus a few pieces: what partial search is really for.
    ///
    /// White pawns c4/d5/e4, black pawns c7/d6/e5, knights c3/f6, bishop g7 —
    /// nine pieces, but only five piece types, so five mask clauses.
    const STRUCTURE_QUERY: &str = "8/2p3b1/3p1n2/3Pp3/2P1P3/2N5/8/8";

    /// Highest game id to consider, defaulting to the whole database.
    ///
    /// Sampling a prefix of ids is not a random sample — in Caissabase the low
    /// ids are a single recent date range holding almost no games with custom
    /// start positions — so the default must cover everything.
    fn sample_limit() -> i32 {
        std::env::var("POSITION_INDEX_TEST_LIMIT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(i32::MAX)
    }

    /// Every game the verifier accepts must survive the SQL pre-filter.
    ///
    /// Runs the real predicate against the real index, then brute-forces the
    /// same id range, and asserts the filter is a superset. A filter bug shows
    /// up here as a dropped match, which is the only way it could corrupt
    /// results.
    fn assert_filter_is_conservative(path: &str, query: PositionQuery, label: &str) {
        let conn = &mut open(path);
        let up_to = crate::db::game_index::indexed_up_to(conn);
        assert!(up_to > 0, "database has no position index");

        let predicate = query
            .index_filter(up_to)
            .to_sql()
            .expect("query should produce a filter");

        let limit = sample_limit();
        let candidates: Vec<i32> = games::table
            .left_join(game_index::table)
            .select(games::id)
            .filter(games::id.le(limit))
            .filter(diesel::dsl::sql::<diesel::sql_types::Bool>(&predicate))
            .load(conn)
            .unwrap();
        let candidate_set: std::collections::HashSet<i32> = candidates.iter().copied().collect();

        // Brute-force in batches; holding every move blob at once would cost
        // hundreds of megabytes on a database this size.
        let mut total = 0usize;
        let mut matches: Vec<i32> = Vec::new();
        let mut cursor = 0i32;
        loop {
            let batch: Vec<(i32, Vec<u8>, Option<String>)> = games::table
                .select((games::id, games::moves, games::fen))
                .filter(games::id.gt(cursor))
                .filter(games::id.le(limit))
                .order(games::id.asc())
                .limit(50_000)
                .load(conn)
                .unwrap();
            if batch.is_empty() {
                break;
            }
            cursor = batch.last().unwrap().0;
            total += batch.len();

            matches.par_extend(
                batch
                    .par_iter()
                    .filter(|(_, moves, fen)| {
                        get_move_after_match(moves, fen, &query)
                            .unwrap_or(None)
                            .is_some()
                    })
                    .map(|(id, _, _)| *id),
            );
        }

        let dropped: Vec<i32> = matches
            .iter()
            .copied()
            .filter(|id| !candidate_set.contains(id))
            .collect();

        println!(
            "{label}: {total} games -> {} candidates ({:.1}%), {} real matches, {} dropped",
            candidates.len(),
            candidates.len() as f64 / total as f64 * 100.0,
            matches.len(),
            dropped.len(),
        );

        assert!(
            dropped.is_empty(),
            "{label}: filter dropped {} real matches, e.g. {:?}",
            dropped.len(),
            &dropped[..dropped.len().min(5)],
        );
        assert!(!matches.is_empty(), "{label}: sample produced no matches");
    }

    /// Replays every game the given predicate admits, as the batch path does.
    fn timed_scan(
        path: &str,
        query: &PositionQuery,
        predicate: Option<&str>,
    ) -> (usize, usize, f64) {
        let conn = &mut open(path);
        let start = Instant::now();
        let mut cursor = 0i32;
        let mut scanned = 0usize;
        let mut matched = 0usize;

        loop {
            let mut batch_query = games::table
                .left_join(game_index::table)
                .select((games::id, games::moves, games::fen))
                .filter(games::id.gt(cursor))
                .order(games::id.asc())
                .limit(30_000)
                .into_boxed();
            if let Some(predicate) = predicate {
                batch_query =
                    batch_query.filter(diesel::dsl::sql::<diesel::sql_types::Bool>(predicate));
            }

            let batch: Vec<(i32, Vec<u8>, Option<String>)> = batch_query.load(conn).unwrap();
            if batch.is_empty() {
                break;
            }
            cursor = batch.last().unwrap().0;
            scanned += batch.len();

            matched += batch
                .par_iter()
                .filter(|(_, moves, fen)| {
                    get_move_after_match(moves, fen, query)
                        .unwrap_or(None)
                        .is_some()
                })
                .count();
        }

        (scanned, matched, start.elapsed().as_secs_f64())
    }

    #[test]
    #[ignore = "requires an indexed database; run manually"]
    fn bench_filtered_vs_full_scan() {
        let path = std::env::var("POSITION_INDEX_BENCH_DB").unwrap();
        let conn = &mut open(&path);
        let up_to = crate::db::game_index::indexed_up_to(conn);

        let queries = [
            (
                "partial pawn structure + pieces",
                PositionQuery::partial_from_fen(STRUCTURE_QUERY).unwrap(),
            ),
            (
                "partial knight e4",
                PositionQuery::partial_from_fen("8/8/8/8/4N3/8/8/8").unwrap(),
            ),
            (
                "exact ruy lopez",
                PositionQuery::exact_from_fen(
                    "r1bqkbnr/pppp1ppp/2n5/1B2p3/4P3/5N2/PPPP1PPP/RNBQK2R b KQkq - 3 3",
                )
                .unwrap(),
            ),
            (
                "partial queen g7",
                PositionQuery::partial_from_fen("8/6Q1/8/8/8/8/8/8").unwrap(),
            ),
        ];

        for (label, query) in queries {
            let predicate = query.index_filter(up_to).to_sql().unwrap();
            let (base_scanned, base_matched, base_secs) = timed_scan(&path, &query, None);
            let (idx_scanned, idx_matched, idx_secs) = timed_scan(&path, &query, Some(&predicate));

            assert_eq!(
                base_matched, idx_matched,
                "{label}: filtered scan changed the result"
            );
            println!(
                "{label}: full {base_scanned} games in {base_secs:.2}s | filtered {idx_scanned} games in {idx_secs:.2}s | {:.1}x faster | {base_matched} matches",
                base_secs / idx_secs,
            );
        }
    }

    #[test]
    #[ignore = "requires an indexed database; run manually"]
    fn filter_never_drops_a_partial_match() {
        let path = std::env::var("POSITION_INDEX_BENCH_DB").unwrap();
        // A white knight on e4.
        let query = PositionQuery::partial_from_fen("8/8/8/8/4N3/8/8/8").unwrap();
        assert_filter_is_conservative(&path, query, "partial knight e4");
    }

    /// The reported ply must actually hold the queried position.
    ///
    /// Replays each match that many half-moves and checks the query matches
    /// there — a ply that is off by one would otherwise open games at the
    /// wrong move with nothing to catch it.
    #[test]
    #[ignore = "requires an indexed database; run manually"]
    fn reported_ply_lands_on_the_match() {
        let path = std::env::var("POSITION_INDEX_BENCH_DB").unwrap();
        let conn = &mut open(&path);
        let query = PositionQuery::partial_from_fen(STRUCTURE_QUERY).unwrap();

        let games: Vec<(Vec<u8>, Option<String>)> = games::table
            .select((games::moves, games::fen))
            .filter(games::id.le(60_000))
            .load(conn)
            .unwrap();

        let mut checked = 0usize;
        for (moves, fen) in &games {
            let Some(found) = get_move_after_match(moves, fen, &query).unwrap_or(None) else {
                continue;
            };
            let ply = found.ply;

            // Replay exactly `ply` half-moves and confirm the query matches.
            let start = match fen {
                Some(fen) => Chess::from_setup(
                    Fen::from_ascii(fen.as_bytes()).unwrap().into_setup(),
                    shakmaty::CastlingMode::Chess960,
                )
                .unwrap(),
                None => Chess::default(),
            };
            let mut stream = MoveStream::new(moves, start);
            for _ in 0..ply {
                assert!(
                    stream.advance().unwrap(),
                    "game ended before reported ply {ply}"
                );
            }
            assert!(
                query.matches(stream.position()),
                "query does not match at reported ply {ply}"
            );

            // The reported FEN must be that same position.
            let reported = Fen::from_position(found.position.clone(), EnPassantMode::Legal);
            let replayed = Fen::from_position(stream.position().clone(), EnPassantMode::Legal);
            assert_eq!(
                reported.to_string(),
                replayed.to_string(),
                "reported FEN differs from the position at ply {ply}"
            );
            checked += 1;
        }

        println!("verified {checked} matches land on their reported ply");
        assert!(checked > 0, "sample produced no matches to verify");
    }

    #[test]
    #[ignore = "requires an indexed database; run manually"]
    fn filter_never_drops_a_structure_match() {
        let path = std::env::var("POSITION_INDEX_BENCH_DB").unwrap();
        let query = PositionQuery::partial_from_fen(STRUCTURE_QUERY).unwrap();
        assert_filter_is_conservative(&path, query, "partial structure");
    }

    #[test]
    #[ignore = "requires an indexed database; run manually"]
    fn filter_never_drops_an_exact_match() {
        let path = std::env::var("POSITION_INDEX_BENCH_DB").unwrap();
        // Ruy Lopez after 3.Bb5.
        let query = PositionQuery::exact_from_fen(
            "r1bqkbnr/pppp1ppp/2n5/1B2p3/4P3/5N2/PPPP1PPP/RNBQK2R b KQkq - 3 3",
        )
        .unwrap();
        assert_filter_is_conservative(&path, query, "exact ruy lopez");
    }

    /// What must-be-empty squares buy on a real database.
    ///
    /// The Carlsbad skeleton as a plain partial query also admits games where
    /// white kept a c-pawn or black an e-pawn, which are not Carlsbad at all.
    /// Prints the count for each reading rather than asserting one, since the
    /// numbers are the point.
    #[test]
    #[ignore = "requires an indexed database; run manually"]
    fn carlsbad_absence_counts() {
        let path = std::env::var("POSITION_INDEX_BENCH_DB").unwrap();
        let conn = &mut open(&path);
        let up_to = crate::db::game_index::indexed_up_to(conn);

        // White pawns a2 b2 d4 e3 f2 g2 h2, black a7 b7 c6 d5 f7 g7 h7.
        const CARLSBAD: &str = "8/pp3ppp/2p5/3p4/3P4/4P3/PP3PPP/8";

        // The whole c- and e-files bar c6 and e3, which the query fills: a
        // square cannot be both required and empty.
        let whole_files = squares_from_indices(&[
            2, 10, 18, 26, 34, 50, 58, // c1-c5, c7, c8
            4, 12, 20, 28, 36, 44, 60, // e1, e2, e4-e7, e8
        ])
        .unwrap();
        // The squares a surviving white c-pawn or black e-pawn could stand
        // on. Forbidding the whole files also rules out a king on e1 or a
        // knight on c3, which say nothing about the pawn structure.
        let pawn_squares = squares_from_indices(&[10, 18, 26, 34, 28, 36, 44, 52]).unwrap();

        let variants = [
            (
                "plain partial",
                PositionQuery::partial_from_fen(CARLSBAD).unwrap(),
            ),
            (
                "c/e files must be empty",
                PositionQuery::partial_from_fen_with(CARLSBAD, whole_files, any_counts()).unwrap(),
            ),
            (
                "c2-c5/e4-e7 must be empty",
                PositionQuery::partial_from_fen_with(CARLSBAD, pawn_squares, any_counts()).unwrap(),
            ),
        ];

        for (label, query) in variants {
            let predicate = query.index_filter(up_to).to_sql().unwrap();
            let (scanned, matched, secs) = timed_scan(&path, &query, Some(&predicate));
            println!("carlsbad {label}: {matched} matches (scanned {scanned} games in {secs:.1}s)");
        }
    }

    /// A placement on h8 sets bit 63, which is the sign bit of a SQLite
    /// integer. Getting that wrong silently matches nothing.
    #[test]
    #[ignore = "requires an indexed database; run manually"]
    fn filter_handles_the_sign_bit() {
        let path = std::env::var("POSITION_INDEX_BENCH_DB").unwrap();
        let query = PositionQuery::partial_from_fen("7r/8/8/8/8/8/8/8").unwrap();
        assert_filter_is_conservative(&path, query, "partial rook h8");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_partial_match(fen1: &str, fen2: &str) {
        let query = PositionQuery::partial_from_fen(fen1).unwrap();
        let fen = Fen::from_ascii(fen2.as_bytes()).unwrap();
        let chess = Chess::from_setup(fen.into_setup(), shakmaty::CastlingMode::Chess960).unwrap();
        assert!(query.matches(&chess));
    }

    #[test]
    fn exact_matches() {
        let query = PositionQuery::exact_from_fen(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        )
        .unwrap();
        let chess = Chess::default();
        assert!(query.matches(&chess));
    }

    #[test]
    fn empty_matches_anything() {
        assert_partial_match(
            "8/8/8/8/8/8/8/8 w - - 0 1",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        );
    }

    #[test]
    fn correct_partial_match() {
        assert_partial_match(
            "8/8/8/8/8/8/8/6N1 w - - 0 1",
            "3k4/8/8/8/8/4P3/3PKP2/6N1 w - - 0 1",
        );
    }

    #[test]
    #[should_panic]
    fn fail_partial_match() {
        assert_partial_match(
            "8/8/8/8/8/8/8/6N1 w - - 0 1",
            "3k4/8/8/8/8/4P3/3PKP2/7N w - - 0 1",
        );
        assert_partial_match(
            "8/8/8/8/8/8/8/6N1 w - - 0 1",
            "3k4/8/8/8/8/4P3/3PKP2/6n1 w - - 0 1",
        );
    }

    #[test]
    fn correct_exact_is_reachable() {
        let query =
            PositionQuery::exact_from_fen("rnbqkb1r/pppp1ppp/5n2/4p3/4P3/2N5/PPPP1PPP/R1BQKBNR")
                .unwrap();
        let chess = Chess::default();
        assert!(query.is_reachable_by(
            &get_material_count(chess.board()),
            get_pawn_home(chess.board())
        ));
    }

    #[test]
    fn correct_partial_is_reachable() {
        let query = PositionQuery::partial_from_fen("8/8/8/8/8/8/8/8").unwrap();
        let chess = Chess::default();
        assert!(query.is_reachable_by(
            &get_material_count(chess.board()),
            get_pawn_home(chess.board())
        ));
    }

    #[test]
    fn correct_partial_can_reach() {
        let query = PositionQuery::partial_from_fen("8/8/8/8/8/8/8/8").unwrap();
        let chess = Chess::default();
        assert!(query.can_reach(
            &get_material_count(chess.board()),
            get_pawn_home(chess.board())
        ));
    }

    /// The Carlsbad pawn skeleton, the query must-be-empty squares exist for.
    const CARLSBAD: &str = "8/pp3ppp/2p5/3p4/3P4/4P3/PP3PPP/8";

    /// Carlsbad with a white pawn still on c2, so not Carlsbad at all.
    const CARLSBAD_WITH_C_PAWN: &str = "4k3/pp3ppp/2p5/3p4/3P4/4P3/PPP2PPP/4K3 w - - 0 1";

    /// The skeleton plus the two kings the query board leaves off.
    const CARLSBAD_KINGS_ONLY: &str = "4k3/pp3ppp/2p5/3p4/3P4/4P3/PP3PPP/4K3 w - - 0 1";

    /// The skeleton in a full middlegame: rooks, queens, bishops, knights.
    const CARLSBAD_MIDDLEGAME: &str =
        "r1bq1rk1/pp3ppp/2p2n2/3p1b2/3P4/2N1PN2/PP3PPP/R1BQ1RK1 w - - 0 1";

    fn position(fen: &str) -> Chess {
        let fen = Fen::from_ascii(fen.as_bytes()).unwrap();
        Chess::from_setup(fen.into_setup(), shakmaty::CastlingMode::Chess960).unwrap()
    }

    fn squares(indices: &[u8]) -> Bitboard {
        squares_from_indices(indices).unwrap()
    }

    #[test]
    fn forbidden_square_rejects_an_occupied_square() {
        // c2, where the extra white pawn stands.
        let query =
            PositionQuery::partial_from_fen_with(CARLSBAD, squares(&[10]), any_counts()).unwrap();
        assert!(!query.matches(&position(CARLSBAD_WITH_C_PAWN)));
    }

    #[test]
    fn forbidden_square_rejects_a_piece_of_any_type() {
        // e1, occupied by a white king rather than a pawn.
        let query =
            PositionQuery::partial_from_fen_with(CARLSBAD, squares(&[4]), any_counts()).unwrap();
        assert!(!query.matches(&position(CARLSBAD_KINGS_ONLY)));
    }

    #[test]
    fn forbidden_square_allows_an_empty_square() {
        // c2 and e2 are empty once the extra white pawn is gone.
        let query =
            PositionQuery::partial_from_fen_with(CARLSBAD, squares(&[10, 12]), any_counts())
                .unwrap();
        assert!(query.matches(&position(CARLSBAD_KINGS_ONLY)));
    }

    /// Forbidding one square leaves every other square untouched.
    #[test]
    fn forbidding_a_square_constrains_only_that_square() {
        // c2 empty is all this asks: the middlegame keeps its knights on c3
        // and f3, bishops, rooks and queens, and still matches.
        let query =
            PositionQuery::partial_from_fen_with(CARLSBAD, squares(&[10]), any_counts()).unwrap();
        assert!(query.matches(&position(CARLSBAD_MIDDLEGAME)));
        assert!(query.matches(&position(CARLSBAD_KINGS_ONLY)));
    }

    /// An exact count on a piece the query never mentions still narrows the search.
    fn counts(selected: &[(&str, &str, u8)]) -> ByColor<ByRole<u8>> {
        exact_counts_from_js(
            &selected
                .iter()
                .map(|(color, role, count)| PieceCountJs {
                    color: color.to_string(),
                    role: role.to_string(),
                    count: *count,
                })
                .collect::<Vec<_>>(),
        )
        .unwrap()
    }

    #[test]
    fn exact_piece_count_rejects_both_fewer_and_more() {
        let query = PositionQuery::partial_from_fen_with(
            CARLSBAD,
            Bitboard::EMPTY,
            counts(&[("white", "knight", 1)]),
        )
        .unwrap();
        // No white knight is too few.
        assert!(!query.matches(&position(CARLSBAD_KINGS_ONLY)));
        // The middlegame has knights on c3 and f3, which is too many.
        assert!(!query.matches(&position(CARLSBAD_MIDDLEGAME)));
        let one_knight = "r1bq1rk1/pp3ppp/2p2n2/3p1b2/3P4/2N1P3/PP3PPP/R1B2RK1 w - - 0 1";
        assert!(query.matches(&position(one_knight)));
    }

    #[test]
    fn exact_piece_count_of_zero_forbids_a_piece_entirely() {
        let query = PositionQuery::partial_from_fen_with(
            CARLSBAD,
            Bitboard::EMPTY,
            counts(&[("white", "queen", 0)]),
        )
        .unwrap();
        assert!(query.matches(&position(CARLSBAD_KINGS_ONLY)));
        assert!(!query.matches(&position(CARLSBAD_MIDDLEGAME)));
    }

    /// Selecting one piece leaves the other eleven unconstrained.
    #[test]
    fn exact_piece_count_constrains_only_what_it_names() {
        let query = PositionQuery::partial_from_fen_with(
            CARLSBAD,
            Bitboard::EMPTY,
            counts(&[("white", "queen", 0)]),
        )
        .unwrap();
        // Black still has a queen, and rooks, bishops and knights abound.
        let black_queen_only = "r1bq1rk1/pp3ppp/2p2n2/3p1b2/3P4/2N1PN2/PP3PPP/R1B2RK1 w - - 0 1";
        assert!(query.matches(&position(black_queen_only)));
    }

    /// No selected counts is exactly the search as it behaved before material filtering.
    #[test]
    fn any_counts_is_todays_behaviour() {
        let plain = PositionQuery::partial_from_fen(CARLSBAD).unwrap();
        let unrestricted =
            PositionQuery::partial_from_fen_with(CARLSBAD, Bitboard::EMPTY, any_counts()).unwrap();
        assert_eq!(plain, unrestricted);
        for fen in [
            CARLSBAD_WITH_C_PAWN,
            CARLSBAD_KINGS_ONLY,
            CARLSBAD_MIDDLEGAME,
        ] {
            assert!(unrestricted.matches(&position(fen)));
        }
    }

    #[test]
    fn an_unknown_piece_name_is_an_error() {
        let bad = vec![PieceCountJs {
            color: "green".to_string(),
            role: "pawn".to_string(),
            count: 1,
        }];
        assert!(exact_counts_from_js(&bad).is_err());
    }

    #[test]
    fn exact_count_below_query_board_is_rejected() {
        let converted = convert_position_query(PositionQueryJs {
            fen: CARLSBAD.to_string(),
            type_: "partial".to_string(),
            forbidden_squares: None,
            exact_pieces: Some(vec![PieceCountJs {
                color: "white".to_string(),
                role: "pawn".to_string(),
                count: 6,
            }]),
        });
        assert!(matches!(converted, Err(Error::InvalidMaterialCount)));
    }

    #[test]
    fn omitting_forbidden_squares_is_todays_behaviour() {
        let plain = PositionQuery::partial_from_fen(CARLSBAD).unwrap();
        for forbidden in [None, Some(Vec::new())] {
            let converted = convert_position_query(PositionQueryJs {
                fen: CARLSBAD.to_string(),
                type_: "partial".to_string(),
                forbidden_squares: forbidden,
                exact_pieces: None,
            })
            .unwrap();
            assert_eq!(plain, converted);
            // Today's partial search accepts a position holding pieces the
            // query never placed, including an extra c-pawn.
            assert!(converted.matches(&position(CARLSBAD_WITH_C_PAWN)));
            assert!(converted.matches(&position(CARLSBAD_KINGS_ONLY)));
            assert!(converted.matches(&position(CARLSBAD_MIDDLEGAME)));
        }
    }

    #[test]
    fn a_square_index_above_63_is_rejected() {
        assert!(squares_from_indices(&[64]).is_err());
    }

    #[test]
    fn get_move_after_exact_match_test() {
        let game = vec![12, 12]; // 1. e4 e5

        let query = PositionQuery::exact_from_fen(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        )
        .unwrap();
        let result = get_move_after_match(&game[..], &None, &query)
            .unwrap()
            .unwrap();
        assert_eq!((result.ply, result.next_move.as_str()), (0, "e4"));

        let query = PositionQuery::exact_from_fen(
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
        )
        .unwrap();
        let result = get_move_after_match(&game[..], &None, &query)
            .unwrap()
            .unwrap();
        assert_eq!((result.ply, result.next_move.as_str()), (1, "e5"));

        let query = PositionQuery::exact_from_fen(
            "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq e6 0 2",
        )
        .unwrap();
        let result = get_move_after_match(&game[..], &None, &query)
            .unwrap()
            .unwrap();
        assert_eq!((result.ply, result.next_move.as_str()), (2, "*"));
    }

    #[test]
    fn get_move_after_partial_match_test() {
        let game = vec![12, 12]; // 1. e4 e5

        let query = PositionQuery::partial_from_fen("8/pppppppp/8/8/8/8/PPPPPPPP/8").unwrap();
        let result = get_move_after_match(&game[..], &None, &query)
            .unwrap()
            .unwrap();
        assert_eq!((result.ply, result.next_move.as_str()), (0, "e4"));
    }

    #[test]
    fn a_malformed_blob_is_an_error_for_that_game_not_a_miss() {
        let query = PositionQuery::exact_from_fen(
            "rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2",
        )
        .unwrap();
        let mut huge = vec![12, 12, crate::db::move_blob::COMMENT];
        huge.extend(u64::MAX.to_be_bytes());
        for blob in [
            vec![12, 12, crate::db::move_blob::NAG],
            huge,
            vec![12, 12, 200],
        ] {
            assert!(matches!(
                get_move_after_match(&blob, &None, &query),
                Err(Error::MalformedMoveBlob(_))
            ));
        }
    }
}

#[cfg(test)]
mod top_games_tests {
    use super::*;

    fn game(id: i32, rating: i32) -> MatchedGame {
        MatchedGame {
            id,
            ply: 0,
            fen: String::new(),
            next_move_uci: String::new(),
            rating,
        }
    }

    fn ids(top: TopGames) -> Vec<i32> {
        let mut ids: Vec<i32> = top.into_vec().into_iter().map(|g| g.id).collect();
        ids.sort();
        ids
    }

    #[test]
    fn keeps_the_highest_rated_games_not_the_first_ones() {
        let mut top = TopGames::with_capacity(3);
        // Ids ascend while ratings are mixed, so "first three" and "best three" differ.
        for (id, rating) in [
            (1, 1500),
            (2, 1600),
            (3, 1700),
            (4, 2800),
            (5, 1000),
            (6, 2700),
        ] {
            top.push(game(id, rating));
        }
        assert_eq!(ids(top), vec![3, 4, 6]);
    }

    #[test]
    fn merging_keeps_the_best_across_both_sides() {
        let mut a = TopGames::with_capacity(2);
        a.push(game(1, 2000));
        a.push(game(2, 1000));
        let mut b = TopGames::with_capacity(2);
        b.push(game(3, 2500));
        b.push(game(4, 1500));
        a.merge(b);
        assert_eq!(ids(a), vec![1, 3]);
    }

    #[test]
    fn ties_prefer_the_lower_id() {
        let mut top = TopGames::with_capacity(2);
        for id in [5, 3, 9, 1] {
            top.push(game(id, 2000));
        }
        assert_eq!(ids(top), vec![1, 3]);
    }
}

#[cfg(test)]
mod search_rows_tests {
    use super::*;

    const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    const AFTER_E4: &str = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1";
    const AFTER_E4_E5: &str = "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq e6 0 2";

    /// A row as the database would hold it, its end-of-game columns derived
    /// from replaying the blob.
    fn row(id: i32, white_id: i32, result: &str, moves: &[u8], elo: Option<i32>) -> GameData {
        let mut stream = MoveStream::new(moves, Chess::default());
        while stream.advance().unwrap() {}
        let board = stream.position().board();
        let material = crate::db::pgn::get_material_count(board);
        (
            id,
            white_id,
            2,
            Some("2020.01.01".to_string()),
            Some(result.to_string()),
            moves.to_vec(),
            None,
            crate::db::import::get_pawn_home(board) as i32,
            material.white as i32,
            material.black as i32,
            elo,
            None,
        )
    }

    fn after_e4() -> PositionQuery {
        PositionQuery::exact_from_fen(AFTER_E4).unwrap()
    }

    fn hit(check: GameCheck) -> GameHit {
        match check {
            GameCheck::Hit(hit) => hit,
            _ => panic!("expected a hit"),
        }
    }

    fn rows() -> Vec<GameData> {
        vec![
            row(1, 1, "1-0", &[12, 12], Some(2400)),
            row(2, 1, "0-1", &[12], Some(2500)),
            row(3, 3, "1/2-1/2", &[12, 12], None),
            row(4, 1, "1-0", &[0, 0], Some(2700)),
            row(5, 4, "*", &[12, 12], Some(1800)),
        ]
    }

    fn summary(r: &SearchResults) -> (Vec<(String, i32, i32, i32)>, Vec<i32>, usize, usize) {
        let mut stats: Vec<_> = r
            .position_stats
            .values()
            .map(|s| (s.move_.clone(), s.white, s.black, s.draw))
            .collect();
        stats.sort();
        let mut ids: Vec<i32> = r.top_games.iter_ids();
        ids.sort();
        (stats, ids, r.processed, r.passed_filters)
    }

    #[test]
    fn an_exact_match_reports_the_move_played_from_it() {
        let h = hit(check_game(
            &row(1, 1, "1-0", &[12, 12], Some(2400)),
            &GameQueryJs::default(),
            &after_e4(),
        ));
        assert_eq!((h.found.ply, h.entry.move_.as_str()), (1, "e5"));
    }

    #[test]
    fn a_partial_match_hits_at_the_first_matching_position() {
        let query = PositionQuery::partial_from_fen("8/pppppppp/8/8/8/8/PPPPPPPP/8").unwrap();
        let h = hit(check_game(
            &row(1, 1, "1-0", &[12, 12], None),
            &GameQueryJs::default(),
            &query,
        ));
        assert_eq!((h.found.ply, h.entry.move_.as_str()), (0, "e4"));
    }

    #[test]
    fn a_game_that_never_reaches_the_position_is_a_miss() {
        let query = PositionQuery::exact_from_fen(AFTER_E4_E5).unwrap();
        let check = check_game(
            &row(2, 1, "0-1", &[12], None),
            &GameQueryJs::default(),
            &query,
        );
        assert!(!matches!(check, GameCheck::Hit(_)));
    }

    #[test]
    fn a_game_failing_a_filter_is_filtered_before_anything_is_decoded() {
        let query = GameQueryJs {
            player1: Some(9),
            ..Default::default()
        };
        let check = check_game(&row(1, 1, "1-0", &[12, 12], None), &query, &after_e4());
        assert!(matches!(check, GameCheck::Filtered));
    }

    #[test]
    fn a_game_whose_end_cannot_contain_the_query_is_rejected_unread() {
        // The game ends with Black's e-pawn at home; the query needs it moved.
        let query = PositionQuery::exact_from_fen(AFTER_E4_E5).unwrap();
        let check = check_game(
            &row(2, 1, "0-1", &[12], None),
            &GameQueryJs::default(),
            &query,
        );
        assert!(matches!(check, GameCheck::Unreachable));
    }

    #[test]
    fn the_hit_tallies_the_games_result_under_its_move() {
        let tally = |result: &str| {
            let e = hit(check_game(
                &row(1, 1, result, &[12, 12], None),
                &GameQueryJs::default(),
                &after_e4(),
            ))
            .entry;
            (e.white, e.black, e.draw)
        };
        assert_eq!(tally("1-0"), (1, 0, 0));
        assert_eq!(tally("0-1"), (0, 1, 0));
        assert_eq!(tally("1/2-1/2"), (0, 0, 1));
        assert_eq!(tally("*"), (0, 0, 0));
    }

    #[test]
    fn the_hit_is_ranked_by_the_higher_elo() {
        let mut r = row(1, 1, "1-0", &[12, 12], Some(2400));
        r.11 = Some(2600);
        let h = hit(check_game(&r, &GameQueryJs::default(), &after_e4()));
        assert_eq!(h.rating, 2600);
    }

    #[test]
    fn merging_adds_tallies_counts_and_keeps_the_better_games() {
        let q = GameQueryJs::default();
        let rows = rows();
        let left = search_rows(&rows[..2], &q, &after_e4(), &|| false);
        let right = search_rows(&rows[2..], &q, &after_e4(), &|| false);
        let merged = left.merge(right);
        let e5 = &merged.position_stats["e5"];
        assert_eq!((e5.white, e5.black, e5.draw), (1, 0, 1));
        let end = &merged.position_stats["*"];
        assert_eq!((end.white, end.black, end.draw), (0, 1, 0));
        assert_eq!((merged.processed, merged.passed_filters), (5, 5));
        assert_eq!(merged.top_games.len(), 4);
    }

    #[test]
    fn merging_with_an_empty_result_changes_nothing() {
        let r = search_rows(&rows(), &GameQueryJs::default(), &after_e4(), &|| false);
        let before = summary(&r);
        assert_eq!(summary(&r.merge(SearchResults::default())), before);
    }

    #[test]
    fn batched_search_matches_cached_search_over_the_same_games() {
        let q = GameQueryJs::default();
        let rows = rows();
        let cached = search_rows(&rows, &q, &after_e4(), &|| false);
        let batched = rows.chunks(2).fold(SearchResults::default(), |acc, batch| {
            acc.merge(search_rows(batch, &q, &after_e4(), &|| false))
        });
        assert_eq!(summary(&cached), summary(&batched));
        assert_eq!(cached.unreachable, batched.unreachable);
    }

    #[test]
    fn the_reachability_pre_check_never_changes_what_is_found() {
        let q = GameQueryJs::default();
        let rows = rows();
        for fen in [START, AFTER_E4, AFTER_E4_E5] {
            let query = PositionQuery::exact_from_fen(fen).unwrap();
            let checked = search_rows(&rows, &q, &query, &|| false);
            // The same fold with every game decoded.
            let mut unchecked = SearchResults::default();
            for r in &rows {
                if get_move_after_match(&r.5, &r.6, &query).unwrap().is_some() {
                    unchecked.record(check_game(
                        r,
                        &q,
                        &PositionQuery::exact_from_fen(fen).unwrap(),
                    ));
                }
            }
            let mut a = summary(&checked);
            let mut b = summary(&unchecked);
            // Counts of games seen differ by design; compare what was found.
            a.2 = 0;
            a.3 = 0;
            b.2 = 0;
            b.3 = 0;
            assert_eq!(a, b, "{fen}");
        }
    }

    #[test]
    fn rows_after_cancellation_are_skipped() {
        let r = search_rows(&rows(), &GameQueryJs::default(), &after_e4(), &|| true);
        assert_eq!(r.processed, 0);
    }
}
