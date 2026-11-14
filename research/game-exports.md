# What the Lichess and Chess.com game exports contain

Research for ticket vksrtmd/pawn-appetit#10. Feeds the spec "Player improvement loop".
Code cited at commit `2d8443f5` (worktree `research-game-exports`). Lichess spec read from
`github.com/lichess-org/api` master (commit `bbbd1153`, 2026-11-13).

## Verification status (read first)

- **Lichess**: verified from the OpenAPI spec in `lichess-org/api` (raw.githubusercontent.com / github.com reachable).
  The rendered `https://lichess.org/api` was **not reachable** (proxy, `SSL_ERROR_SYSCALL`), but the page is generated from that spec.
- **Chess.com**: the Published-Data API doc (`www.chess.com/news/view/published-data-api`) and `api.chess.com` were **not reachable**
  (proxy). Every Chess.com API claim below is therefore **UNVERIFIABLE from a primary source in this run**. Where the
  table says "unverified" the cell is recall, not evidence. Re-check against the doc before the spec relies on it.
  The only hard Chess.com facts are what the app's own zod schema expects (`src/utils/chess.com/api.tsx:50-61`).
- Blocked hosts: `www.chess.com`, `api.chess.com`, `lichess.org`. (An archive.org mirror was not tried: it would be a workaround.)

## A. What the exports return

Lichess endpoint: `GET /api/games/user/{username}`, operationId `apiGamesUser`
(`doc/specs/tags/games/api-games-user-username.yaml`).

| Topic | Lichess (verified, spec) | Chess.com (UNVERIFIED, recall) |
|---|---|---|
| Endpoint | `/api/games/user/{username}`, PGN or NDJSON via `Accept`; newest first (`sort=dateDesc` default, `dateAsc` allowed) | `/pub/player/{u}/games/archives` lists monthly URLs; `/pub/player/{u}/games/{YYYY}/{MM}` returns `{games:[...]}`; each game has `url, pgn, time_control, end_time, rated, fen, rules, white, black` (matches the app schema, `api.tsx:50-61`) |
| Per-move clocks | Only with `clocks=true` (default **false**): PGN `{ [%clk 1:01:27] }` comments, or `clocks` array of centisecond ints in JSON | PGN movetext reportedly carries `{[%clk h:mm:ss.s]}`; no separate JSON clocks field recalled. Unverified |
| Engine evals | Only with `evals=true` (default **false**), "when available": PGN `{ [%eval 0.23] }` or JSON `analysis[]` of `{eval (cp), mate, best, variation, judgment{name,comment}}` (`schemas/GameMoveAnalysis.yaml`). Only exists for games that were analysed; filter `analysed=true/false` | Not in the games export. Chess.com has an `accuracies` object on some games, from recall. Unverified |
| Opening | `opening=true` adds `[Opening "..."]` tag (default false); JSON `opening{eco,name,ply}` (`schemas/GameOpening.yaml`) | PGN has `[ECO]` and an `[ECOUrl]` tag, from recall. Unverified |
| Daily / correspondence | Same endpoint. `speed` enum includes `correspondence`; JSON has `daysPerTurn` (`schemas/GameJson.yaml`, `schemas/Speed.yaml`). `perfType` filter | Daily games appear in the monthly archive of the month they **end** (recall); `time_control` like `1/86400`; a separate `/games/to-move` lists ongoing ones. Unverified |
| Variants | `variant` key: standard, chess960, crazyhouse, antichess, atomic, horde, kingOfTheHill, racingKings, threeCheck, fromPosition (`schemas/VariantKey.yaml`); filter via `perfType` | `rules` field: `chess`, `chess960`, plus bughouse/crazyhouse/kingofthehill/threecheck from recall. Unverified |
| Incremental sync | `since` / `until` (ms timestamps, min 1356998400070), `max`, `sort`. Spec says "games played since" but does not say which timestamp (created vs last move) is compared: **unverifiable here** | Monthly archives; the doc reportedly supports `ETag`/`If-None-Match` and `Last-Modified`/`If-Modified-Since` (304). Unverified |
| Auth | Spec lists `security: OAuth2: []` with **no scope**; anonymous allowed (rate limits are stated for anonymous). Any user's games are public | None: public data, no key. Unverified |
| Rate limits | Stream throttled: anonymous 20 games/s, OAuth 30 games/s, own games with token 60 games/s. Global: one request at a time; on 429 wait about 1 minute; send a `User-Agent` (`lichess-api.yaml:41-55`) | Serial requests are fine, parallel may get 429, from recall. Unverified |
| Other useful flags | `accuracy=true` (JSON only), `division=true` (middlegame/endgame plies, JSON only), `literate`, `tags`, `moves`, `lastFen`, `rated`, `color`, `vs`, `ongoing` (delayed 3-60 s) | n/a |

