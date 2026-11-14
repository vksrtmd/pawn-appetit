# Prior art for automatic tactical motif tagging (ticket #11)

Feeds the "Weakness taxonomy and detection" decision. Terms follow `GLOSSARY.md` (Mistake Position, Weakness, Puzzle).
Retrieved 2026-11-14. Tags: **[verified]** read from the primary source this session; **[secondary]** search-engine summary only, not opened in full; **[unverifiable]** host blocked.

## 1. Lichess puzzle tagger (lichess-puzzler)

Source: <https://github.com/ornicar/lichess-puzzler> (dirs `bin generator project tagger validator`; licence AGPL-3.0 per repo page) [verified].
Code: `tagger/cook.py`, `tagger/model.py`, `tagger/util.py` (raw.githubusercontent.com/ornicar/lichess-puzzler/master/...).

### Inputs
`cook(puzzle: Puzzle) -> List[TagKind]`. A `Puzzle` has `id`, `game` (python-chess `Game`), `pov`, `mainline` (list of `ChildNode`: the puzzle position and the full solution line) and `cp` (final eval) (`tagger/model.py`) [verified].
So the tagger does not look at a bare position. It needs the **solution line**, both sides' moves until the tactic ends, plus the final eval (used for crushing/advantage/equality). The engine is not called inside the tagger; the generator produced the line beforehand.

### Motifs detected (functions in `cook.py`) [verified]
- Material/line tactics: `fork`, `pin` (via `pin_prevents_attack`, `pin_prevents_escape`), `skewer`, `x_ray`, `discovered_attack`, `discovered_check`, `double_check`, `hanging_piece`, `trapped_piece`, `deflection`, `attraction`, `interference` (and `self_interference` helper), `intermezzo`, `clearance`, `capturing_defender`, `sacrifice`, `collinear`, `quiet_move`, `defensive_move`, `check_escape`.
- Mating patterns: `back_rank_mate`, `anastasia_mate`, `hook_mate`, `arabian_mate`, `boden_or_double_bishop_mate`, `dovetail_mate`, `smothered_mate`, `mate_in` (mateIn1..5).
- Position/king: `exposed_king`, `attacking_f2_f7`, `kingside_attack`, `queenside_attack`, `advanced_pawn`.
- Special moves: `en_passant`, `castling`, `promotion`, `under_promotion`.
- Endgame type: `piece_endgame` (pawn/knight/bishop/rook/queen), `queen_rook_endgame`.
- Outcome/length: `crushing` (cp>600), `advantage` (cp>200), `equality`, `oneMove/short/long/veryLong`.
- `TagKind` also lists `overloading`, `coercion`, `zugzwang`, `simplification`, but `cook()` never emits `coercion`, `zugzwang`, `simplification`, and `overloading()` is a stub that always returns `False` [verified, `cook.py`, `model.py`].

