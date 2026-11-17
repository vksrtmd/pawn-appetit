import { beforeEach, describe, expect, test, vi } from "vitest";
import { getChessComAccount, getChesscomGame } from "../api";

const mocks = vi.hoisted(() => ({
    fetch: vi.fn<(url: string, init?: unknown) => Promise<Response>>(),
}));

vi.mock("@tauri-apps/plugin-http", () => ({ fetch: mocks.fetch }));
vi.mock("@tauri-apps/plugin-log", () => ({
    error: vi.fn<() => void>(),
    info: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/plugin-fs", () => ({ writeTextFile: vi.fn<() => void>() }));
vi.mock("@tauri-apps/api/path", () => ({
    appDataDir: vi.fn<() => string>(),
    resolve: vi.fn<() => string>(),
}));
vi.mock("@mantine/notifications", () => ({ notifications: { show: vi.fn<() => void>() } }));
vi.mock("@/bindings", () => ({ events: { downloadProgress: { emit: vi.fn<() => void>() } } }));

// Shaped like https://www.chess.com/callback/live/game/<id>: ratings and flags are numbers.
const gameCallback = {
    game: {
        id: 123,
        moveList: "mC0K",
        pgnHeaders: {
            Event: "Live Chess",
            White: "alice",
            Black: "bob",
            WhiteElo: 1500,
            BlackElo: 1480,
            Result: "1-0",
        },
    },
};

function jsonResponse(body: unknown, status = 200) {
    return new Response(JSON.stringify(body), { status });
}

beforeEach(() => {
    mocks.fetch.mockReset();
});

describe("getChesscomGame", () => {
    test("imports a game whose callback has numeric header values", async () => {
        mocks.fetch.mockResolvedValue(jsonResponse(gameCallback));

        const pgn = await getChesscomGame("https://www.chess.com/game/live/123");

        expect(pgn).toContain('[WhiteElo "1500"]');
        expect(pgn).toContain("1. e4 e5");
    });

    test.each([
        ["https://www.chess.com/game/123", "live"],
        ["https://www.chess.com/game/live/123?foo=1", "live"],
        ["https://www.chess.com/game/daily/123", "daily"],
    ])("%s is fetched from the %s callback", async (url, type) => {
        mocks.fetch.mockResolvedValue(jsonResponse(gameCallback));

        await getChesscomGame(url);

        expect(mocks.fetch.mock.calls[0][0]).toBe(
            `https://www.chess.com/callback/${type}/game/123`,
        );
    });
});

describe("getChessComAccount", () => {
    test("accepts stats without a rating deviation", async () => {
        const stats = {
            chess_rapid: {
                last: { rating: 1500, date: 1700000000 },
                record: { win: 10, loss: 5, draw: 1 },
            },
            chess_blitz: {
                last: { rating: 1400, date: 1700000000, rd: 60 },
                record: { win: 1, loss: 2, draw: 3 },
            },
        };
        mocks.fetch.mockResolvedValue(jsonResponse(stats));

        const result = await getChessComAccount("Alice");

        expect(result?.chess_rapid?.last.rating).toBe(1500);
    });
});
