import { describe, expect, it, vi } from "vitest";
import type { BestMoves, Score } from "@/bindings";
import { createTreeStore, addAnalysis } from "@/state/store/tree";
import { defaultTree, type TreeState, treeIterator } from "@/utils/treeReducer";

vi.mock("@/utils/sound", () => ({ playSound: vi.fn() }));

const cp = (value: number): Score => ({ value: { type: "cp", value }, wdl: null });

function line(score: Score, uci: string[]): BestMoves {
    return { depth: 20, score, uciMoves: uci, sanMoves: [], multipv: 1, nps: 0, nodes: 0 };
}

function play(sans: string[]) {
    const store = createTreeStore(undefined, defaultTree());
    for (const san of sans) store.getState().makeMove({ payload: san });
    return store;
}

// One entry per position of the main line, root included.
function flat(n: number) {
    return Array.from({ length: n }, () => ({
        best: [line(cp(20), ["e2e4"])],
        novelty: false,
        is_sacrifice: false,
    }));
}

const review = (store: ReturnType<typeof play>, analysis: ReturnType<typeof flat>) => {
    const state = { root: structuredClone(store.getState().root) } as TreeState;
    addAnalysis(state, analysis);
    return state;
};

describe("Game Review keeps what the user made", () => {
    it("keeps a user variation and a user !? after results are applied", () => {
        const store = play(["e4", "e5", "Nf3"]);
        store.getState().goToStart();
        store.getState().makeMove({ payload: "d4" }); // user variation at the first move
        store.getState().goToStart();
        store.getState().makeMove({ payload: "e4" });
        store.getState().setAnnotation("!?");

        const state = review(store, flat(4));

        expect(state.root.children.map((n) => n.san)).toEqual(["e4", "d4"]);
        expect(state.root.children[0].annotations).toContain("!?");
    });

    it("removes only the marks and lines a previous review added", () => {
        const store = play(["e4", "e5", "Nf3"]);
        store.getState().goToStart();
        store.getState().makeMove({ payload: "d4" });
        store.getState().goToStart();
        store.getState().makeMove({ payload: "e4" });
        store.getState().setAnnotation("!");
        const userNodes = [...treeIterator(store.getState().root)].length;

        const drop = flat(4);
        drop[2] = { best: [line(cp(-900), ["g1f3"])], novelty: false, is_sacrifice: false };
        drop[3] = { best: [line(cp(-900), ["b8c6"])], novelty: false, is_sacrifice: false };
        const withReview = review(store, drop);
        const added = [...treeIterator(withReview.root)];
        const marks = added.flatMap((i) => i.node.annotations).length;
        expect(added.length + marks).toBeGreaterThan(userNodes + 1); // review added something

        addAnalysis(withReview, flat(4));

        // what is left is what a review of the user's own tree gives
        expect(withReview.root).toEqual(review(store, flat(4)).root);
        expect(withReview.root.children.map((n) => n.san)).toEqual(["e4", "d4"]);
    });

    it("does not throw when a middle position has no lines", () => {
        const store = play(["e4", "e5", "Nf3", "Nc6"]);
        const analysis = flat(5);
        analysis[2] = { best: [], novelty: false, is_sacrifice: false };
        expect(() => review(store, analysis)).not.toThrow();
    });
});

describe("deleteMove", () => {
    it.each([[[]], [[5]], [[0, 7]]])("leaves the tree unchanged for path %j", (path) => {
        const store = play(["e4", "e5"]);
        const before = structuredClone(store.getState().root);
        const pos = [...store.getState().position];
        store.getState().deleteMove(path);
        expect(store.getState().root).toEqual(before);
        expect(store.getState().position).toEqual(pos);
    });
});

describe("result is set only by the end of an unfinished main line", () => {
    const mating = ["f3", "e5", "g4", "Qh4#"];

    it("sets the result when the main line of a * game ends in mate", () => {
        expect(play(mating).getState().headers.result).toBe("0-1");
    });

    it("keeps 0-1 when a mating variation is played", () => {
        const store = play(["e4", "e5"]);
        store.getState().setResult("0-1");
        store.getState().goToStart();
        for (const san of mating) store.getState().makeMove({ payload: san });
        expect(store.getState().headers.result).toBe("0-1");
    });

    it("keeps * when a mating variation is played off the main line", () => {
        const store = play(["e4", "e5"]);
        store.getState().goToStart();
        for (const san of mating) store.getState().makeMove({ payload: san });
        expect(store.getState().headers.result).toBe("*");
    });

    it("does not apply the fifty-move rule to a game whose result is set", () => {
        const store = createTreeStore(undefined, defaultTree("4k3/8/8/8/8/8/8/R3K3 w - - 98 80"));
        store.getState().setResult("1-0");
        store.getState().makeMove({ payload: "Ra2" });
        store.getState().makeMove({ payload: "Kd8" });
        expect(store.getState().headers.result).toBe("1-0");
    });
});