Key point for the spec: on Lichess, clocks and evals are **opt-in per request**. A request that omits `clocks=true`
returns no per-move time at all, and that cannot be recovered later without re-downloading.

## B. What the app fetches and keeps

### Fetch path

| Item | Lichess | Chess.com | Cite |
|---|---|---|---|
| URL built | `/api/games/user/{player}` plus `?since={ms}` only when a cursor exists | `/pub/player/{p}/games/archives`, then every monthly URL at or after the cursor month | `src/utils/lichess/api.tsx:437-440`; `src/utils/chess.com/api.tsx:99-103,168-181` |
| Params used | `since` only. No `clocks`, `evals`, `opening`, `max`, `perfType`, `Accept` | none | `lichess/api.tsx:437-440`; no `Accept` set at `:443-451` |
| Transport | Rust `download_file`, streamed to `db/{player}_lichess.pgn`, optional `Authorization: Bearer` | JS `plugin-http` fetch, `User-Agent: Pawn Appetit`; writes `.pgn` from each game's `pgn` field only | `src-tauri/src/fs.rs:32-40,79-85,126-163`; `api.tsx:15-17,203-205` |
| Auth | Token optional; OAuth scope requested is `preference:read`, which the export does not need per spec. Token only buys 20 -> 30/60 games/s | none | `src-tauri/src/oauth.rs:71`; `AccountCard.tsx:517`; `fs.rs:81-83` |
| User-Agent | none set on the Rust client (spec asks for one) | set | `fs.rs:75-77`; `api.tsx:15-17` |
| Rate limit / 429 | non-2xx becomes a hard error, no retry or backoff | none; no retry | `fs.rs:87-92`; `api.tsx:185-201` |
| ETag / Last-Modified | n/a | not used | `api.tsx:185-188` |
| Size limit | 10 GiB cap, progress estimated at 900 bytes per game | none | `fs.rs:21`; `lichess/api.tsx:450` |

### Sync is "incremental" by start time of the newest stored game

- The cursor is the Date + UTCTime of the newest stored game, found by querying the account DB sorted by date desc
  (`AccountCard.tsx:259-274`, `src/utils/lastGameDate.ts:12-22`). Used at `AccountCard.tsx:511-520`.
- Lichess: cursor goes straight into `since` (`lichess/api.tsx:438-439`).
- Chess.com: only the cursor's **month** is used; archives from that month on are re-fetched in full (`api.tsx:168-181`).
  The cursor is not passed to the importer: `convert` calls `convertPgn(filepath, dbPath, null, ...)` so the
  importer's timestamp filter is off (`AccountCard.tsx:152`; filter code `src-tauri/src/db/pgn.rs:319-334`).
- Re-fetched games are deduplicated by (WhiteID, BlackID, Date, UTCTime), and only when both date and time parse
  (`src-tauri/src/db/import.rs:30-46,63-88`). A game lacking UTCTime is never deduplicated.
- Sync is manual (a button). Nothing runs in the background (`AccountCard.tsx:495-536`).
- The Dashboard separately fetches only the **last** monthly archive, top 50 by `end_time`, for the recent-games widget
  (`api.tsx:105-166`; `src/features/dashboard/DashboardPage.tsx:237-246`). It is not the import path.

### What is kept vs dropped when stored

