import { parseSquare } from "chessops";
import { describe, expect, test, vi } from "vitest";
import type { Token } from "@/bindings";
import { ANNOTATION_INFO, type Annotation, NAG_INFO } from "@/utils/annotation";
import {
    getMoveDestination,
    getPgnHeaders,
    hasMorePriority,
    parseKeyboardMove,
    parsePGN,
} from "@/utils/chess";

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

test("castling destination ignores a check or mate suffix", () => {
    const white = (san: string) =>
        getMoveDestination({ san, halfMoves: 1, move: { from: 4, to: 7 } });
    const black = (san: string) =>
        getMoveDestination({ san, halfMoves: 2, move: { from: 60, to: 63 } });
    expect(white("O-O")).toBe(parseSquare("g1"));
    expect(white("O-O+")).toBe(parseSquare("g1"));
    expect(white("O-O-O#")).toBe(parseSquare("c1"));
    expect(black("O-O+")).toBe(parseSquare("g8"));
    expect(black("O-O-O+")).toBe(parseSquare("c8"));
    expect(getMoveDestination({ san: "Nf3+", halfMoves: 1, move: { from: 6, to: 21 } })).toBe(21);
});

describe("parseKeyboardMove castling", () => {
    const fen = "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1";
    test.each(["0-0", "O-O", "o-o", "OO", "00"])("%s castles kingside", (input) => {
        expect(parseKeyboardMove(input, fen)).toEqual({ from: 4, to: 7 });
    });
    test.each(["0-0-0", "O-O-O", "o-o-o", "000"])("%s castles queenside", (input) => {
        expect(parseKeyboardMove(input, fen)).toEqual({ from: 4, to: 0 });
    });
});
