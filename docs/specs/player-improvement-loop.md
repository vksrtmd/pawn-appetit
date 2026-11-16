# Spec: Player improvement loop

Pawn Appétit's v1 as the best open-source personal improvement app for a 2000+ Player: it syncs the Player's games, reviews every one, keeps each Error as a Mistake Position, finds the Weaknesses behind them, and schedules the fix as Cards in a daily Training Session, with Error Rate showing whether it works.

Every rule here was decided on the map [Map: Player improvement loop][map] and cites the ticket that holds its reasoning, as `[#n] r` (ticket `#n`, rule `r` of its resolution comment). To change a rule, reopen the discussion on its ticket. Terms are defined in [`GLOSSARY.md`](../../GLOSSARY.md); backend and frontend boundaries are in [`docs/architecture.md`](../architecture.md). Code references are at `bcdcbe12`. `Pawn-Appetit/pawn-appetit#n` is an upstream issue.

## Contents

- [Product frame](#product-frame)
- [Build order](#build-order)
- [Foundation fixes](#foundation-fixes)
- [Player DB](#player-db)
- Loop stages: [Account sync](#1-account-sync), [Game Review](#2-game-review), [Mistake Positions](#3-mistake-positions), [Weaknesses](#4-weaknesses), [Cards](#5-cards), [Training Session](#6-training-session), [Progress](#7-progress)
- [UI wording](#ui-wording)
- [After v1](#after-v1)

## Product frame

- **Who**: one Player rated 2000+. The driving dev is that Player, and their Accounts are the test corpus ([#8] 4).
- **Platform**: local-first Tauri desktop app. Online integrations are optional; no web, mobile, hosted accounts or server ([#8] 3).
- **Repo**: `vksrtmd/pawn-appetit`, a diverging product. Upstreaming is optional and never blocks work ([#8] 2).
- **Capacity**: one dev plus agents, part-time; v1 is about 2–3 months ([#8] 6).
- **Training emphasis**, in order: openings from own games, calculation, endgames, time management, strategy ([#8] 8).
- **Frozen**: Lessons, Courses and Exercises get no new content ([#8] 10).
- **The edge** ([#9], corrected 2026-11-16): own-mistake spaced repetition already exists elsewhere (Aimchess, Blunders.ai, chess-self-coach; Masterboard unverified). What no tool combines is: a free desktop app; the Player's own Mistake Positions on FSRS, the same scheduler as Repertoire Review; recent Errors first; lenient grading; Weaknesses that name a cause. Every stage below serves that combination.
- **The loop's only input is the Player's own Accounts** ([#16] 3). Games of any other username are plain database imports, with no Game Review, Mistake Positions, Weaknesses or Cards.

## Build order

Each milestone is usable on its own ([#36] 1–7). Milestone names are used throughout the spec to place each rule.

| Milestone                     | Contents                                                                                                                                                                                            | Usable result                                                                             |
| ----------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| **M0 Foundation**             | F1, F1b, F2, F3, F5, F7, F8, F10, F18, F19, and the profiling pass ([Foundation fixes](#foundation-fixes))                                                                                          | Sync keeps every standard and Chess960 game; Game Review screens and the move tree work   |
| **M1 Player DB**              | `player.db3` with `user_version` migrations, `accounts` table, `SourceId` column, Account DB rebuild ([Player DB](#player-db), [Account sync](#1-account-sync))                                     | The one-way storage door is through; Account DBs carry clocks, evals and site ids         |
| **M2 Game Review queue**      | Two passes, the win-chance Error rule, the Review Standard recorded, workers with resume, newest first, half the cores ([Game Review](#2-game-review))                                              | Every Account game gets a Game Review in the background                                   |
| **M3 Thin slice**             | F11–F13 first; then Mistake Position Cards, `record_card_review`, a minimal Training Session, Error Rate as one number, and a Start entry on the existing dashboard in place of the hardcoded goals | sync → Game Review → Mistake Position Cards → Training Session → Error Rate, usable daily |
| **M4 Full Card collection**   | `deck-*` migration, repertoire Cards on lenient grading, failed-Puzzle Cards, the full Training Session queue rules ([Cards](#5-cards), [Training Session](#6-training-session))                    | Repertoire Review, Mistake Positions and Puzzles run on one scheduler                     |
| **M5 Weaknesses**             | Non-motif facets: phase, endgame type, eval context, time trouble, ECO family ([Weaknesses](#4-weaknesses))                                                                                         | Weaknesses that name a context, each a filtered deck                                      |
| **M6 Progress and dashboard** | [Progress](#7-progress) metrics, the Improvement dashboard, the final glossary rename sweep                                                                                                         | The Player sees whether training works                                                    |

**Cut line.** Below it, stretch work in this order ([#36] 8, 12):

1. **M7 Motif tagger**: the clean-room tagger, its precision-gate labelling screen, and the Lichess `Themes` import for Puzzles. Until it ships, Puzzles are drawn from all themes ([#28] 14).
2. Battery pause for the Game Review queue.
3. Lichess `%eval` substitution for pass 1.
4. F4 (Lichess correspondence games) and F6 (streamed-download timeout).
5. Transfer metrics: Repeat-Error rate, repertoire adherence.
6. Repertoire-gap flagging.

Rules that belong to stretch work are marked **(stretch)** where they appear.

**Process** ([#36] 13–14): glossary renames land per screen, in the milestone that touches the screen, with a final sweep in M6. Foundation fixes are small standalone PRs that could go upstream, never gated on upstream.

## Foundation fixes

Bugs that lose data a loop stage consumes, or make a stage unusable for a Player with years of games ([#12]). Cause analysis, file and line references and sizes are in [#12]; it was read from code, not run.

**M0**

| #         | Fix                                                                                                                                                                                                                                                                |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| F1        | Chess.com sync: await every `writeTextFile` (`src/utils/chess.com/api.tsx:174,203`) and `continue` past a bad month or 429 instead of returning (`:199-200`). The daily-game cause of Pawn-Appetit/pawn-appetit#440 is still unknown; F1b detects it if it recurs. |
| F1b       | After each sync, compare the archive's game count with the stored count and warn when they differ.                                                                                                                                                                 |
| F2        | Parse `[FEN]` with `CastlingMode::Chess960` (`src-tauri/src/db/pgn.rs:303`), so Chess.com and Lichess Chess960 games import.                                                                                                                                       |
| F3        | Download only standard and Chess960: Lichess `perfType`, Chess.com `rules` filter. Other variants no longer fail silently at SAN replay.                                                                                                                           |
| F5        | Send a User-Agent with contact info on both sites, back off and retry on 429 (`src-tauri/src/fs.rs:21-38`, `chess.com/api.tsx:183-211`). Chess.com asks for contact info in the User-Agent ([#10]).                                                                |
| F7        | Surface a failed import to the Player instead of only the console (`db/mod.rs:~114`, `AccountCard.tsx:521-527`). Retry is safe thanks to dedup.                                                                                                                    |
| F8        | The novelty `N` no longer hides the move's annotation (Pawn-Appetit/pawn-appetit#431, `src/state/store/tree.ts:733` and the six `annotations[0]` readers).                                                                                                         |
| F10       | One color per annotation symbol across views (`src/utils/annotation.ts:105,125-133`).                                                                                                                                                                              |
| F18       | The move context menu works in analysis and Repertoire editing (Pawn-Appetit/pawn-appetit#408, `src/components/CompleteMoveCell.tsx:100-138`); clipboard errors surface.                                                                                           |
| F19       | The board fits panels wider than tall (Pawn-Appetit/pawn-appetit#361, `src/styles/Chessboard.css.ts:6-9`).                                                                                                                                                         |
| Profiling | Profile board, analysis and Repertoire Review (Pawn-Appetit/pawn-appetit#385, #354, #380) to confirm F11–F13 and F16 before fixing them ([#12]).                                                                                                                   |

**Before M3**: F11 (keep the live engine score out of the tree store's `root`, narrow selectors), F12 (stable `drawable`/`movable` props for Chessground), F13 (coalesce arrow-key repeat).

**M4 and later**: F14, F15 and F17 are dropped as standalone fixes, since M4 replaces that code ([#36] 5). F16 (audio element per move) is fixed only if profiling blames it. F9 is replaced by the Error rule in [Game Review](#2-game-review).

**Not foundation** ([#12]): Checkmate Training, Play mode, audio, tab flicker, updater and signing bugs.

## Player DB

All loop records live in one app-level SQLite DB; Per-Account game DBs are caches ([#27]). Built in M1, extended by later milestones.

- **Location and owner**: `appData/player.db3`, not listed under Databases. Rust owns it (diesel), versioned by `PRAGMA user_version` with ordered SQL scripts. TS reaches it only through commands ([#27] 1–2).
- **Tables** ([#27] 3–10):

  | Table               | Holds                                                                                                                                                                  |
  | ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
  | `accounts`          | The Player's Accounts: platform, username, added_at                                                                                                                    |
  | `game_reviews`      | One per reviewed game: platform, source id, Account, Player colour, time-control class, result, played_at, engine name and version, pass 1/2 node budgets, reviewed_at |
  | `review_moves`      | One per ply: review, ply, side, eval cp/mate, eval source (local or Lichess `%eval`), win chance, `clock_ms`, phase, eval context (~400k rows for 5k games)            |
  | `mistake_positions` | Self-contained, see [Mistake Positions](#3-mistake-positions); scalar facet columns                                                                                    |
  | `mistake_motifs`    | Mistake Position, motif, missed/allowed, tagger version **(stretch, M7)**                                                                                              |
  | `cards`             | kind, key, `ts-fsrs` state columns, suspended                                                                                                                          |
  | `card_reviews`      | Card, grade, reviewed_at, duration_ms, FSRS log fields                                                                                                                 |
  | `card_mistakes`     | Links a Card to each of its Mistake Positions                                                                                                                          |

- **Game identity**: a Game Review is keyed by (platform, site game id): Lichess from `[Site]`, Chess.com from `[Link]`. Nothing references game-DB rowids ([#27] 4).
- **Re-review** replaces a Game Review's `review_moves` and Errors in one transaction ([#27] 8). It changes a Card's links, not the Card: a Mistake Position that is no longer an Error is unlinked, and a Card with no links left is suspended, keeping its log ([#27] 12).
- **Removing an Account from the Player** deletes its Game Reviews and Mistake Positions and suspends orphaned Cards. Deleting only its game DB file deletes nothing in the Player DB ([#27] 13).
- **Not stored**: Weaknesses and every progress metric are computed by query; no snapshot tables ([#27] 9, [#29] 9).
- **Retired**: `analyzed_games.json` is not imported; those games are re-reviewed ([#27] 15).

## 1. Account sync

**M0**: the foundation fixes F1–F3, F5, F7 above. **M1**:

1. **The Player's Accounts** are the rows of `accounts`, set by a "this is me" toggle on the Account card, seeded once from the `mainAccount` group in the `sessions` localStorage atom. Tokens stay in `sessions`. Other usernames never enter the table ([#27] 3).
2. **Lichess requests** add `clocks=true`, `evals=true` and `opening=true` ([#10], [#27] 16).
3. **`SourceId`**: game DBs get a nullable `SourceId` column, filled on import from `[Site]` (Lichess) or `[Link]` (Chess.com, which today drops it) ([#27] 4).
4. **Upgrade rebuilds the Player's Account DBs**: each is deleted and re-downloaded with the new sync, which fills clocks, evals, opening tags and `SourceId`. Opponent DBs are untouched ([#27] 16).
5. **Manual prep keeps working** through the rework: adding another username's games without login, its player card's Openings, Ratings and Results panels (`get_players_game_info`, `src-tauri/src/db/mod.rs:847`), and position search by player and colour (`DatabasePanel.tsx:51`) ([#16] 4).
6. **Each finished sync queues Game Reviews** for the new games of the Player's Accounts ([#13] 13).

**Stretch**: F4, re-fetch a lookback window so Lichess correspondence games still running at sync aren't lost, letting dedup absorb the overlap; F6, an idle timeout in place of the 300 s total timeout ([#12], [#36] 12).

## 2. Game Review

Built in M2 ([#13]). Replaces "Analyze All" and today's 500 ms per-move report as the source of Errors.

**Search**

1. **Two passes** ([#13] 1–3, 15):
   - **Pass 1** over every position at fixed nodes, about 1M (a tunable constant calibrated during the build), MultiPV=1.
   - **Pass 2** on candidate Errors only, at about 10× the nodes, MultiPV=3. It confirms the loss at depth, and its alternatives feed lenient grading.
   - A game's pass 2 runs right after its pass 1, so no game is left with unconfirmed Errors.
2. **Errors** are the win-chance loss of the Player's move on a 0–100 scale, from `getWinChance` over stored evaluations, never from `getAnnotation` ([#13] 4–5):

   | Grade   | Win-chance loss |
   | ------- | --------------- |
   | Dubious | ≥5              |
   | Mistake | ≥10             |
   | Blunder | ≥20             |

   No better-alternative condition. Only Mistakes and Blunders become Mistake Positions. The current display grading's known quirks (mate handling, dead rules) are pinned by `src/utils/__tests__/score.test.ts` ([#12] comments) and do not apply to Errors.

3. **Opponent moves** are evaluated in pass 1 for the evaluation before the Player's next move, and for the opponents' Error Rate reference ([#13] 5, [#29] 14). Pass 2 confirms only the Player's moves.
4. **Final position** takes its value from the result: checkmate 0 or 100, stalemate 50, so the final move is judged too ([#13] 6).
5. **No book skip**: openings are analysed from move 1 ([#13] 10).
6. **Critical moments** of a Game Review are its Errors; no "only move" detection in v1 ([#13] 8).
7. **Every time control** gets a Game Review, daily and correspondence included; each Error carries its time-control class ([#13] 11).
8. **Clocks** are parsed once here, from `%clk` into `review_moves.clock_ms` ([#27] 7).
9. **Lichess `%eval`** replaces pass 1 when every ply has one; pass 2 still runs locally ([#13] 7). **(stretch)**

**Queue**

10. **Runs automatically** after each Account sync as a background queue, newest games first, every game, no cap. The manual button stays, for re-running ([#13] 13–14). Rough cost: ~20 s per game with ~4 workers; 5,000 games ≈ 28 h of background time.
11. **Workers**: several engines side by side, one game each, Threads=1, using half the cores by default (a setting) ([#13] 9, 13).
12. **Pause and resume**: the Player can pause; each finished game is saved, so a restart resumes where it stopped ([#13] 13).
13. **No engine installed**: the queue waits and prompts to install Stockfish through the existing download (`src/utils/engines.ts`); it never fails silently ([#13] 17).
14. **Battery**: pauses on battery power by default, with a setting to turn that off ([#13] 16). **(stretch)**

**Review Standard**

15. **Each Game Review records** engine name and version and both node budgets ([#13] 12).
16. **The Review Standard is pinned.** Updating Stockfish does not change it. The Player opts in to a new standard, which queues a re-review, newest first ([#29] 10).

## 3. Mistake Positions

Created by M2's Game Review; Cards for them in M3.

1. **Each Mistake or Blunder** on a Player's move is a Mistake Position ([#13] 4).
2. **Self-contained** ([#27] 5): full FEN, normalized FEN (no move counters), move played, best move, accepted alternatives (moves within 5 win-chance points of the best, from pass 2's MultiPV=3), win chance before and after, grade, best PV and refutation PV (UCI). Deleting or rebuilding a game DB never breaks it.
3. **The refutation PV** comes from one deep search of the position after the Error ([#15] 5). It feeds only the motif tagger, so it is filled with M7. **(stretch)**
4. **Facet columns** are filled by [Weaknesses](#4-weaknesses) (M5).
5. **At a repertoire position**, an Error lapses the repertoire Card instead of making a new Card. If the move played is the repertoire move itself, no Card is made and the Mistake Position is flagged "repertoire move is an Error" so the Player can fix the line ([#14] 10).

## 4. Weaknesses

Built in M5, motifs in M7 ([#15]).

1. **Facet model**: each Mistake Position is tagged on independent facets; a Weakness is one facet value that recurs. Single-facet Weaknesses only; facet pairs are a UI filter ([#15] 1, 8).
2. **v1 facets** ([#15] 2, 6, 12, 16), as columns on `mistake_positions` ([#27] 9):

   | Facet              | Values                                                 | Rule                                                                                                   |
   | ------------------ | ------------------------------------------------------ | ------------------------------------------------------------------------------------------------------ |
   | Phase              | opening, middlegame, endgame                           | Clean-room heuristic modelled on Lichess's divider; the endgame starts at ≤6 non-pawn, non-king pieces |
   | Endgame type       | pawn, rook, minor, queen, mixed                        | Endgame positions only                                                                                 |
   | Eval context       | conversion, equal, defence                             | Win chance before the Error ≥75 is conversion, ≤25 defence (tunable constant)                          |
   | Time trouble       | yes, no                                                | Under 10% of base time on `%clk`; daily, correspondence and clockless games excluded                   |
   | Opening            | ECO family                                             | From the stored `ECO`                                                                                  |
   | Time-control class | bullet, blitz, rapid, classical, daily, correspondence | Filter only                                                                                            |

3. **Counted games** ([#15] 13): blitz, rapid and classical pooled, bullet excluded. Daily and correspondence games count for every facet except time trouble. A filter shows a single class.
4. **Window**: the last 300 counted games across all Accounts, recomputed after each Game Review batch ([#15] 9).
5. **Confidence rule** ([#15] 7): a Weakness needs ≥5 Mistake Positions from distinct games. A context facet also needs the Wilson lower bound of its Error-per-move rate (denominators from `review_moves`) to exceed the Player's overall rate.
6. **Unclassified**: strategic Errors go to an Unclassified bucket. They still become Cards, never a Weakness ([#15] 14).
7. **How Weaknesses drive training** ([#15] 17): each Weakness is a filtered deck of its Mistake Position Cards. Time trouble is an insight only.
8. **Motifs (stretch, M7)** ([#15] 2–5, 7, 11, 15):
   - Facets _motif missed_ (in the best line) and _motif allowed_ (in the refutation), in `mistake_motifs` with a tagger version so it can re-run.
   - Motifs: fork, pin, skewer, discoveredAttack, hangingPiece, backRankMate, mate (mate-in-1 to 5 merged), the Lichess theme names.
   - A clean-room shakmaty tagger written from the theme definitions; no port of cook.py (AGPL) ([#11]). Forcing Mistake Positions with a best line only.
   - **Precision gate**: the driving dev hand-labels about 30 of their Mistake Positions per motif in a minimal review screen; a motif ships at ≥0.8 precision and is hidden otherwise.
   - Motif Weaknesses are ranked by count (no opportunity denominator in v1) and also pull Puzzles with the same theme.
   - **Puzzle themes**: import the Lichess `Themes` column into the Puzzles table (a schema migration); local tagging covers only Puzzle files without themes.
9. **Repertoire deviations** ([#15] 10, [#14] 10): a Player deviation from their own repertoire lapses that repertoire Card even without an Error (M4). An opponent deviation followed by a Player Error within 6 plies is flagged as a "repertoire gap" **(stretch)**.

## 5. Cards

Mistake Position Cards in M3; the full collection in M4 ([#14]).

1. **One Card collection, one scheduler** for repertoire moves, Mistake Positions and Puzzles. Decks are filtered views by kind, repertoire or Weakness ([#14] 1).
2. **FSRS via `ts-fsrs`**: default parameters, desired retention 0.9 as one setting, fuzz on, no optimizer ([#14] 2–3).
3. **Scheduling stays in TS.** `ts-fsrs` computes the next state and sends it to `record_card_review`, which upserts one `cards` row and inserts one `card_reviews` row ([#14] 13, [#27] 2). Every answer is logged: Card, grade, time, time taken ([#14] 4).
4. **Keys** ([#14] 5, [#27] 10–11):

   | Kind             | Key                                                                                         |
   | ---------------- | ------------------------------------------------------------------------------------------- |
   | Repertoire       | (repertoire id, normalized FEN, side)                                                       |
   | Mistake Position | normalized FEN; one Error repeated across games is one Card linked to each Mistake Position |
   | Puzzle           | (FEN, solution moves)                                                                       |

   The repertoire id is a UUID written into the file's `.info` sidecar on first use, so in-app renames keep it. Positions shared across games in one repertoire file merge into one Card.

5. **Card creation**:
   - Each Mistake Position gets a Card automatically, introduced newest game first and never before the day after the game ([#14] 7).
   - A Puzzle becomes a Card only when the Player fails it ([#14] 8).
   - When a repertoire's move at a position changes, that Card resets to New and keeps its log ([#14] 6).
   - The Player can suspend any Card ([#14] 7).
6. **A Mistake Position Card asks for one move**: the best move, with alternatives accepted as below ([#14] 11).
7. **Lenient automatic grading** ([#14] 9):

   | Grade | When                                                                                                                                                                                  |
   | ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
   | Again | Wrong move, or "see answer"                                                                                                                                                           |
   | Hard  | An accepted alternative (Mistake Position: within 5 win-chance points of the best; repertoire: a sideline the repertoire holds at that position), or the right move on the second try |
   | Good  | The best move, or the repertoire's main move, on the first try                                                                                                                        |
   | Easy  | Never automatic                                                                                                                                                                       |
   | Skip  | No grade, no log                                                                                                                                                                      |

8. **`deck-*` migration** (M4) ([#14] 12, [#27] 14): runs once at startup; resolves each path to its `.info` id, creating it if needed; re-keys each position by normalized FEN with side from the FEN; imports FSRS state and logs (matched by `fen`); deletes a localStorage key only after its import commits. A deck whose file no longer exists is skipped and counted in a notice.

## 6. Training Session

A minimal Training Session in M3; the full rules in M4 ([#28]).

**Shape**

1. **One Training Session a day**, sized by a time budget, 20 minutes by default. The queue is estimated from each kind's median answer time in `card_reviews`, falling back to 10 s per repertoire Card, 45 s per Mistake Position Card and 60 s per Puzzle. The session ends at the first Card after the budget runs out ([#28] 1).
2. **Blocks in emphasis order**: repertoire Cards (due and new), then Mistake Position Cards, then Puzzles ([#28] 2).
3. **Puzzle share**: 25% of the budget. If Cards finish early, Puzzles fill the rest; if due Cards overflow, the share shrinks to a 10% floor, never zero ([#28] 3).
4. **"Keep training"** after the session adds a 10-minute block of Puzzles only. New Cards stay capped and nothing extra is scheduled ([#28] 4).
5. **Day boundary**: local time, rolling over at 04:00 ([#28] 6).

**Queue rules**

6. **Due before new**: no new Card is introduced while due Cards already fill the session ([#28] 7).
7. **Due order** within a block: lowest FSRS retrievability first ([#28] 8).
8. **New Cards cap, per kind**: 10 Mistake Position Cards and 10 repertoire Cards a day by default, both settings. No cap on due Cards; the budget caps them and the overflow rolls over ([#28] 9–10). A big first sync needs no special rule ([#28] 11).
9. **Relearning**: a Card graded Again returns inside its own block after at least 5 other Cards, using `ts-fsrs` short-term steps ([#28] 12).
10. **Leeches**: a Card with 8 lapses is suspended automatically and listed for the Player to fix or delete ([#28] 13).

**Puzzles and surfaces**

11. **Puzzles** use the existing difficulty selection and are drawn from all themes. With the motif tagger, they target one motif Weakness a day, rotating through the top 3 by rank **(stretch, M7)** ([#28] 14, [#36] 8).
12. **Filtered decks stay usable alone** (one repertoire, one Weakness, Puzzle mode). Their answers go through the same scheduler and count toward the day ([#28] 15).
13. **Replaces the hardcoded daily goals** (`src/utils/dailyGoals.ts`): in M3 a Start entry on the existing dashboard, in M6 the Today screen ([#28] 16, [#36] 4).

## 7. Progress

Error Rate as one number in M3; everything else in M6 ([#29], [#34]).

**Metrics**

1. **Headline: Error Rate**, Mistakes plus Blunders per 100 of the Player's moves over counted games ([Weaknesses](#4-weaknesses) rule 3); Dubious moves left out. Blunders per 100 moves gets its own line ([#29] 1).
2. **Breakdowns** by phase, eval context, time-control class and ECO family ([#29] 2).
3. **Accuracy** stays in each Game Review's summary and is not a progress metric ([#29] 3).
4. **Weakness trend**: each Weakness's rate in the current window against the baseline. A Weakness that no longer meets the confidence rule moves to a "Fixed" list with the date ([#29] 4).
5. **Card retention**: true retention per kind over the last 30 days, counting reviews with an interval of at least 1 day, retained unless graded Again, shown against the 0.9 target ([#29] 7).
6. **Effort**: Training Session days, minutes trained (sum of `card_reviews.duration_ms`) and Cards reviewed, replacing the goal streaks ([#29] 8).
7. **Rating** per Account and time-control class from the PGN headers, reusing `RatingsPanel`, as context only ([#29] 6).
8. **Transfer metrics** ([#29] 5) **(stretch)**: Repeat-Error rate (how often the Player errs again when a position with a Mistake Position Card recurs in a later game) and repertoire adherence (share of games where the Player stays in their repertoire until the opponent leaves it).

**Reference points**

9. **Opponents' Error Rate** in the same games, from pass-1 Errors on both sides. The Player's headline keeps the pass-2-confirmed figure ([#29] 14).
10. **Baseline**: the 300 counted games before the Player's first Training Session, frozen ([#29] 15).

**Computation**

11. **By query** from `game_reviews`, `review_moves` and `card_reviews` by `played_at`, so a re-review updates history too ([#29] 9).
12. **Only Game Reviews at the current Review Standard** count, shown as "N of M games at the current standard" ([#29] 10).
13. **Lichess `%eval` and local pass 1 are pooled**; the eval source stays queryable, and a platform filter exists ([#29] 11).
14. **Trend window**: the last 100 counted games, rolling by game count, plotted against date ([#29] 13).
15. **Minimum sample**: a value shows only when its slice has ≥20 counted games, drawn with a 95% Wilson interval; smaller slices read "not enough games" ([#29] 12).

**Improvement dashboard** (M6). Prototype: branch `34-dashboard-prototype`, commit `2fa398e0`, `src/features/dashboard/prototype-improvement/`, run with `pnpm dev` ([#34]).

16. **Replaces the current dashboard** at `/`. The Continue row, quick actions, puzzle stats, suggestions and hardcoded daily goals go; `GamesHistoryCard` stays, at the bottom of the Today screen, as the only list of Play-mode games ([#34] 1, 10).
17. **Today screen** ([#34] 2):
    - Today's Training Session: Start/Continue, minutes against the budget, one bar per block showing due, new, done and the day's targeted Weakness.
    - Four tiles: **Error Rate** (current, opponents', baseline); **Top Weakness** (rate against baseline, count of Weaknesses and Fixed); **Training** (30-day effort, retention per kind against 90%); **Game Review** ("N of M games at the current standard", queue state including paused on battery).
    - One line of rating per Account and time control, then `GamesHistoryCard`.
18. **Progress page**, the one drill-down ([#34] 3): a master-detail page listing every metric, grouped as Outcome (Error Rate, Blunders), facet breakdowns, Weaknesses, Fixed, Transfer, Card retention and Rating. The selected metric shows its trend chart: rolling 100-game line, Wilson band, opponents' line, baseline mark. Each tile opens Progress with its metric selected.
19. **Weakness detail** has "Train this Weakness" (opens its filtered deck) and "Show Mistake Positions" ([#34] 4).
20. **Rating entry** embeds the existing `RatingsPanel`, which also stays on the Account page ([#34] 5).
21. **Not enough games** ([#34] 6–9): a Today tile greys out with an "n of 20 reviewed games" bar; a Progress list entry shows an n/20 ring, and its detail names how many more reviewed games it needs. With no Weakness yet, Top Weakness says none has been found and that one needs 5 games with the same facet value. Retention with no review older than a day shows "–". During a re-review, the Game Review tile's coverage bar explains thin slices.

## UI wording

The UI uses the glossary terms, removing the double meaning of "Practice" ([#8] 12, [#28] 17, [#29] 18): Repertoire Review (not Practice), Game Review (not Report), Course (not practice category), Training Session, Card, Mistake Position, Weakness, Error Rate, Review Standard, Dubious (not Inaccuracy). Renames land per screen in the milestone that touches it, with a final sweep in M6 ([#36] 13).

## After v1

Everything ruled beyond v1 is listed with its reason in the [map][map]'s Out of scope section. Post-v1 candidates, in the order the map gives them ([#36] 9–11, [#16] 1):

- **Endgame training**: play-out against the engine or a tablebase. The first candidate after the motif tagger; v1 covers endgames through endgame-type and conversion Weaknesses and their Mistake Position Cards.
- **Opponent prep**: profiling a named opponent's repertoire and holes.
- **Time-management analysis** beyond the time-trouble facet.
- **LLM coaching**, with the Player's own key or a local model, never required ([#8] 5); would also take on strategic Errors left Unclassified.
- **Opt-in sync between devices.**
- **Per-kind FSRS tuning**, once there are about 1k review logs per kind.
- **Playing out the line** for Mistake Position Cards.
- **Motif opportunity rate**, which needs a forcing-line test at every position.
- **Estimated rating from Error Rate** (Blunder Elo style), which needs a calibration corpus.
- **OTB games** ([#29] 17).
- **Fork identity and release pipeline** (Pawn-Appetit/pawn-appetit#429, #404): v1 is local `pnpm tauri build` only.

Permanently out: web and mobile clients, hosted services, new beginner content, and roadmap coordination with upstream.

[map]: https://github.com/vksrtmd/pawn-appetit/issues/7
[#8]: https://github.com/vksrtmd/pawn-appetit/issues/8
[#9]: https://github.com/vksrtmd/pawn-appetit/issues/9
[#10]: https://github.com/vksrtmd/pawn-appetit/issues/10
[#11]: https://github.com/vksrtmd/pawn-appetit/issues/11
[#12]: https://github.com/vksrtmd/pawn-appetit/issues/12
[#13]: https://github.com/vksrtmd/pawn-appetit/issues/13
[#14]: https://github.com/vksrtmd/pawn-appetit/issues/14
[#15]: https://github.com/vksrtmd/pawn-appetit/issues/15
[#16]: https://github.com/vksrtmd/pawn-appetit/issues/16
[#27]: https://github.com/vksrtmd/pawn-appetit/issues/27
[#28]: https://github.com/vksrtmd/pawn-appetit/issues/28
[#29]: https://github.com/vksrtmd/pawn-appetit/issues/29
[#34]: https://github.com/vksrtmd/pawn-appetit/issues/34
[#36]: https://github.com/vksrtmd/pawn-appetit/issues/36
