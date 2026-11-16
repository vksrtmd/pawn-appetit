# Pawn Appétit

A chess application that helps one person get better at chess: it gathers their games, finds what keeps costing them points, and schedules the training that fixes it.

## Language

### Who is improving

**Player**:
The one person the app is improving. Weaknesses, statistics and training belong to the Player, pooled across all their Accounts.
_Avoid_: User, main account, profile

**Account**:
One linked online profile (Lichess or Chess.com) that belongs to the Player and supplies their games.
_Avoid_: Profile, player

### Judging games

**Game Review**:
The engine's verdict on one game: its evaluations, the Errors found, and the summary drawn from them.
_Avoid_: Report, game report, analysis report

**Error**:
A move judged to lose value, graded as a Blunder, Mistake or Dubious move.
_Avoid_: Inaccuracy

**Blunder**:
The most severe Error: a move that throws away a large share of the winning chances.
_Avoid_: ??

**Mistake**:
A serious Error, less costly than a Blunder.
_Avoid_: ?

**Dubious**:
The mildest Error: a move that gives up a little without changing the outcome.
_Avoid_: Inaccuracy, ?!

**Mistake Position**:
A position from one of the Player's own games where they made an Error, kept so they can retry it.
_Avoid_: Own-game puzzle, mistake puzzle

**Weakness**:
A recurring pattern across many Mistake Positions, such as missing knight forks or losing won rook endgames.
_Avoid_: Flaw, issue, problem area

**Motif**:
A named tactical pattern, such as a fork or a pin, in the line the Player missed or allowed at a Mistake Position.
_Avoid_: Theme, tactic type

### Training

**Card**:
Anything the Player is scheduled to see again by spaced repetition: a repertoire move, a Mistake Position or a Puzzle.
_Avoid_: Flashcard, item, review item

**Repertoire Review**:
Spaced-repetition review of the Player's own opening lines.
_Avoid_: Practice, repertoire practice

**Puzzle**:
A tactic taken from a puzzle database rather than from the Player's own games.
_Avoid_: Exercise, problem

**Exercise**:
One hand-authored training position with a known solution, part of a Lesson or a Course.
_Avoid_: Practice, puzzle

**Lesson**:
A guided tutorial that explains an idea, then tests it with Exercises.
_Avoid_: Tutorial

**Course**:
A themed set of Exercises without guided explanation, such as checkmate patterns or rook endgames.
_Avoid_: Practice, practice category, category
