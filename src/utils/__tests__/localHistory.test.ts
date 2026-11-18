import { beforeEach, describe, expect, test, vi } from "vitest";
import { saveAnalyzedGame, getAllAnalyzedGames } from "../analyzedGames";
import { type GameRecord, saveGameRecord, getRecentGames } from "../gameRecords";

const fs = vi.hoisted(() => ({ files: new Map<string, string>() }));

vi.mock("@/utils/chess", () => ({
    getGameStats: vi.fn<() => void>(),
    parsePGN: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/path", () => ({
    appDataDir: async () => "/data",
    resolve: async (...parts: string[]) => parts.join("/"),
}));
vi.mock("@tauri-apps/plugin-fs", () => ({
    exists: async (p: string) => fs.files.has(p),
    readTextFile: async (p: string) => {
        const v = fs.files.get(p);
        if (v === undefined) throw new Error("No such file or directory (os error 2)");
        return v;
    },
    writeTextFile: async (p: string, c: string) => {
        await Promise.resolve();
        fs.files.set(p, c);
    },
    rename: async (from: string, to: string) => {
        const v = fs.files.get(from);
        if (v === undefined) throw new Error("missing");
        fs.files.set(to, v);
        fs.files.delete(from);
    },
}));

const record = (id: string): GameRecord => ({
    id,
    white: { type: "human" },
    black: { type: "engine" },
    result: "*",
    timestamp: 1,
    moves: [],
    fen: "x",
});

beforeEach(() => fs.files.clear());

describe("played_games.json", () => {
    test("a missing file counts as empty", async () => {
        await saveGameRecord(record("a"));
        expect(JSON.parse(fs.files.get("/data/played_games.json") ?? "")).toHaveLength(1);
    });

    test("a corrupt file is moved aside and not overwritten by the save", async () => {
        fs.files.set("/data/played_games.json", "{not json");
        await expect(saveGameRecord(record("a"))).rejects.toThrow();
        expect(fs.files.has("/data/played_games.json")).toBe(false);
        const aside = [...fs.files.entries()].filter(([k]) =>
            k.startsWith("/data/played_games.json."),
        );
        expect(aside.map(([, v]) => v)).toEqual(["{not json"]);
    });

    test("concurrent saves both land", async () => {
        await Promise.all([saveGameRecord(record("a")), saveGameRecord(record("b"))]);
        expect((await getRecentGames()).map((r) => r.id).sort()).toEqual(["a", "b"]);
    });
});

describe("analyzed_games.json", () => {
    test("a corrupt file is moved aside and not overwritten", async () => {
        fs.files.set("/data/analyzed_games.json", "oops");
        await expect(saveAnalyzedGame("g", "pgn")).rejects.toThrow();
        expect(fs.files.has("/data/analyzed_games.json")).toBe(false);
        expect([...fs.files.values()]).toContain("oops");
    });

    test("concurrent saves both land", async () => {
        await Promise.all([saveAnalyzedGame("a", "1"), saveAnalyzedGame("b", "2")]);
        expect(await getAllAnalyzedGames()).toEqual({ a: "1", b: "2" });
    });
});
