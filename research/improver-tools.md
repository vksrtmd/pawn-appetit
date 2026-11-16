# What 2000+ improvers use and lack (ticket #9)

Vocabulary follows `GLOSSARY.md` (Player, Account, Game Review, Error, Mistake Position, Weakness, Card).

## Read this first: evidence quality

The research proxy reached only `github.com`, `api.github.com` and `raw.githubusercontent.com` for page fetches. Every other host failed (see "Blocked hosts" at the end). Search-engine result snippets still came through, so each claim carries one of these tags:

- **[P]** Primary: read directly from the source (lila source on GitHub, GitHub issues, Pawn Appétit code).
- **[S]** Snippet only: a search engine quoted the page, but I could not open the page. The URL is real, the wording is not verified, and the claim should be re-checked before it goes in the spec.
- **[U]** Unverifiable: the primary source (product site or docs) was blocked and no snippet covered it.

The "2000+" slice is a gap in the evidence. I found no thread where a 2000+ player describes their own toolchain. The r/chess searches returned nothing and reddit.com is blocked. The forum posters I did reach are mostly not 2000+, and one review says outright that Aimchess targets 800-1800. Treat "what 2000+ players do" as an inference, not an observation.

## Update 2026-11-16: re-run from an unrestricted network

Most blocked hosts were reachable in a second run (outside the proxy). That run overturns implication 1: **retrying your own mistakes across games, with spaced repetition, already exists.** The gap is narrower than first stated.

**Existing own-mistake retry tools** [P unless marked]
- **Aimchess "Retry Mistakes"**: built from the Player's past mistakes. The homepage says it finds the most common opening mistakes and uses spaced repetition to teach the right move. It re-shows a missed puzzle a little later. https://aimchess.com, https://apps.apple.com/tt/app/aimchess/id1524941307
- **Demand and complaints, first-hand**: a Chess.com user who wants to drill lots of "_my own_ past mistakes/blunders" calls Aimchess the closest thing. Two complaints: it pulls mistakes from too far back, so they're irrelevant, and it treats everything but the best move as a full blunder. https://www.chess.com/forum/view/general/app-for-drilling-your-past-blunders?lc=1
- **Blunders.ai**: spaced repetition over tactics and blunders from your own games. https://huntscreens.com/pt/products/blundersai
- **chess-self-coach** (open source): fetches Lichess and Chess.com games, finds blunders with Stockfish and tablebases, drills the correct moves with spaced repetition. https://pypi.org/project/chess-self-coach/0.3.11/
- **Chessigma Blunder Shield**: positions from your games where you were about to hang a piece. The page doesn't say how drills are scheduled. https://chessigma.com/supercoach
- **Masterboard** [U]: advertises as free, local-first, with FSRS training on your own blunders, which would make it the closest competitor. Only an admin login page was reachable, so what it ships is unconfirmed. https://masterboard-internal.pages.dev/

**Re-checked claims from the first run**
- Aimchess "symptoms, not causes": supported. An independent review says its advice is diagnosis ("practice rook endgames") rather than pedagogy. https://aicentral.substack.com/p/experience-beats-analysis
- Aimchess targets 800–1800: **not confirmed**. The homepage speaks only of improving your rating. Drop the claim. Pricing: free tier with a monthly 40-game report; Premium $7.99/month.
- Chess.com Game Review distrust: confirmed, mainly of the estimated rating. Users say it inflates (75 accuracy at 2000 estimated near 2200) and anchors on actual rating; another calls it "just a toy" and notes the accuracy formula changed. https://www.chess.com/forum/view/game-analysis/is-chess-coms-rating-estimator-accurate, https://www.chess.com/forum/view/for-beginners/why-600-on-chess-com-feels-off-a-data-driven-look-at-the-rating-landscape?lc=1
- "Game Review disagrees with Insights": no source found. Drop it.

**ChessBase, Chessable, Chess Tempo, Listudy** (were [U])
- **Chess Tempo**: likely the strongest self-study tool. Sets target your previous mistakes, repeat until correct, and support spaced repetition; the repertoire trainer is spaced-repetition too. The mistakes are on Chess Tempo's own puzzles, not your games. One commenter rates its opening trainer above Chessable's for managing your own repertoire. https://www.iphoneaddict.fr/apps/education/chesstempo-train-learn-play.html, https://zwischenzug.substack.com/p/spaced-repetition/comments
- **Chessable**: a course store built on the MoveTrainer drill. Usual complaint: hard to create and update your own material. Doesn't import your games. Some players build a custom course of their own blunders by hand, the same manual export as Chessli to Anki. https://zwischenzug.substack.com/p/spaced-repetition, https://ankichess.pages.dev/blog/ankichess-vs-chessable-vs-listudy, https://nickvasquezmd.substack.com/p/analyzing-our-games/comments
- **Listudy**: free, open source, spaced repetition over uploaded PGNs. By its own account it helps only with memorising openings. One user notes there are no review reminders. https://listudy.org/de/blog/learning-chess-openings, https://listudy.org/nl/blog/memorizing-chess-openings
- **ChessBase '26**: Style Report, Error Report with a "Blunder Elo" (strength in complex positions from error rate, a reference point), and Identify Player matching Lichess and Chess.com usernames. The player's guide describes no own-mistake training and no spaced repetition. Its player profiling bears on #16 (opponent prep). https://en.chessbase.com/post/chessbase-2026-a-players-guide-3
- **En Croissant**: FSRS for repertoire training only, nothing for own mistakes, so Pawn Appétit inherits none. https://www.mintlify.com/franciscoBSalgueiro/en-croissant/features/repertoire-training

