import { parseSquare } from "chessops";
import { describe, expect, test, vi } from "vitest";
import type { Token } from "@/bindings";
import { commands } from "@/bindings";
import { createNode, defaultTree } from "@/utils/treeReducer";
import { ANNOTATION_INFO, type Annotation, NAG_INFO } from "@/utils/annotation";
import {
    getMoveDestination,
    getPGN,
    getPgnHeaders,
    getVariationLine,
    hasMorePriority,
    headersToPGN,
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

function headerTokens(headers: Record<string, string>): Token[] {
    return Object.entries(headers).map(([tag, value]) => ({
        type: "Header",
        value: { tag, value },
    }));
}

test("PGN headers survive a parse and save round trip", () => {
    const headers = getPgnHeaders(
        headerTokens({
            Event: "Live",
            Annotator: "Me",
            UTCTime: "12:34:56",
            WhiteElo: "?",
            BlackElo: "-",
            ECO: "C20",
            PlyCount: "40",
            Custom: "x",
            SetUp: "1",
        }),
    );
    const pgn = headersToPGN(headers);
    expect(pgn).toContain('[UTCTime "12:34:56"]');
    expect(pgn).toContain('[WhiteElo "-"]');
    expect(pgn).toContain('[BlackElo "-"]');
    expect(pgn).not.toContain("NaN");
    expect(pgn).not.toContain("[SetUp");
    expect(pgn.match(/\[ECO /g)).toHaveLength(1);
    const names = [...pgn.matchAll(/^\[(\w+) /gm)].map((m) => m[1]);
    expect(names.slice(-4)).toEqual(["Annotator", "ECO", "PlyCount", "Custom"]);
    const again = getPgnHeaders(
        headerTokens(
            Object.fromEntries([...pgn.matchAll(/\[(\w+) "(.*)"\]/g)].map((m) => [m[1], m[2]])),
        ),
    );
    expect(again.other).toEqual(headers.other);
    expect(again.time).toBe("12:34:56");
});

test("invalid Result and Orientation fall back to * and white", () => {
    const headers = getPgnHeaders(headerTokens({ Result: "foo", Orientation: "sideways" }));
    expect(headers.result).toBe("*");
    expect(headers.orientation).toBe("white");
});

test("unusable rating tags become unrated and are written as a dash", () => {
    for (const value of ["?", "??", "-", "0", "", "N/A", "-5", "999999"]) {
        const headers = getPgnHeaders(headerTokens({ WhiteElo: value, BlackElo: value }));
        expect(headers.white_elo).toBe(0);
        expect(headers.black_elo).toBe(0);
    }
    for (const [value, expected] of [
        ["2100", 2100],
        ["02300", 2300],
        [" 2100 ", 2100],
    ] as const) {
        expect(getPgnHeaders(headerTokens({ WhiteElo: value })).white_elo).toBe(expected);
    }
    expect(getPgnHeaders(headerTokens({ Event: "x" })).white_elo).toBeUndefined();
    const base = getPgnHeaders(headerTokens({}));
    const pgn = headersToPGN({ ...base, white_elo: Number.NaN, black_elo: 0 });
    expect(pgn).toContain('[WhiteElo "-"]');
    expect(pgn).toContain('[BlackElo "-"]');
    expect(headersToPGN({ ...base, white_elo: undefined })).not.toContain("WhiteElo");
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

// A lexer as the Rust one behaves: header values and comment text come back raw
function lexRaw(pgn: string) {
    const tokens: Token[] = [];
    const re = /\[(\w+) "((?:[^"\\]|\\.)*)"\]|\{([^}]*)\}|(\S+)/g;
    for (const m of pgn.matchAll(re)) {
        if (m[1]) tokens.push({ type: "Header", value: { tag: m[1], value: m[2] } });
        else if (m[3] !== undefined) tokens.push({ type: "Comment", value: m[3] });
        else if (m[4] === "*") tokens.push({ type: "Outcome", value: "*" });
        else tokens.push({ type: "San", value: m[4] });
    }
    return { status: "ok" as const, data: tokens };
}

describe("parsePGN refuses to truncate", () => {
    test("an illegal move at ply 20 is an error, not a shortened game", async () => {
        const legal =
            "e4 e5 Nf3 Nc6 Bb5 a6 Ba4 Nf6 O-O Be7 Re1 b5 Bb3 d6 c3 O-O h3 Nb8 d4 Nbd7".split(" ");
        expect(legal).toHaveLength(20);
        await expect(parsePGN([...legal.slice(0, 19), "Qh8", "Nf3"].join(" "))).rejects.toThrow(
            /pgnParse/,
        );
    });
});

describe("PGN text that would break the format survives write then parse", () => {
    test("a comment with } and a header with a quote and backslash", async () => {
        const tree = defaultTree();
        tree.headers.white = 'Sm"ith \\ Jr';
        tree.headers.other = { Annotator: 'a "b" \\' };
        const e4 = parseSquare("e2");
        expect(e4).toBeDefined();
        tree.root.comment = "set {a} and \\} too";
        const pgn = getPGN(tree.root, {
            headers: tree.headers,
            glyphs: true,
            comments: true,
            variations: true,
            extraMarkups: true,
        });
        vi.mocked(commands.lexPgn).mockImplementationOnce(async (p) => lexRaw(p));

        const back = await parsePGN(pgn);

        expect(back.headers.white).toBe('Sm"ith \\ Jr');
        expect(back.headers.other?.Annotator).toBe('a "b" \\');
        expect(back.root.comment).toBe("set {a} and \\} too");
    });

    test("a backslash in a comment is written as is, and only } is escaped", () => {
        const root = defaultTree().root;
        root.comment = "a\\b} c";
        const pgn = getPGN(root, {
            headers: null,
            glyphs: false,
            comments: true,
            variations: false,
            extraMarkups: false,
        });
        expect(pgn).toContain("{a\\b\\u007d c}");
    });

    test("a backslash in a comment in a file from another program reads back as written", async () => {
        vi.mocked(commands.lexPgn).mockImplementationOnce(async (p) => lexRaw(p));
        const back = await parsePGN("{path C:\\dir and \\u007d} *");
        expect(back.root.comment).toBe("path C:\\dir and }");
    });

    test("Result and Orientation headers are escaped like the others", () => {
        const headers = {
            ...defaultTree().headers,
            result: '1-0"]' as never,
            orientation: 'w"' as never,
        };
        expect(headersToPGN(headers)).toContain('[Result "1-0\\"]"]');
        expect(headersToPGN(headers)).toContain('[Orientation "w\\""]');
    });
});
