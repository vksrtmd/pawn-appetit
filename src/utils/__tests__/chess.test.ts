import { expect, test, vi } from "vitest";
import type { Token } from "@/bindings";
import { createNode, defaultTree } from "@/utils/treeReducer";
import { ANNOTATION_INFO, type Annotation, NAG_INFO } from "@/utils/annotation";
import { getPgnHeaders, getVariationLine, hasMorePriority, parsePGN } from "@/utils/chess";

vi.mock("@/bindings", () => ({
    commands: {
        lexPgn: vi.fn<(pgn: string) => Promise<unknown>>(async (pgn) => ({
            status: "ok",
            data: pgn
                .split(" ")
                .filter(Boolean)
                .map((value) => ({ type: "San", value })),
        })),
    },
}));

test("parsePGN keeps UCI moves, as puzzle analysis passes them", async () => {
    const tree = await parsePGN("e2e4 e7e5");

    expect(tree.root.children[0]?.san).toBe("e4");
    expect(tree.root.children[0]?.children[0]?.san).toBe("e5");
});

test("NAGs are consistent", () => {
    for (const k of Object.keys(ANNOTATION_INFO)) {
        if (k === "") continue;
        const nag = ANNOTATION_INFO[k as Annotation].nag!;
        expect(NAG_INFO.get(`$${nag}`)).toBe(k);
    }
});

test("priority comparison", () => {
    expect(hasMorePriority([0, 0], [0])).toBe(false);
    expect(hasMorePriority([0], [0, 0])).toBe(true);
    expect(hasMorePriority([0], [1])).toBe(true);
    expect(hasMorePriority([1], [0])).toBe(false);
    expect(hasMorePriority([0, 0], [0, 1])).toBe(true);
    expect(hasMorePriority([0, 1], [0, 0])).toBe(false);
    expect(hasMorePriority([0, 1], [0, 2])).toBe(true);
    expect(hasMorePriority([0, 2], [0, 1])).toBe(false);
});

test("PGN orientation detection from FEN", () => {
    // Test 1: Black to move without Orientation tag should get "black"
    const tokensBlackToMove: Token[] = [
        { type: "Header", value: { tag: "Event", value: "Test Game" } },
        {
            type: "Header",
            value: { tag: "FEN", value: "r5rk/ppqR3p/8/4N2P/3P4/2P2nPK/P2Q4/R7 b - - 1 1" },
        },
    ];
    const headersBlackToMove = getPgnHeaders(tokensBlackToMove);
    expect(headersBlackToMove.orientation).toBe("black");

    // Test 2: White to move without Orientation tag should get "white"
    const tokensWhiteToMove: Token[] = [
        { type: "Header", value: { tag: "Event", value: "Test Game" } },
        {
            type: "Header",
            value: {
                tag: "FEN",
                value: "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR w KQkq e3 0 1",
            },
        },
    ];
    const headersWhiteToMove = getPgnHeaders(tokensWhiteToMove);
    expect(headersWhiteToMove.orientation).toBe("white");

    // Test 3: Existing Orientation tag should be preserved regardless of FEN
    const tokensWithOrientation: Token[] = [
        { type: "Header", value: { tag: "Event", value: "Test Game" } },
        { type: "Header", value: { tag: "Orientation", value: "white" } },
        {
            type: "Header",
            value: { tag: "FEN", value: "r5rk/ppqR3p/8/4N2P/3P4/2P2nPK/P2Q4/R7 b - - 1 1" },
        },
    ];
    const headersWithOrientation = getPgnHeaders(tokensWithOrientation);
    expect(headersWithOrientation.orientation).toBe("white");

    // Test 4: No FEN tag should default to "white"
    const tokensNoFen: Token[] = [{ type: "Header", value: { tag: "Event", value: "Test Game" } }];
    const headersNoFen = getPgnHeaders(tokensNoFen);
    expect(headersNoFen.orientation).toBe("white");
});

test("variation line stops at the last existing move for a stale path", () => {
    const root = defaultTree().root;
    root.children = [
        createNode({
            fen: "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
            move: { from: 12, to: 28 },
            san: "e4",
            halfMoves: 1,
        }),
    ];

    expect(getVariationLine(root, [0, 3])).toEqual(["e2e4"]);
});
