import { beforeEach, describe, expect, test, vi } from "vitest";

import {
    buildFileIndex,
    getCachedFileIndex,
    invalidateFileIndex,
    processEntriesRecursively,
} from "../file";

const mocks = vi.hoisted(() => ({
    readDir: vi.fn<() => Promise<Array<{ name: string; isFile: boolean; isDirectory: boolean }>>>(),
    exists: vi.fn<() => Promise<boolean>>(),
    readTextFile: vi.fn<() => Promise<string>>(),
    writeTextFile: vi.fn<() => Promise<void>>(),
    basename: vi.fn<(path: string) => Promise<string>>(),
    join: vi.fn<(...parts: string[]) => Promise<string>>(),
    getFileMetadata: vi.fn<() => Promise<{ status: "ok"; data: { last_modified: bigint } }>>(),
    countPgnGames: vi.fn<(path?: string) => Promise<{ status: "ok"; data: number }>>(),
}));

vi.mock("@tauri-apps/api/path", () => ({
    BaseDirectory: { AppLocalData: "AppLocalData" },
    basename: mocks.basename,
    join: mocks.join,
}));

vi.mock("@tauri-apps/plugin-fs", () => ({
    exists: mocks.exists,
    readDir: mocks.readDir,
    readTextFile: mocks.readTextFile,
    writeTextFile: mocks.writeTextFile,
}));

vi.mock("@/bindings", () => ({
    commands: {
        getFileMetadata: mocks.getFileMetadata,
        countPgnGames: mocks.countPgnGames,
    },
}));

describe("file index cache", () => {
    beforeEach(() => {
        vi.clearAllMocks();
        invalidateFileIndex();

        mocks.readDir.mockResolvedValue([
            { name: "first.pgn", isFile: true, isDirectory: false },
            { name: "notes.txt", isFile: true, isDirectory: false },
        ]);
        mocks.exists.mockResolvedValue(true);
        mocks.readTextFile.mockResolvedValue(JSON.stringify({ type: "game", tags: ["rapid"] }));
        mocks.basename.mockImplementation(async (path) => path.split("/").pop() ?? path);
        mocks.join.mockImplementation(async (...parts) => parts.join("/"));
        mocks.getFileMetadata.mockResolvedValue({
            status: "ok",
            data: { last_modified: 1_700_000_000n },
        });
        mocks.countPgnGames.mockResolvedValue({ status: "ok", data: 42 });
    });

    test("reuses indexed file metadata until invalidated", async () => {
        const first = await getCachedFileIndex("/docs");
        const second = await getCachedFileIndex("/docs");

        expect(first).toEqual(second);
        expect(first).toHaveLength(1);
        expect(first[0]).toEqual(
            expect.objectContaining({
                name: "first",
                path: "/docs/first.pgn",
                numGames: 42,
                metadata: { type: "game", tags: ["rapid"] },
            }),
        );
        expect(mocks.readDir).toHaveBeenCalledTimes(1);
        expect(mocks.countPgnGames).toHaveBeenCalledTimes(1);

        invalidateFileIndex("/docs");
        await getCachedFileIndex("/docs");

        expect(mocks.readDir).toHaveBeenCalledTimes(2);
        expect(mocks.countPgnGames).toHaveBeenCalledTimes(2);
    });

    test("keeps directory listing order when entries resolve out of order", async () => {
        // Earlier entries take longer to read, so a naive parallel push would reorder them.
        mocks.countPgnGames.mockImplementation(async (path?: string) => {
            const delay = String(path).includes("slow") ? 20 : 0;
            await new Promise((r) => setTimeout(r, delay));
            return { status: "ok", data: 1 };
        });
        mocks.readDir.mockResolvedValue([{ name: "inner.pgn", isFile: true, isDirectory: false }]);

        const result = await processEntriesRecursively("/docs", [
            { name: "slow.pgn", isFile: true, isDirectory: false, isSymlink: false },
            { name: "skip.txt", isFile: true, isDirectory: false, isSymlink: false },
            { name: "sub", isFile: false, isDirectory: true, isSymlink: false },
            { name: "fast.pgn", isFile: true, isDirectory: false, isSymlink: false },
        ]);

        expect(result.map((e) => (e.type === "directory" ? `dir:${e.name}` : e.name))).toEqual([
            "slow",
            "dir:sub",
            "fast",
        ]);
    });

    test("reads last_modified seconds as-is", async () => {
        const [file] = await buildFileIndex("/docs");
        expect(file.lastModified).toBe(1_700_000_000);
    });
});