| Data | Kept? | Why / cite |
|---|---|---|
| Moves (main line + variations) | Kept, binary-encoded in `Games.moves` | `db/pgn.rs:58-90,341-366`; `db/import.rs:126` |
| PGN comments, including `[%clk ...]` / `[%eval ...]` if present in the file | **Kept** as `Comment` nodes inside the moves blob (not as columns; no clock/eval column exists, so no SQL query over clocks) | `db/pgn.rs:345-349,78-81`; `NewGame` fields `db/import.rs:107-128` |
| NAGs | Kept | `db/pgn.rs:351-353` |
| Lichess `%clk`, `%eval`, `Opening` | **Never downloaded** (flags absent), so absent from the DB | `lichess/api.tsx:437-440` |
| Chess.com `%clk` | Survives the importer if Chess.com puts it in `pgn` (unverified at source); app writes the `pgn` string verbatim | `api.tsx:203`; `pgn.rs:345` |
| Chess.com JSON siblings of `pgn` (`time_class`, `rules`, `accuracies`, `fen`, `initial_setup`, `rated`, `url`) | **Dropped** at export-to-file; only `g.pgn` is written. The URL is lost (the dashboard path keeps it, the import path does not) | `api.tsx:203` |
| Lichess JSON-only data (`clocks` JSON, `analysis`, `accuracy`, `division`, `daysPerTurn`) | Not requested; PGN requested | `lichess/api.tsx:437-451` |
| Header tags stored | White, Black, WhiteElo, BlackElo, TimeControl, ECO, Round, Date/UTCDate, UTCTime, EndDate, EndTime, Site, Event, Result, FEN | `db/pgn.rs:267-296` |
| Header tags dropped | `Opening`, `Variant`, `Termination`, `TimeControl` is kept but there is no time-class column, `WhiteRatingDiff`, `LichessURL` etc. Opening name is therefore not stored, only ECO | `db/pgn.rs:267-316` (no match arms) |
| Variant games | No variant handling anywhere in Rust. Movetext is parsed as standard chess; a game whose moves are illegal in standard rules (crazyhouse drops, etc.) hits `to_move` error and is **silently discarded** | `db/pgn.rs:381-390` (returns `None`); grep `Variant` in `src-tauri/src` finds none. Chess960 FEN setup is built with `CastlingMode::Standard`, a failure sets `skip` (`pgn.rs:303-310`). Exact fate of 960 games not tested |
| Per-game clock/eval for time-management analysis | Chess.com: probably present inside comments; Lichess: absent | see above |

Dependency: any "clock" feature has to parse `[%clk]` out of stored comments at read time (`pgn.rs:184-185` prints
them back) or move to a typed column.

## C. Upstream bug #440 root cause

