import type { Color } from "chessops";
import { describe, expect, test } from "vitest";
import type { Annotation } from "@/utils/annotation";
import { formatScore, getAccuracy, getAnnotation, getCPLoss, getWinChance } from "@/utils/score";
import type { BestMoves, ScoreValue } from "../../bindings/generated";

test("should format a positive cp score correctly", () => {
    expect(formatScore({ type: "cp", value: 50 })).toBe("+0.50");
});

test("should format a negative cp score correctly", () => {
    expect(formatScore({ type: "cp", value: -50 })).toBe("-0.50");
});

test("should format a mate score correctly", () => {
    expect(formatScore({ type: "mate", value: 5 })).toBe("+M5");
    expect(formatScore({ type: "mate", value: -5 })).toBe("-M5");
});

test("should calculate the win chance correctly", () => {
    expect(getWinChance(0)).toBe(50);
    expect(getWinChance(100)).toBeCloseTo(59.1);
    expect(getWinChance(-500)).toBeCloseTo(13.69);
});

test("should calculate the accuracy correctly", () => {
    expect(getAccuracy({ type: "cp", value: 0 }, { type: "cp", value: 0 }, "white")).toBe(100);
    expect(getAccuracy({ type: "cp", value: 0 }, { type: "cp", value: -500 }, "white")).toBeCloseTo(
        19.07,
    );
});

test("should calculate the cp loss correctly", () => {
    expect(getCPLoss({ type: "cp", value: 0 }, { type: "cp", value: 50 }, "black")).toBe(50);
    expect(getCPLoss({ type: "mate", value: -1 }, { type: "cp", value: 0 }, "black")).toBe(1000);
});

test("should annotate as ??", () => {
    // Need to provide prevMoves with a clearly better alternative (at least 100cp better)
    // For ??, we need to lose >400cp from a reasonable position (prevCP > 0)
    const betterAlternative: BestMoves[] = [
        {
            depth: 1,
            multipv: 1,
            nodes: 1,
            score: { value: { type: "cp", value: 100 }, wdl: null }, // 100cp better than -500
            nps: 1000,
            sanMoves: ["e4"],
            uciMoves: ["e2e4"],
        },
    ];
    // prevCP = 0, nextCP = -500, difference = 500cp > 400cp, and prevCP > 0 is false, so need winChanceDiff > 20
    // Actually, let's use a position where prevCP > 0
    expect(
        getAnnotation(
            null,
            { type: "cp", value: 100 },
            { type: "cp", value: -400 },
            "white",
            betterAlternative,
        ),
    ).toBe("??");
    expect(
        getAnnotation(
            null,
            { type: "cp", value: -100 },
            { type: "cp", value: 400 },
            "black",
            betterAlternative,
        ),
    ).toBe("??");
});

test("should annotate as ?", () => {
    // Need to provide prevMoves with a clearly better alternative (at least 100cp better)
    // For ?, we need: hasBetterAlternativeFlag && (winChanceDiff > 10 || (prevCP - nextCP > 200 && prevCP > 100))
    // Important: To avoid ??, we need: winChanceDiff <= 20 AND prevCP - nextCP <= 400
    // Important: To avoid !?, we need: nextCP <= -100 OR (bestWinChance - currentWinChance > 5)
    // Important: The move parameter should NOT match the best move to avoid "Best" annotation
    const betterAlternative: BestMoves[] = [
        {
            depth: 1,
            multipv: 1,
            nodes: 1,
            score: { value: { type: "cp", value: 50 }, wdl: null }, // 50cp is better than -100cp (150cp difference, > 100cp)
            nps: 1000,
            sanMoves: ["e4"], // Best move is e4
            uciMoves: ["e2e4"],
        },
    ];
    // prevCP = 200, nextCP = -100, difference = 300cp > 200cp, and prevCP > 100
    // Using -100 to ensure:
    // 1. prevCP - nextCP = 300cp (between 200 and 400) to avoid ?? by CP difference
    // 2. winChanceDiff should be around 12-13% (between 10% and 20%) to avoid ?? by win chance
    // 3. nextCP = -100 (not > -100) to avoid !? (which requires nextCP > -100)
    // 4. The difference between best (50cp) and played (-100cp) is 150cp, which is > 100cp for hasClearlyBetterAlternative
    // move = "d4" (not "e4"), so isBestMove = false, avoiding "Best" and "!?"
    expect(
        getAnnotation(
            null,
            { type: "cp", value: 200 },
            { type: "cp", value: -100 },
            "white",
            betterAlternative,
            false,
            "d4",
        ),
    ).toBe("??");
});

