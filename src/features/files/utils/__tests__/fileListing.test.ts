import { beforeEach, expect, test, vi } from "vitest";
import { infoPathFor, processEntriesRecursively } from "../file";

const mocks = vi.hoisted(() => ({
    infos: new Map<string, string>(),
    writes: [] as string[],
}));

vi.mock("@tauri-apps/api/path", () => ({
    BaseDirectory: { AppLocalData: "AppLocalData" },
    basename: async (p: string) => p.split("/").pop() ?? p,
    join: async (...parts: string[]) => parts.join("/"),
}));
vi.mock("@tauri-apps/plugin-fs", () => ({
    exists: async (p: string) => mocks.infos.has(p),
    readDir: vi.fn<() => Promise<never[]>>(),
    readTextFile: async (p: string) => mocks.infos.get(p) ?? "",
    writeTextFile: async (p: string) => void mocks.writes.push(p),
}));
vi.mock("@/bindings", () => ({
    commands: {
        getFileMetadata: async () => ({ status: "ok", data: { last_modified: 1n } }),
        countPgnGames: async () => ({ status: "ok", data: 2 }),
    },
}));

beforeEach(() => {
    mocks.infos.clear();
    mocks.writes.length = 0;
});

const entry = (name: string) => ({ name, isFile: true, isDirectory: false, isSymlink: false });

test("one broken .info file does not empty the list, and listing writes no files", async () => {
    mocks.infos.set("/r/a.info", "{broken");
    mocks.infos.set("/r/b.info", '{"type":"puzzle","tags":["t"]}');
    const list = await processEntriesRecursively("/r", [
        entry("a.pgn"),
        entry("b.pgn"),
        entry("c.pgn"),
    ]);
    expect(list.map((f) => f.type === "file" && f.metadata.type)).toEqual([
        "other",
        "puzzle",
        "other",
    ]);
    expect(mocks.writes).toEqual([]);
});

test("the .info path replaces only the trailing .pgn", () => {
    expect(infoPathFor("/x.pgn.d/my.pgn.pgn")).toBe("/x.pgn.d/my.pgn.info");
});
