import { describe, expect, it, vi } from "vitest";
import { createTreeStore } from "@/state/store/tree";
import { defaultTree } from "@/utils/treeReducer";

vi.mock("@/utils/sound", () => ({ playSound: vi.fn() }));

function play(fen: string, sans: string[]) {
    const store = createTreeStore(undefined, defaultTree(fen));
    for (const san of sans) store.getState().makeMove({ payload: san });
    return store.getState().headers.result;
}

describe("tree draw detection", () => {
    it("draws on the fifty-move rule when the halfmove clock reaches 100", () => {
        expect(play("4k3/8/8/8/8/8/8/R3K3 w - - 98 80", ["Ra2", "Kd8"])).toBe("1/2-1/2");
    });

    it("does not draw when a capture resets the halfmove clock", () => {
        expect(play("4k3/r7/8/8/8/8/8/R3K3 w - - 98 80", ["Rxa7", "Kd8"])).not.toBe("1/2-1/2");
    });

    it("draws on a threefold repetition with no legal en passant after a double push", () => {
        const shuffle = ["Nf6", "Nf3", "Ng8", "Ng1"];
        expect(
            play("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", [
                "e4",
                ...shuffle,
                ...shuffle,
            ]),
        ).toBe("1/2-1/2");
    });

    it("does not draw when en passant was legal only in the first occurrence", () => {
        // Black can take e4 en passant right after e4; once the knights return it cannot.
        const fen = "4k1n1/8/8/8/3p4/8/4P3/4K1N1 w - - 0 1";
        const shuffle = ["Nf6", "Nf3", "Ng8", "Ng1"];
        expect(play(fen, ["e4", ...shuffle, ...shuffle])).not.toBe("1/2-1/2");
    });
});
