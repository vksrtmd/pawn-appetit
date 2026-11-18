import { beforeEach, describe, expect, test, vi } from "vitest";
import { downloadChessCom, getChessComAccount, getChesscomGame } from "../api";

const mocks = vi.hoisted(() => ({
    fetch: vi.fn<(url: string, init?: unknown) => Promise<Response>>(),
    writeTextFile: vi.fn<(path: string, data: string, opts?: unknown) => Promise<void>>(),
    show: vi.fn<(n: unknown) => void>(),
}));

vi.mock("@tauri-apps/plugin-http", () => ({ fetch: mocks.fetch }));
vi.mock("@tauri-apps/plugin-log", () => ({
    error: vi.fn<() => void>(),
    info: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/plugin-fs", () => ({ writeTextFile: mocks.writeTextFile }));
vi.mock("@tauri-apps/api/path", () => ({
    appDataDir: vi.fn<() => Promise<string>>(async () => "/data"),
    resolve: vi.fn<(...parts: string[]) => Promise<string>>(async (...parts) => parts.join("/")),
}));
vi.mock("@mantine/notifications", () => ({ notifications: { show: mocks.show } }));
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
    mocks.writeTextFile.mockReset().mockResolvedValue(undefined);
    mocks.show.mockReset();
});

const encoding =
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!?{~}(^)[_]@#$,./&-*++=";
function tcn(from: string, to: string) {
    const index = (sq: string) => sq.charCodeAt(0) - 97 + (Number(sq[1]) - 1) * 8;
    return encoding[index(from)] + encoding[index(to)];
}

const archiveUrl = (y: number, m: string) =>
    `https://api.chess.com/pub/player/alice/games/archives`.replace(
        "/games/archives",
        `/games/${y}/${m}`,
    );
function archiveGame(pgn: string) {
    return {
        url: "u",
        pgn,
        time_control: "600",
        end_time: 1,
        rated: true,
        initial_setup: "",
        fen: "",
        rules: "chess",
        white: { rating: 1, result: "win", username: "alice" },
        black: { rating: 1, result: "lose", username: "bob" },
    };
}

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

describe("downloadChessCom", () => {
    function serve(routes: Record<string, () => Response | Promise<Response>>) {
        mocks.fetch.mockImplementation(async (url) => {
            const route = routes[url];
            if (!route) throw new Error(`unexpected ${url}`);
            return route();
        });
    }
    const list = "https://api.chess.com/pub/player/alice/games/archives";

    test("truncates first, appends archives in order and resolves after the last write", async () => {
        const feb = archiveUrl(2026, "02");
        const mar = archiveUrl(2026, "03");
        serve({
            [list]: () => jsonResponse({ archives: [feb, mar] }),
            [feb]: () => jsonResponse({ games: [archiveGame("FEB")] }),
            [mar]: () => jsonResponse({ games: [archiveGame("MAR")] }),
        });
        const log: string[] = [];
        mocks.writeTextFile.mockImplementation(async (_path, data) => {
            log.push(`start:${data}`);
            await new Promise((r) => setTimeout(r, data === "" ? 20 : 5));
            log.push(`end:${data}`);
        });

        await downloadChessCom("alice", null);
        log.push("converted");

        expect(log).toEqual([
            "start:",
            "end:",
            "start:FEB",
            "end:FEB",
            "start:MAR",
            "end:MAR",
            "converted",
        ]);
    });

    test("fails the sync with a message when an archive fails other than 404", async () => {
        const feb = archiveUrl(2026, "02");
        serve({
            [list]: () => jsonResponse({ archives: [feb] }),
            [feb]: () => jsonResponse({}, 500),
        });

        await expect(downloadChessCom("alice", null)).rejects.toThrow(/2026\/02/);
    });

    test("skips an archive that is 404", async () => {
        const feb = archiveUrl(2026, "02");
        const mar = archiveUrl(2026, "03");
        serve({
            [list]: () => jsonResponse({ archives: [feb, mar] }),
            [feb]: () => jsonResponse({}, 404),
            [mar]: () => jsonResponse({ games: [archiveGame("MAR")] }),
        });

        await downloadChessCom("alice", null);

        expect(mocks.writeTextFile).toHaveBeenLastCalledWith(expect.any(String), "MAR", {
            append: true,
        });
    });

    test("still fetches the March archive for a game at 2026-03-31 22:00 UTC when the clock is in Tokyo", async () => {
        vi.stubEnv("TZ", "Asia/Tokyo");
        try {
            const feb = archiveUrl(2026, "02");
            const mar = archiveUrl(2026, "03");
            serve({
                [list]: () => jsonResponse({ archives: [feb, mar] }),
                [mar]: () => jsonResponse({ games: [archiveGame("MAR")] }),
            });

            await downloadChessCom("alice", Date.UTC(2026, 2, 31, 22, 0, 0));

            expect(mocks.fetch.mock.calls.map((c) => c[0])).toEqual([list, mar]);
        } finally {
            vi.unstubAllEnvs();
        }
    });

    test("reports an unknown username as player not found", async () => {
        serve({ [list]: () => jsonResponse({ code: 0, message: "not found" }, 404) });

        await expect(downloadChessCom("alice", null)).rejects.toThrow(/player.*not found/i);
    });
});

describe("getChesscomGame start position", () => {
    test("rebuilds a Chess960 game from its own start FEN", async () => {
        mocks.fetch.mockResolvedValue(
            jsonResponse({
                game: {
                    moveList: tcn("e1", "f3") + tcn("e8", "f6"),
                    pgnHeaders: {
                        Variant: "Chess960",
                        SetUp: "1",
                        FEN: "bbqnnrkr/pppppppp/8/8/8/8/PPPPPPPP/BBQNNRKR w KQkq - 0 1",
                    },
                },
            }),
        );

        const pgn = await getChesscomGame("https://www.chess.com/game/live/123");

        expect(pgn).toContain("1. Nf3 Nf6");
    });

    test("rebuilds a from-position game from its start FEN", async () => {
        mocks.fetch.mockResolvedValue(
            jsonResponse({
                game: {
                    moveList: tcn("e2", "e4"),
                    pgnHeaders: { SetUp: "1", FEN: "4k3/8/8/8/8/8/4P3/4K3 w - - 0 1" },
                },
            }),
        );

        const pgn = await getChesscomGame("https://www.chess.com/game/live/123");

        expect(pgn).toContain("1. e4");
        expect(pgn).toContain('[FEN "4k3/8/8/8/8/8/4P3/4K3 w - - 0 1"]');
    });
});