**What 2000+ players use** (still mostly inference)
- A 2000-rated player reviews with Chess.com analysis and saves misplayed positions as flashcards. He wrote the app he uses, and the article doesn't mention spaced repetition. https://chessvision.ai/blog/how-i-reached-2000-on-chess-com-studying-with-my-own-app/
- A coach describes the common setup: all games in one Lichess Study, instructive positions from your own games in another. https://zwischenzug.substack.com/p/the-4-lichess-studies-i-make-for
- Pattern: manual capture, then review. Reddit and the Lichess forums weren't searched directly; no survey found.

**Revised implications** (replace 1 and amend 3 below)
1. "Retry my Errors across games, spaced" is **not** a differentiator by itself. Aimchess, Blunders.ai, chess-self-coach and possibly Masterboard ship it. What still looks unclaimed is the combination: a free desktop app; own Mistake Positions scheduled with FSRS, the same scheduler as Repertoire Review; recent Errors first; lenient grading of non-best moves; and Weaknesses that name a cause.
3. Weakness-names-a-cause is now better supported (independent review of Aimchess).
- New: Card grading must accept good-enough moves. A strict "only the best move" grade is a named complaint about Aimchess.
- New: favour recent Mistake Positions; stale ones are a named complaint.
- New: ChessBase's "Blunder Elo" is prior art for the reference-point problem (implication 4).

## Per tool

### Lichess Insights, Learn from your mistakes, Studies

