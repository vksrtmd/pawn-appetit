import { expect, test, vi } from "vitest";
import type { Token } from "@/bindings";
import { ANNOTATION_INFO, type Annotation, NAG_INFO } from "@/utils/annotation";
import { getPgnHeaders, hasMorePriority, headersToPGN, parsePGN } from "@/utils/chess";

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
