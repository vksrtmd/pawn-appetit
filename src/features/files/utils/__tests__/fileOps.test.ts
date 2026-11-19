import { beforeEach, describe, expect, test, vi } from "vitest";
import type { FileMetadata } from "../file";
import {
    deleteFile,
    RenameExistsError,
    RenameSidecarError,
    removeEntry,
    renameFile,
} from "../fileOps";

const fs = vi.hoisted(() => ({
    files: new Map<string, string>(),
    insensitive: false,
    failRename: false,
    failWrite: false,
}));

vi.mock("@tauri-apps/plugin-fs", () => ({
    exists: async (p: string) =>
        fs.files.has(p) ||
        (fs.insensitive && [...fs.files.keys()].some((k) => k.toLowerCase() === p.toLowerCase())),
    readDir: async () => [...fs.files.keys()].map((k) => ({ name: k.split("/").pop() })),
    readTextFile: async (p: string) => fs.files.get(p) ?? "",
    writeTextFile: async (p: string, c: string) => {
        if (fs.failWrite) throw new Error("read-only");
        fs.files.set(p, c);
    },
    rename: async (a: string, b: string) => {
        if (fs.failRename) throw new Error("permission denied");
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
    fs.insensitive = false;
    fs.failRename = false;
    fs.failWrite = false;
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

describe("renameFile edge cases", () => {
    test("a new name with $& or $` builds the path from the name as typed", async () => {
        const renamed = await renameFile(meta("a"), "x$&y$`z", "game");
        expect(renamed.path).toBe("/d/x$&y$`z.pgn");
    });

    test("a case-only rename succeeds where the file system treats both names as one file", async () => {
        // Case-insensitive: "exists" sees A.pgn as a.pgn, but the directory lists the real name
        fs.insensitive = true;
        await expect(renameFile(meta("a"), "A", "game")).resolves.toMatchObject({
            path: "/d/A.pgn",
        });
        expect(fs.files.has("/d/A.pgn")).toBe(true);
    });

    test("a taken name is reported as RenameExistsError, any other failure is not", async () => {
        await expect(renameFile(meta("a"), "b", "game")).rejects.toBeInstanceOf(RenameExistsError);
        fs.failRename = true;
        const err = await renameFile(meta("a"), "c", "game").catch((e: unknown) => e);
        expect(err).toBeInstanceOf(Error);
        expect(err).not.toBeInstanceOf(RenameExistsError);
    });

    test("a sidecar failure after the rename still reports the renamed file", async () => {
        fs.failWrite = true;
        const err = await renameFile(meta("a"), "c", "game").catch((e: unknown) => e);
        expect(err).toBeInstanceOf(RenameSidecarError);
        expect((err as RenameSidecarError).renamed.path).toBe("/d/c.pgn");
        expect(fs.files.has("/d/c.pgn")).toBe(true);
    });
});