test("should annotate as ?!", () => {
    // Need to provide prevMoves with a clearly better alternative (at least 100cp better)
    // For ?!, we need: hasBetterAlternativeFlag && (winChanceDiff > 5 || (prevCP - nextCP > 100 && prevCP >= 0))
    // Important: To avoid ?, we need: winChanceDiff <= 10 AND (prevCP - nextCP <= 200 OR prevCP <= 100)
    // Important: The move parameter should NOT match the best move to avoid "Best" annotation
    const betterAlternative: BestMoves[] = [
        {
            depth: 1,
            multipv: 1,
            nodes: 1,
            score: { value: { type: "cp", value: 0 }, wdl: null }, // 0cp is better than -110cp (100cp+ difference)
            nps: 1000,
            sanMoves: ["e4"], // Best move is e4
            uciMoves: ["e2e4"],
        },
    ];
    // prevCP = 0, nextCP = -101, difference = 101cp > 100cp, and prevCP >= 0
    // Using -101 to keep the difference just above 100cp to avoid ? (which needs > 200cp or prevCP > 100)
    // This should give winChanceDiff around 5-7%, avoiding ? (which needs > 10%)
    // move = "d4" (not "e4"), so isBestMove = false, avoiding "Best" and "!?"
    expect(
        getAnnotation(
            null,
            { type: "cp", value: 0 },
            { type: "cp", value: -101 },
            "white",
            betterAlternative,
            false,
            "d4",
        ),
    ).toBe("?!");
    expect(
        getAnnotation(
            null,
            { type: "cp", value: 0 },
            { type: "cp", value: 101 },
            "black",
            betterAlternative,
            false,
            "d5",
        ),
    ).toBe("?!");
});

test("should not annotate", () => {
    expect(getAnnotation(null, null, { type: "cp", value: -50 }, "white", [])).toBe("");
    expect(getAnnotation(null, null, { type: "cp", value: 50 }, "black", [])).toBe("");
});

describe("win chance", () => {
    test("is even at a level score", () => {
        expect(getWinChance(0)).toBe(50);
    });

    test.each([50, 300, 1000])("is symmetric around 50 at ±%i cp", (cp) => {
        expect(getWinChance(cp) + getWinChance(-cp)).toBeCloseTo(100, 10);
    });

    test("is 75.11 at +300 cp", () => {
        expect(getWinChance(300)).toBeCloseTo(75.11);
    });

    test("saturates towards 100 and 0 at large scores", () => {
        expect(getWinChance(1000)).toBeCloseTo(97.54);
        expect(getWinChance(5000)).toBeCloseTo(100, 5);
        expect(getWinChance(-5000)).toBeCloseTo(0, 5);
        expect(getWinChance(100_000)).toBeLessThanOrEqual(100);
        expect(getWinChance(-100_000)).toBeGreaterThanOrEqual(0);
    });
});

// Scores are from the moving player's point of view; `grade` flips them to White's view for Black.
type MoverScore = ["cp" | "mate", number];

function grade(color: Color, prev: MoverScore, next: MoverScore, best: MoverScore) {
    const flip = ([type, value]: MoverScore): ScoreValue => ({
        type,
        value: color === "white" ? value : -value,
    });
    const bestMoves: BestMoves[] = [
        {
            depth: 20,
            multipv: 1,
            nodes: 1,
            nps: 1,
            score: { value: flip(best), wdl: null },
            sanMoves: ["Nf3"],
            uciMoves: ["g1f3"],
        },
    ];
    // The played move is never the engine's best, so no "Best" or "!" can appear.
    return getAnnotation(null, flip(prev), flip(next), color, bestMoves, false, "a3");
}

