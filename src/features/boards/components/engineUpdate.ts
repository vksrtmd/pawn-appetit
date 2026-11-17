import type { BestMoves, Score } from "@/bindings";

/** Apply an engine payload: progress always, lines and score only when there are lines. */
export function applyEngineUpdate({
    progress,
    lines,
    setProgress,
    setLines,
    setScore,
}: {
    progress: number;
    lines: BestMoves[];
    setProgress: (progress: number) => void;
    setLines: (lines: BestMoves[]) => void;
    setScore: (score: Score) => void;
}) {
    setProgress(progress);
    if (lines.length === 0) return;
    setLines(lines);
    setScore(lines[0].score);
}