Issue (Pawn-Appetit/pawn-appetit#440, 2026-10): "not all games are imported from chess.com, mostly daily games are not
imported". Body: daily games missing after connecting Chess.com; competing apps import them. One comment, "Am I the
only one?". No logs, version, or repro; no maintainer analysis. **No confirmed root cause is possible from the
issue.** Hypotheses ranked by code evidence, none reproduced (Chess.com API unreachable):

1. **Strict per-month schema; one non-conforming game aborts the whole sync (best-supported).**
   `ChessComGameSchema` requires `url, time_control, end_time, rated, initial_setup, fen, rules, white{rating,result,username}, black{...}`
   (`api.tsx:44-61`). `safeParse` runs on the whole month (`api.tsx:189`); on failure the code shows a notification and,
   unless the HTTP status is 404, executes `return` (`api.tsx:191-201`). Result: that month and **every later month**
   are skipped, the `.pgn` was already truncated (`api.tsx:174-176`), and `convert` still runs on the partial file
   (`AccountCard.tsx:520-527`). Any month containing a game where one of these fields is missing or differently typed
   would drop. Whether daily games lack any of them is **unverified**. This would fit "mostly daily games" only if
   daily records differ in shape. A 200-status invalid response is never retried.
2. **Unawaited writes race the converter.** `writeTextFile(file, "", {append:false})` and each append are not awaited
   (`api.tsx:174-176,203-205`). `downloadChessCom` returns before the writes settle, then `convert(p)` reads the file
   (`AccountCard.tsx:520-527`). Last months can be missing or the truncate can land after appends. Also games are
   joined with `"\n"` and the chunk has no trailing newline (`api.tsx:203`), so the first game of the next month may
   be glued to the last result token of the previous one; whether the PGN reader tolerates that is unverified.
   This loses games at month boundaries but has no reason to favour daily games.
3. **Daily games have a different time identity.** Dedupe and the cursor use `Date` + `UTCTime` (start time)
   (`import.rs:63-88`, `lastGameDate.ts:12-22`). A daily game can last days to weeks, finishes in a later month
   archive than it starts. Since dedupe never drops a game that lacks parseable time (it inserts it), this causes
   duplicates, not loss. The cursor month compares against archive months, which is conservative (same or earlier
   month than end), so it does not lose daily games either. Listed to rule it out.
4. **Live/Chess.com games for variants.** Not specific to daily: see variant row above.
5. **Dashboard path only reads the last archive** (`api.tsx:115-116`); a daily game finishing in the previous
   month never shows there. That is the widget, not the import, but a reporter could conflate them. Plausible user-facing
   confusion, unverified.

To settle it, someone needs a real Chess.com archive containing a finished daily game and to run it through
`ChessComGameSchema` and `convert_pgn`; neither was possible here.

## D. Implications for the spec ("Player improvement loop")

1. **Time management is only possible if the export is asked for it.** Lichess needs `clocks=true` on the request
   (opt-in, default false). Current code does not send it, so no stored Lichess game has clocks. Existing users must
   re-download; there is no way to backfill from the DB. The same applies to `evals=true` and `opening=true`.
2. **Clocks live only inside the moves blob as comments.** To compute time per move, per phase, or time trouble the
   spec must say where clocks are parsed (read-time parse of `%clk` comments vs a new typed column / table).
   Chess.com clock presence needs confirming against the primary doc and a real archive.
3. **Lichess evals are free where they exist.** With `evals=true`, an analysed game has `[%eval]` per move
   and judgments, which could seed Mistake Positions without re-running the engine. Only analysed games have them, so
   a Game Review still has to analyse the rest. Chess.com export has no evals (unverified): all analysis is local.
4. **Incremental sync is workable on both sides but not background-ready.** Lichess `since`/`until`/`max` fit a
   cursor well; Chess.com needs month-level fetch (current cursor re-fetches the whole current month, relying on dedupe).
   ETag/Last-Modified on archives would cut that cost (unverified, unused today). Today sync is a manual button with
   no retry, no 429 handling, no User-Agent on the Rust client; a background Game Review needs all of: serialized
   requests, 429 backoff, resumable cursor, per-month error isolation (not a `return`).
5. **Cursor semantics need a decision for slow games.** The cursor is the *start* time of the newest stored game. For
   Lichess correspondence, whether `since` filters by creation or last-move time is not stated in the spec; if it is
   creation time, a correspondence game created before the cursor but finished after would never sync. Test before relying on it.
6. **Daily/correspondence games are a first-class case for a 2000+ player? Probably not for time analysis.** Their
   clocks are days, not seconds; the spec should exclude `correspondence`/daily from time-management stats but still
   keep them for opening/endgame weakness mining. Both exports carry them (Lichess verified; Chess.com unverified).
7. **Variants** are not handled by the importer and fail silently. The improvement loop should filter to standard
   chess at fetch time (`perfType` on Lichess, `rules` on Chess.com) rather than rely on silent drops.
8. **#440 is a sync-completeness bug that directly undermines the "Account sync" stage.** Any spec that counts
   Mistake Positions per game needs a trustworthy imported-games count. Add a reconciliation check (archive game counts
   vs stored counts) and fix the abort-on-first-bad-month behaviour before building on top. Not fixed here.
9. **Token scope**: `preference:read` is requested (`oauth.rs:71`) but export needs none. A token only raises the
   throttle; the spec needs no new scope for syncing a user's own games, and should not request extra ones.
10. **Opening names are not stored** (only ECO). Opening-based Weaknesses need either `opening=true` on Lichess or an
    ECO-to-name lookup.
