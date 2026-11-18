import { beforeEach, describe, expect, test, vi } from "vitest";
import type { FileMetadata } from "../file";
import { deleteFile, removeEntry, renameFile } from "../fileOps";

const fs = vi.hoisted(() => ({ files: new Map<string, string>() }));

vi.mock("@tauri-apps/plugin-fs", () => ({
    exists: async (p: string) => fs.files.has(p),
    readTextFile: async (p: string) => fs.files.get(p) ?? "",
    writeTextFile: async (p: string, c: string) => void fs.files.set(p, c),
    rename: async (a: string, b: string) => {
        fs.files.set(b, fs.files.get(a) ?? "");
        fs.files.delete(a);
    },
    remove: async (p: string) => void fs.files.delete(p),
}));
vi.mock("@/features/files/utils/file", async (orig) => ({
    ...(await orig<object>()),
    invalidateFileIndex: vi.fn<() => void>(),
}));
vi.mock("@/bindings", () => ({ commands: {} }));

const meta = (name: string): FileMetadata => ({
    type: "file",
    name,
    path: `/d/${name}.pgn`,
    numGames: 1,
    metadata: { type: "repertoire", tags: ["x"] },
    lastModified: 0,
});

beforeEach(() => {
    fs.files.clear();
    fs.files.set("/d/a.pgn", "A");
    fs.files.set("/d/a.info", '{"type":"repertoire","tags":["x"]}');
    fs.files.set("/d/b.pgn", "B");
});

describe("renameFile", () => {
    test("refuses to overwrite an existing file and leaves it unchanged", async () => {
        await expect(renameFile(meta("a"), "b", "game")).rejects.toThrow(/./);
        expect(fs.files.get("/d/b.pgn")).toBe("B");
        expect(fs.files.get("/d/a.pgn")).toBe("A");
    });

    test("keeps the file's tags and moves the sidecar", async () => {
        const renamed = await renameFile(meta("a"), "c", "puzzle");
        expect(renamed.path).toBe("/d/c.pgn");
        expect(renamed.metadata).toEqual({ type: "puzzle", tags: ["x"] });
        expect(JSON.parse(fs.files.get("/d/c.info") ?? "")).toEqual({
            type: "puzzle",
            tags: ["x"],
        });
        expect(fs.files.has("/d/a.info")).toBe(false);
        expect(fs.files.has("/d/a.pgn")).toBe(false);
    });
});

describe("deleteFile", () => {
    test("removes the file and its .info sidecar", async () => {
        await deleteFile("/d/a.pgn", false);
        expect([...fs.files.keys()].sort()).toEqual(["/d/b.pgn"]);
    });
});

describe("removeEntry", () => {
    test("drops the entry from nested listings by exact path", () => {
        const list = [
            meta("a"),
            meta("b"),
            { type: "directory" as const, name: "s", path: "/d/s", children: [meta("a")] },
        ];
        const next = removeEntry(list, "/d/a.pgn");
        expect(next.map((e) => e.path)).toEqual(["/d/b.pgn", "/d/s"]);
    });
});
