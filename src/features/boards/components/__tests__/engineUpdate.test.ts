import type { BestMoves, Score } from "@/bindings";
import { expect, test, vi } from "vitest";
import { applyEngineUpdate } from "@/features/boards/components/engineUpdate";

test("an engine update with no lines reports progress and does not throw", () => {
    const setLines = vi.fn<(lines: BestMoves[]) => void>();
    const setScore = vi.fn<(score: Score) => void>();
    const setProgress = vi.fn<(progress: number) => void>();

    expect(() =>
        applyEngineUpdate({ progress: 100, lines: [], setProgress, setLines, setScore }),
    ).not.toThrow();

    expect(setProgress).toHaveBeenCalledWith(100);
    expect(setLines).not.toHaveBeenCalled();
    expect(setScore).not.toHaveBeenCalled();
});

test("an engine update with lines stores them and scores from the first", () => {
    const score = { value: { type: "cp", value: 12 } };
    const lines = [{ score }, { score: { value: { type: "cp", value: 5 } } }] as never[];
    const setLines = vi.fn<(lines: BestMoves[]) => void>();
    const setScore = vi.fn<(score: Score) => void>();
    const setProgress = vi.fn<(progress: number) => void>();

    applyEngineUpdate({ progress: 40, lines, setProgress, setLines, setScore });

    expect(setProgress).toHaveBeenCalledWith(40);
    expect(setLines).toHaveBeenCalledWith(lines);
    expect(setScore).toHaveBeenCalledWith(score);
});