- **Insights** [P]: `lila/modules/insight/src/main/InsightDimension.scala` lists the slicing dimensions: period, perf, phase, result, termination, colour, opening family and variation, opponent strength, piece role, move-time range, own and opponent castling, queen trade, material range, eval range, centipawn-loss range, accuracy range, win-percent range, clock-percent range, game source. A 2000+ player can slice their own games finely, for example blunders by phase and piece. https://raw.githubusercontent.com/lichess-org/lila/master/modules/insight/src/main/InsightDimension.scala
- **Learn from your mistakes** [S]: replays the Errors of one just-played game as a retry exercise. https://lichess.org/forum/lichess-feedback/how-to-use-learn-from-your-mistakes-y9Zs
- **Lichess puzzles use a spaced-repetition (Leitner, 3 boxes) replay of failed puzzles** [S]: https://lichess.org/@/lichess/blog/puzzles-update/WDY6cCEA (the snippet says the post is old and may no longer match the current system).
- **Complaints and requests**
  - Insights gives numbers with no reference point, so "75% endgame accuracy" can't be judged against a peer group; the poster says this makes Insights "kind of useless" for finding strengths and weaknesses [P]. https://github.com/lichess-org/lila/issues/16950 (open, tizwe, 2025-02-10)
  - Insights misses some rated games [P]. https://github.com/lichess-org/lila/issues/5704 (open, 2019)
  - The "Time Pressure" metric is unclear, and the poster wants minimum time left [P]. https://github.com/lichess-org/lila/issues/20971 (open, 2026-07-19)
  - Learn from your mistakes covers only the latest game. The request is to run it across ALL analysed games, taking any mistake not yet reviewed [P]. https://github.com/lichess-org/lila/issues/6980 (closed, which suggests it was not built, but the close reason was not read) and the original thread https://lichess.org/forum/lichess-feedback/create-learn-from-your-mistakes-across-all-games [S]
  - Puzzle generation from own games was requested in 2017 [P]. https://github.com/lichess-org/lila/issues/3437 (closed). The linked forum thread, via snippet [S], notes the catch that you already know you erred when solving a puzzle from your own game: https://lichess.org/forum/lichess-feedback/puzzle-generator-from-own-games
  - A forum thread asks for a "Learn from own mistakes" tab that serves Insights-filtered mistakes as puzzles, with filters such as minimum move time and minimum centipawn loss [S]. https://lichess.org/forum/lichess-feedback/feature-suggestion-play-random-position-from-chess-insights
  - A third-party tool, Chessli (https://pypi.org/project/chessli), exports game mistakes to Anki [S]. That is a sign players build the spaced-repetition loop themselves.

### Aimchess

- Features [U]: aimchess.com was blocked. Secondary reviews [S] say it merges Lichess and Chess.com games, shows long-term trend dashboards and generates training plans/practice.
- A review says it is best for 800-1800 and that players above 2000 likely design their own training [S]. https://www.raindropchess.com/aimchess-review-does-personalized-chess-training-actually-work/
- A Chess.com forum user calls it an interesting puzzle trainer at the top levels around 2000 and says Insights are better on Lichess [S]; the thread is about 2,300 days old. https://www.chess.com/forum/view/site-feedback/aimchess-shouldnt-that-sort-of-feedback-be-available-to-members-already
- Complaint: it leans heavily on centipawn loss and so over-penalises practical human choices; it "describes symptoms, not causes" [S]. https://mychessplan.com/aimchess-review-2026-signal-vs-noise/ (an independent review, but a competitor-adjacent site, so a commercial bias is possible).

### Chess.com Game Review and Insights

Features [U] (docs blocked). Complaints, all [S] from chess.com forum threads, most posters not 2000+:

- Accuracy and estimated rating seen as unreliable or toy-like: https://www.chess.com/forum/view/general/game-review-estimated-game-rating-is-a-lie-and-deceit
- Game Review and Insights show different accuracy: https://www.chess.com/forum/view/help-support/game-review-info-doesnt-match-insights-info
- Analysis incorrect in complex positions and the fastest setting weak: https://www.chess.com/forum/view/game-analysis/incorrect-game-review-analysis
- Engine does not see the human plan: https://www.chess.com/forum/view/general/game-review-no-explanation
- Layout change hides the evaluation graph; Game-Phase Accuracy removed: https://www.chess.com/forum/view/site-feedback/detailed-critique-of-new-game-review-110227953
- Users still value the score graph, move classification and explanations (one reply, same set of threads).

### ChessBase, Chessable, Chess Tempo, Listudy

- All product pages and docs were blocked [U]. I found nothing on what 2000+ users value or dislike in them beyond a listing that names Chess Tempo and Listudy as Chessable alternatives and Listudy as spaced-repetition [S] (https://alternativeto.net/software/chessable). Do not use this for the spec without a re-run from an unrestricted network.
- Related but not primary for those products: the book "How to Study Chess on Your Own" (Kuljasevic) targets 1800-2100 and recommends positional puzzles, model-game replay and playing typical structures against an engine [S] (https://www.abebooks.com/9789083336626/Study-Chess-Own-Workbook-Volume-908333662X/plp).

### En Croissant (upstream of Pawn Appétit)

- Open requests on its tracker [P]:
  - Woodpecker-style repeat of puzzle sets: https://github.com/franciscoBSalgueiro/en-croissant/issues/453 (2024-12-04)
  - FSRS trainer over database lines (Chessable MoveTrainer style): https://github.com/franciscoBSalgueiro/en-croissant/issues/962 (2026-10-17)
  - Correct opening moves always graded "easy" by FSRS: https://github.com/franciscoBSalgueiro/en-croissant/issues/658
  - Repertoire practice rework: https://github.com/franciscoBSalgueiro/en-croissant/issues/58
- Takeaway: upstream users want spaced repetition for more than repertoire lines. None of the issues I read ask for Mistake Position Cards from own games.

## Cross-tool: recurring valued features and gaps

| Theme | Evidence | Tools that have it | Tag |
|---|---|---|---|
| Slice own games by many dimensions | lila Insights dimension list | Lichess Insights | P |
| Cross-platform (both Accounts) in one view | Aimchess reviews; Insights is Lichess-only | Aimchess | S |
| Retry Errors from own games | lila #6980, #3437, forum threads | Lichess (latest game only) | P/S |
| Retry Errors across all games, not one | lila #6980; forum thread; 2026-11-16 re-run | Aimchess, Blunders.ai, chess-self-coach | P |
| Peer-relative benchmark ("is 75% good for my rating") | lila #16950 | none found in Insights | P |
| Name the cause (pattern/Weakness), not just the stat | mychessplan review; improvemychess/chessdock summaries | partly Aimchess | S |
| Link diagnosis to a drill | search summary of Insights vs Aimchess | Aimchess (plans) | S |
| Spaced repetition of own-game mistakes | Chessli to Anki; 2026-11-16 re-run | Aimchess (openings), Blunders.ai, chess-self-coach; Masterboard unverified | P |
| Trustworthy Error grading (centipawn vs human plan) | Chess.com threads; Aimchess review | complaint against both | S |
| Data completeness and clear metric definitions | lila #5704, #20971 | complaint against Insights | P |

## Coverage: Pawn Appétit

Checked in `/Users/dmtrskv/work/claude/pawn-appetit/.claude/worktrees/research-improver-tools`.

| Capability (loop stage) | Status | Evidence |
|---|---|---|
| Account sync, Lichess and Chess.com game import | Covered (fetch and import; no incremental sync checked) | `src/utils/lichess/api.tsx` (`fetchLastLichessGames`, `downloadLichess`), `src/utils/chess.com/api.tsx`, `src/features/dashboard/components/{Lichess,ChessCom}GamesTab.tsx`, `src/features/accounts/` |
| Multiple Accounts pooled as one Player | Partial: per-account views exist (`WebsiteAccountSelector.tsx`); pooling across Accounts not verified | `src/features/accounts/components/PersonalCardPanels/` |
| Game Review (engine eval, Error grading, accuracy) | Covered | `src/utils/score.ts` (Blunder `??` at >20% win-chance loss, Mistake `?` >10%, Dubious `?!` >5%), `src/utils/chess.ts` (`getGameStats`, harmonic-mean accuracy), `src-tauri/src/chess/analysis.rs` |
| Batch Game Review of imported games | Covered | `src/features/dashboard/components/AnalyzeAllModal.tsx` |
| Persisted Game Reviews for online games | Covered, as an analysed-PGN file only | `src/utils/analyzedGames.ts` (`analyzed_games.json`) |
| Mistake Positions (extract Errors, retry) | Missing: no code found that stores Errors as retryable positions | search of `src/` and `src-tauri/src/` |
| Weaknesses (pattern across Mistake Positions) | Missing: no motif, theme or phase aggregation of Errors; per-opening win/draw/loss only | `src/features/accounts/components/PersonalCardPanels/OpeningsPanel.tsx` |
| Peer-relative benchmark | Missing | none |
| Cards: repertoire spaced repetition (FSRS) | Covered | `src/features/files/utils/opening.ts` (`ts-fsrs`), `src/components/panels/practice/PracticePanel.tsx`, `src/state/gameAtoms.ts` |
| Cards: Puzzles with spaced repetition | Missing: Puzzles come from puzzle DBs with stats but no scheduling found | `src-tauri/src/puzzle.rs`, `src/features/boards/components/puzzles/` |
| Cards: Mistake Positions | Missing (follows from Mistake Positions) | none |
| Lessons, Courses, Exercises | Covered | `src/features/train/` (`constants/practices.tsx`, `types/lesson-types.ts`) |
| Progress measurement | Partial: daily goals, streaks, achievements and puzzle-solved history. Nothing tracks Error rate or Weakness trend over time | `src/features/dashboard/components/DailyGoalsCard.tsx`, `PuzzleStatsCard.tsx`, `src/features/train/utils/progressManager.ts` |
| Suggestions that point at a Weakness | Partial: suggestions are rule-based (practice, puzzle streak, analyse last game, explore openings), not driven by Weaknesses | `src/features/dashboard/DashboardPage.tsx` (`suggestions`, ~line 290), `SuggestionsCard.tsx` |

## Implications for the spec

1. **Superseded by the 2026-11-16 update above.** The clearest unmet need, backed by primary sources, is "retry my Errors across all my games" (lila #6980) and "serve them as drills" (forum threads). This is the Mistake Position stage. The first-party tools I could verify cover only the latest game, so a cross-game version is a differentiator. Pawn Appétit already has the Error grading and batch review needed to feed it.
2. Decide the knowledge-of-the-answer problem up front: the forum thread notes you remember your own mistake. Scheduling a Mistake Position as a Card (delay, then spaced repeat) is a design answer.
3. Weakness needs a named cause (phase, piece, motif), not only a statistic. The Aimchess criticism "symptoms, not causes" and lila #16950 both point here. Pawn Appétit has no tagging yet.
4. A peer benchmark (lila #16950) is optional but a real complaint. It needs rating-band reference data that Pawn Appétit does not have.
5. Cross-platform pooling across Accounts is a stated reason people pay for Aimchess. Verify that Pawn Appétit pools Accounts before claiming it.
6. Error grading trust: Chess.com users distrust accuracy and rating estimates. Pawn Appétit's rules are in `src/utils/score.ts`. Document their thresholds and make them configurable, because a 2000+ Player will test them.
7. Progress measurement should track Error rate and Weakness trend, not only streaks and goals.
8. Re-run the research from an unrestricted network before the spec relies on any [S] or [U] claim, especially for Chessable, ChessBase, Chess Tempo, Listudy and r/chess.

## Blocked hosts

`lichess.org`, `aimchess.com`, `www.chess.com`, `forum.chess.com`, `www.chessable.com`, `chesstempo.com`, `en-croissant.com`, `listudy.org`, `www.reddit.com` / `old.reddit.com` (all failed or were refused; I did not work around them). Reachable: `github.com`, `api.github.com`, `raw.githubusercontent.com`, and web search snippets.