### How detection works
Hand-written heuristics over python-chess boards walked along `puzzle.mainline`. E.g. `fork` iterates `mainline[1::2][:-1]` (the Player's moves), skips squares where `util.is_in_bad_spot`, counts attacked opponent pieces that are worth more than the mover or hanging, and fires if two or more targets (pawns skipped). `sacrifice` is a material-diff test (down 2+ points, no promotion). `deflection`, `attraction` etc. are similar short material/attack-set checks. [verified]

### Accuracy and limits
- No published accuracy evaluation found. The README has no tagger description, and `cook.py` has no accuracy/false-positive comments [verified]. Repo has `tagger/test.py`-style helpers that assert `cook.test` on specific puzzle ids (README snippet), i.e. regression tests on hand-picked puzzles, not precision/recall. [verified for the snippet; test contents not read]
- Limits by reading the code: depends on the engine-built solution line, so it tags only forced, winning sequences, not "the move you should have played if the engine line is long or quiet"; several themes never emitted (above); multi-theme positions get several tags with no ranking; heuristics ignore deeper tactics (e.g. fork must be on the Player's own move in the line).
- Generator (`generator/generator.py`) shows what engine line the tagger sees: main search `Limit(depth=50, time=30, nodes=25_000_000)`; mate defence `depth=15, time=10, nodes=8_000_000`; `mate_soon = Mate(15)`; advantage line needs score >= Cp(200) and a win-chance jump > 0.6; "only move" test is win_chances(best) > win_chances(second) + 0.7, so it needs **MultiPV >= 2**; Threads 4 default [verified]. Which multipv `util.get_next_move_pair` uses was not seen.

## 2. Other detectors

| Tool | What | Motifs | Evidence of accuracy | Licence |
|---|---|---|---|---|
| Chess Tempo | Human tagging: users add and vote tags, with modifiers like `Fork[Check,Knight]`. Not automatic. [secondary: search summary of chesstempo.com manual and tagging-guidelines; pages not fetched, `chesstempo.com/tagging-guidelines` fetch failed] | ~34 tags (2012 blog) | n/a | proprietary |
| dev-arcturus/positional_chess (<https://github.com/dev-arcturus/positional_chess>) | Rust (WASM) `engine-rs/src/motifs.rs` on **shakmaty**: given position before, after, the move, terminal flag. 70+ detectors (fork, pin, skewer, discovered/double check, Greek gift, named mates, trades, pawn structure, outposts, king attack, etc.) with SEE guards. [verified] | see left | CI harness requires precision >= 0.80, recall >= 0.70 per motif with >=3 corpus mentions; no corpus size or numbers published [verified] | **No licence file** (all rights reserved) [verified]; can only be read as inspiration |
| TacticRadar (<https://github.com/Mokshii46/TacticRadar>) | ML multi-label classifier on a FEN (after Moves[0]) for Fork/Pin/Skewer/DiscoveredAttack, 69 python-chess features; trained on the Lichess puzzle DB. [verified] | 4 | Best Random Forest: macro F1 0.51, micro F1 0.51, subset acc 27.7%; rule-based baseline macro F1 0.33 [verified, README metrics]. Weak. | CC0-1.0 badge [verified] |
| ChessGrammar (<https://chessgrammar.com/>) | Commercial API | fork/pin/skewer... | Claims 97.3% on 25k annotated positions; methodology unseen [secondary, vendor claim] | proprietary |
| chess-coach-ai PR #473 | Rule detectors measured vs labels: fork 97.5% confirmed, pin 100%, fork false fires 11.3% [secondary] | | one project's self-measurement | not checked |
| Papers | "Towards the Automatic Synthesis of Interpretable Chess Tactics" (arXiv 2610.07640): coverage 0.32 can_fork, 0.22 skewer, 0.79 pin [secondary; fetch failed]. Bizjak & Guid, "Automatic Recognition of Similar Chess Motifs" (ACG 2021): retrieval, top-1 0.252 static / 0.418 dynamic features [secondary; PDF fetch failed]. ChessQA benchmark uses deterministic detectors as gold labels [secondary]. | | | |
| Rust crates | Search found no standalone motif-detection crate on shakmaty [secondary]. `shakmaty` itself provides attacks/rays/legal moves only. | | | |

No peer-reviewed accuracy study of the Lichess tagger was found.

## 3. Could it run locally on Mistake Positions?

Facts from this repo:
- Backend uses `shakmaty = "0.27.3"` and `pgn-reader` (`src-tauri/Cargo.toml` lines 31-32); package licence `GPL-3.0-only` (line 6); `LICENSE` is GNU GPL v3.
- The analysis path already forces MultiPV=2 for the engine (`src-tauri/src/chess/analysis.rs` ~line 93-100), which matches what an "only move" check needs.
- Python is not in the Tauri bundle. Running `cook.py` would mean shipping a Python runtime plus python-chess (sidecar), or a port to Rust on shakmaty.

Needed per Mistake Position: the position, the Player's played move, and the engine's better line. To tag *what the Player missed* you need the engine refutation/best line (a few plies, MultiPV 2 at modest depth, e.g. 14-18, is a judgment, not a published figure; the Lichess generator uses depth up to 50 offline). The Lichess tags are meant for a *forced winning* line; for mistakes where the best line is quiet or positional, `cook` yields mostly `quietMove`/`advantage` and few motif tags.
Runtime: tagging is cheap Rust/Python board walking (milliseconds for a short line; not benchmarked here). The engine analysis dominates, and the app already runs it for game analysis.

### Licence and porting

| Option | Licence fit with this GPL-3.0 repo | Notes |
|---|---|---|
| Port `cook.py` to Rust (derivative of AGPL-3.0) | **Conditional.** GPLv3 section 13 and AGPLv3 section 13 explicitly allow combining GPLv3 and AGPLv3 works; the combined work keeps AGPL's network clause for the AGPL part. Not read from the licence texts this session, so confirm against the gnu.org text; also ask a human whether the project is willing to carry AGPL terms. [from knowledge, not verified this session] | Contributors/redistributors would then need to meet AGPL obligations for the ported part |
| Clean-room reimplementation from the theme definitions (docs of themes, not code) | Fine, no inheritance of AGPL | Definitions of "fork" etc. are not copyrightable; do not read cook.py while writing |
| Run `cook.py` as a separate sidecar process | Possible but still distributing AGPL code | heavy: Python runtime |
| positional_chess motifs.rs | **Blocked**: no licence | inspiration only |
| TacticRadar | CC0 | ML, weak accuracy; not useful |
| Write own detectors on shakmaty | Fine | Most control |

## 4. Lichess puzzle DB themes for picking Puzzles

- Lichess puzzle DB is a CSV with a themes column (and TacticRadar's README confirms it filters on `popularity` and `plays`, and uses puzzle themes as labels) [secondary]. The official page (`database.lichess.org`) **could not be fetched (socket closed): field list and licence unverifiable here** (commonly known as CC0, not verified).
- The app's Puzzle table has **no themes column**: `src-tauri/src/db/schema.rs` / `models.rs` define `puzzles(id, fen, moves, rating, rating_deviation, popularity, nb_plays)`. `src-tauri/src/puzzle.rs` imports `.db/.db3` copy, PGN (`parse_puzzles_from_pgn`, reading FEN header etc.) and compressed files; I found no theme/tag handling in it [verified by grep]. So themes are currently dropped, and matching Puzzles to a Weakness requires a schema migration plus importing the `Themes` column, or tagging Puzzles locally with the same tagger used on Mistake Positions.
- Using the same tagger on both sides keeps vocabularies aligned; using Lichess themes for Puzzles and a home-grown tagger for Mistake Positions needs a mapping to Lichess theme names (fork, pin, skewer, discoveredAttack, hangingPiece, backRankMate, ...).

## 5. Implications for the spec

1. Use the Lichess `TagKind` names as the Weakness vocabulary for tactics: it maps 1:1 onto Lichess Puzzle themes and Chess Tempo-style names.
2. Tagging needs an engine line per Mistake Position (best line at least ~3-4 plies, MultiPV 2). Decide whether analysis already stored provides this; otherwise add a background re-analysis step.
3. Expect tags only for forcing mistakes (missed forks, hanging pieces, mates, pins, skewers, discovered attacks, back-rank mates). Positional, strategic and endgame-technique Weaknesses need other signals (endgame type from material, phase, opening) not from `cook`.
4. No published accuracy: treat tags as hints and measure precision on a small hand-labelled sample of the target Player's Mistake Positions; the Weakness should require several occurrences, so single false tags wash out.
5. Prefer a clean-room Rust implementation on shakmaty for a short list (fork, pin, skewer, discovered attack, hanging piece, back-rank mate, mate-in-N, endgame type); a direct port needs a licensing decision by a human.
6. Matching Puzzles to a Weakness requires importing the theme column (schema change) or local tagging.

## Blocked / unverifiable
- `database.lichess.org` (socket closed twice): puzzle CSV schema, licence, theme list.
- `chesstempo.com` (tagging-guidelines fetch failed): Chess Tempo claims rest on a search summary.
- `arxiv.org` (2610.07640) and `icga.org` PDF: socket closed; numbers are from search summary only.
- AGPL/GPL compatibility clauses not read from gnu.org this session.
