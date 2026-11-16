import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import type { SetStateAction } from "react";
import type { PracticeData } from "@/state/atoms";
import type { TreeNode } from "@/utils/treeReducer";
import {
    buildFromTree,
    getCardForReview,
    type Position,
    positionSchema,
    updateCardPerformance,
} from "../opening";

const NOW = new Date("2026-01-15T12:00:00Z");
const DAY = 24 * 60 * 60 * 1000;

function node(
    fen: string,
    halfMoves: number,
    san: string | null,
    children: TreeNode[] = [],
): TreeNode {
    return {
        fen,
        move: null,
        san,
        children,
        score: null,
        depth: null,
        halfMoves,
        shapes: [],
        annotations: [],
        comment: "",
    };
}

// 1. e4 e5 2. Nf3 Nc6: White to move at halfMoves 0 and 2, Black at 1 and 3.
function repertoire(): TreeNode {
    return node("start", 0, null, [
        node("after-e4", 1, "e4", [
            node("after-e5", 2, "e5", [node("after-Nf3", 3, "Nf3", [node("after-Nc6", 4, "Nc6")])]),
        ]),
    ]);
}

function deckOf(positions: Position[]) {
    const deck = {
        data: { positions, logs: [] } as PracticeData,
        set(action: SetStateAction<PracticeData>) {
            deck.data = typeof action === "function" ? action(deck.data) : action;
        },
    };
    return deck;
}

function answer(deck: ReturnType<typeof deckOf>, i: number, grade: 1 | 2 | 3 | 4) {
    updateCardPerformance(deck.set, i, deck.data.positions[i].card, grade);
}

beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);
});

afterEach(() => {
    vi.useRealTimers();
});

describe("building Repertoire Review Cards", () => {
    test("White's Cards hold only the moves White plays", () => {
        const answers = buildFromTree(repertoire(), "white", []).map((c) => c.answer);
        expect(answers).toContain("Nf3");
        expect(answers).not.toContain("e5");
        expect(answers).not.toContain("Nc6");
    });

    test("Black's Cards hold only the moves Black plays", () => {
        const cards = buildFromTree(repertoire(), "black", []);
        expect(cards.map((c) => [c.fen, c.answer])).toEqual([
            ["after-e4", "e5"],
            ["after-Nf3", "Nc6"],
        ]);
    });
});

describe("choosing the next Card", () => {
    test("a new Card is due straight away", () => {
        const cards = buildFromTree(repertoire(), "black", []);
        expect(getCardForReview(cards)?.fen).toBe("after-e4");
    });

    test("nothing is returned when every Card is scheduled for later", () => {
        const deck = deckOf(buildFromTree(repertoire(), "black", []));
        answer(deck, 0, 4);
        answer(deck, 1, 4);
        expect(getCardForReview(deck.data.positions)).toBeNull();
    });

    test("a due Card that was reviewed before is picked ahead of a new Card", () => {
        const deck = deckOf(buildFromTree(repertoire(), "black", []));
        answer(deck, 1, 3);
        vi.setSystemTime(new Date(NOW.getTime() + 30 * DAY));
        // A Card created now, so its due date is the present moment.
        const fresh = buildFromTree(repertoire(), "black", [])[0];
        const positions = [fresh, deck.data.positions[1]];
        expect(getCardForReview(positions)?.fen).toBe("after-Nf3");
    });

    test("among due Cards reviewed before, the first in the deck is picked", () => {
        const deck = deckOf(buildFromTree(repertoire(), "black", []));
        answer(deck, 1, 3);
        vi.setSystemTime(new Date(NOW.getTime() + DAY));
        answer(deck, 0, 3);
        vi.setSystemTime(new Date(NOW.getTime() + 30 * DAY));
        expect(getCardForReview(deck.data.positions)?.fen).toBe("after-e4");
    });
});

describe("answering a Card", () => {
    function dueAfter(grade: 1 | 2 | 3 | 4): number {
        const deck = deckOf(buildFromTree(repertoire(), "black", []));
        answer(deck, 0, 3);
        vi.setSystemTime(new Date(deck.data.positions[0].card.due));
        const reviewedAt = Date.now();
        answer(deck, 0, grade);
        return new Date(deck.data.positions[0].card.due).getTime() - reviewedAt;
    }

    test("Again brings the Card back within the day", () => {
        expect(dueAfter(1)).toBeLessThan(DAY);
    });

    test("Hard, Good and Easy push the Card out in that order", () => {
        const hard = dueAfter(2);
        const good = dueAfter(3);
        const easy = dueAfter(4);
        expect(dueAfter(1)).toBeLessThan(hard);
        expect(hard).toBeGreaterThan(0);
        expect(hard).toBeLessThan(good);
        expect(good).toBeLessThan(easy);
    });

    test("Again on a Card already spaced out by days brings it back within the day", () => {
        const deck = deckOf(buildFromTree(repertoire(), "black", []));
        let gap = 0;
        while (gap <= DAY) {
            const reviewedAt = Date.now();
            answer(deck, 0, 3);
            const due = new Date(deck.data.positions[0].card.due).getTime();
            gap = due - reviewedAt;
            vi.setSystemTime(due);
        }
        const reviewedAt = Date.now();
        answer(deck, 0, 1);
        expect(new Date(deck.data.positions[0].card.due).getTime() - reviewedAt).toBeLessThan(DAY);
    });

    test("each further Good answer pushes the Card out further", () => {
        const deck = deckOf(buildFromTree(repertoire(), "black", []));
        const intervals: number[] = [];
        for (let i = 0; i < 4; i++) {
            const reviewedAt = Date.now();
            answer(deck, 0, 3);
            const due = new Date(deck.data.positions[0].card.due).getTime();
            intervals.push(due - reviewedAt);
            vi.setSystemTime(due);
        }
        expect(intervals).toEqual([...intervals].sort((a, b) => a - b));
        expect(new Set(intervals).size).toBe(intervals.length);
    });
});

describe("stored Cards", () => {
    function storedAndReadBack(positions: Position[]): Position[] {
        return positionSchema
            .array()
            .parse(JSON.parse(JSON.stringify(positions))) as unknown as Position[];
    }

    test("a reviewed Card read back from storage is still accepted", () => {
        const deck = deckOf(buildFromTree(repertoire(), "black", []));
        answer(deck, 0, 3);
        const stored = storedAndReadBack(deck.data.positions);
        expect(typeof stored[0].card.due).toBe("string");
        expect(stored.map((p) => [p.fen, p.answer, p.card.reps])).toEqual([
            ["after-e4", "e5", 1],
            ["after-Nf3", "Nc6", 0],
        ]);
    });

    test("a Card read back from storage is picked when due and can be answered again", () => {
        const deck = deckOf(buildFromTree(repertoire(), "black", []));
        answer(deck, 0, 3);
        answer(deck, 1, 3);
        const due = new Date(deck.data.positions[0].card.due).getTime();
        const stored = deckOf(storedAndReadBack(deck.data.positions));

        vi.setSystemTime(due);
        expect(getCardForReview(stored.data.positions)?.fen).toBe("after-e4");

        answer(stored, 0, 3);
        expect(new Date(stored.data.positions[0].card.due).getTime()).toBeGreaterThan(due);
        expect(stored.data.positions[0].card.reps).toBe(2);
    });
});