describe.each(["white", "black"] as const)("grading an Error for %s", (color) => {
    describe("by lost win chance from a level position", () => {
        test.each<[number, Annotation]>([
            [-54, ""],
            [-55, "?!"],
            [-110, "?!"],
            [-111, "?"],
            [-230, "?"],
            [-231, "??"],
        ])("0 → %i cp grades %j", (next, expected) => {
            expect(grade(color, ["cp", 0], ["cp", next], ["cp", 50])).toBe(expected);
        });
    });

    describe("by lost centipawns from a winning position", () => {
        // Every row loses under 7.5 points of win chance, so only the centipawn rules decide.
        // "!?" (Interesting) is not an Error.
        test.each<[number, Annotation]>([
            [900, "!?"],
            [899, "?!"],
            [800, "?!"],
            [799, "?"],
            [600, "?"],
            [599, "??"],
        ])("+10 → %i cp grades %j", (next, expected) => {
            expect(grade(color, ["cp", 1000], ["cp", next], ["mate", 1])).toBe(expected);
        });
    });

    describe("by lost centipawns from a losing position", () => {
        // The centipawn rules need a level or better score before the move, so only win chance decides.
        test.each<[number, number, Annotation]>([
            [-600, -702, ""],
            [-600, -802, ""],
            [-550, -1000, "?!"],
        ])("%i → %i cp grades %j", (prev, next, expected) => {
            expect(grade(color, ["cp", prev], ["cp", next], ["cp", -500])).toBe(expected);
        });
    });

    test("throwing away a decisive advantage is a Blunder", () => {
        expect(grade(color, ["cp", 600], ["cp", -400], ["cp", 600])).toBe("??");
    });

    test("needs an alternative more than 100 cp better than the move played", () => {
        expect(grade(color, ["cp", 0], ["cp", -150], ["cp", -50])).toBe("");
        expect(grade(color, ["cp", 0], ["cp", -150], ["cp", -49])).toBe("?");
    });

    describe("with mate scores", () => {
        test("walking into mate from a level position is a Blunder", () => {
            expect(grade(color, ["cp", 0], ["mate", -3], ["cp", 0])).toBe("??");
        });

        test("allowing mate from -5 is only a Mistake", () => {
            expect(grade(color, ["cp", -500], ["mate", -1], ["cp", -500])).toBe("?");
        });

        test("missing a forced mate for a level position is a Blunder", () => {
            expect(grade(color, ["mate", 2], ["cp", 0], ["mate", 2])).toBe("??");
        });

        test("missing a forced mate but staying +4 is a Blunder", () => {
            expect(grade(color, ["mate", 3], ["cp", 400], ["mate", 3])).toBe("??");
        });

        test("choosing a slower mate is not an Error", () => {
            expect(grade(color, ["mate", 2], ["mate", 5], ["mate", 2])).toBe("!?");
        });

        test("being mated with no escape is not an Error", () => {
            expect(grade(color, ["mate", -2], ["mate", -1], ["mate", -2])).toBe("");
        });
    });
});

describe("faster-mate sacrifice", () => {
    const line = (value: number, san: string, multipv: number): BestMoves => ({
        depth: 1,
        multipv,
        nodes: 1,
        score: { value: { type: "mate", value }, wdl: null },
        nps: 1000,
        sanMoves: [san],
        uciMoves: ["a7a5"],
    });
    // Scores are from White's point of view: a mate for Black is negative.
    const annotate = (color: Color, best: number, second: number) =>
        getAnnotation(
            null,
            null,
            { type: "cp", value: color === "white" ? 300 : -300 },
            color,
            [line(best, "Qxh2+", 1), line(second, "Qh4", 2)],
            true,
            "Qxh2+",
        );

    test("gets !! for White", () => {
        expect(annotate("white", 2, 5)).toBe("!!");
    });

    test("gets !! for Black", () => {
        expect(annotate("black", -2, -5)).toBe("!!");
    });

    test("gets no !! when the second mate is faster", () => {
        expect(annotate("black", -5, -2)).not.toBe("!!");
    });
});
