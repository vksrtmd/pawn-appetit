import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import {
    createFile,
    createTempImportFile,
    nowEpochSeconds,
    openFile,
    openFileAndRemember,
} from "../files";
import { PgnParseError } from "../pgnParseError";
import { getRecentFiles } from "../recentFiles";
import { deserializeStorageValue } from "../tabStateStorage";

const mocks = vi.hoisted(() => ({
    countPgnGames: vi.fn<() => Promise<{ status: "ok"; data: number }>>(),
    readGames:
        vi.fn<
            (file: string, start: number, end: number) => Promise<{ status: "ok"; data: string[] }>
        >(),
    createTab: vi.fn<() => Promise<string>>(),
    parsePGN: vi.fn<() => Promise<{ headers: { event: string } }>>(),
    exists: vi.fn<() => Promise<boolean>>(),
    notify: vi.fn<(options: unknown) => void>(),
}));

vi.mock("@mantine/notifications", () => ({ notifications: { show: mocks.notify } }));
vi.mock("i18next", () => ({ default: { t: (key: string) => key } }));

vi.mock("@/bindings", () => ({
    commands: {
        countPgnGames: mocks.countPgnGames,
        readGames: mocks.readGames,
    },
}));

vi.mock("@/features/files/utils/file", () => ({
    invalidateFileIndex: vi.fn<() => void>(),
}));

vi.mock("@/utils/chess", () => ({
    parsePGN: mocks.parsePGN,
}));

vi.mock("@/utils/tabs", () => ({
    createTab: mocks.createTab,
}));

vi.mock("@tauri-apps/api/path", () => ({
    BaseDirectory: { Temp: "Temp" },
    basename: vi.fn<(path: string) => Promise<string>>(
        async (path) => path.split("/").pop() ?? path,
    ),
    extname: vi.fn<(path: string) => Promise<string>>(async (path) => path.split(".").pop() ?? ""),
    resolve: vi.fn<(...parts: string[]) => Promise<string>>(async (...parts) => parts.join("/")),
    tempDir: vi.fn<() => Promise<string>>(async () => "/tmp"),
}));

vi.mock("@tauri-apps/plugin-fs", () => ({
    exists: mocks.exists,
    mkdir: vi.fn<() => Promise<void>>(async () => undefined),
    readTextFile: vi.fn<() => Promise<string>>(async () => "{}"),
    writeTextFile: vi.fn<() => Promise<void>>(async () => undefined),
}));

vi.mock("@tauri-apps/plugin-os", () => ({
    platform: vi.fn<() => Promise<string>>(async () => "macos"),
}));

describe("openFile", () => {
    beforeEach(() => {
        vi.clearAllMocks();
        sessionStorage.clear();
        localStorage.clear();
        mocks.countPgnGames.mockResolvedValue({ status: "ok", data: 3 });
        mocks.readGames.mockResolvedValue({
            status: "ok",
            data: ['[Event "First"]\n\n1. e4 e5 *\n\n'],
        });
        mocks.createTab.mockResolvedValue("tab-1");
        mocks.parsePGN.mockResolvedValue({ headers: { event: "First" } });
        mocks.exists.mockResolvedValue(false);
    });

    test("loads only the first PGN game into a new file-backed tab", async () => {
        const setTabs = vi.fn<() => void>();
        const setActiveTab = vi.fn<() => void>();

        await openFile("/docs/large.pgn", setTabs, setActiveTab);

        expect(mocks.countPgnGames).toHaveBeenCalledWith("/docs/large.pgn");
        expect(mocks.readGames).toHaveBeenCalledWith("/docs/large.pgn", 0, 0);
        expect(mocks.readGames).toHaveBeenCalledTimes(1);
        expect(mocks.createTab).toHaveBeenCalledWith(
            expect.not.objectContaining({
                pgn: expect.any(String),
            }),
        );
        expect(mocks.createTab).toHaveBeenCalledWith(
            expect.objectContaining({
                gameNumber: 0,
                srcInfo: expect.objectContaining({
                    path: "/docs/large.pgn",
                    numGames: 3,
                }),
            }),
        );
        expect(mocks.parsePGN).toHaveBeenCalledWith('[Event "First"]\n\n1. e4 e5 *\n\n');
        expect(deserializeStorageValue(sessionStorage.getItem("tab-1") ?? "")).toEqual({
            version: 0,
            state: { headers: { event: "First" } },
        });
    });

    test("records a recent PGN after a successful open", async () => {
        await openFileAndRemember("/docs/recent.pgn", vi.fn<() => void>(), vi.fn<() => void>());

        expect(getRecentFiles()).toEqual([
            expect.objectContaining({
                path: "/docs/recent.pgn",
                name: "recent",
            }),
        ]);
    });

    test("opens a PGN when sidecar metadata is outside the fs scope", async () => {
        mocks.exists.mockRejectedValue(new Error("path not allowed on the configured scope"));

        await openFile(
            "/home/daniel/Downloads/Naroditsky-Bortnyk_Jobava-London.pgn",
            vi.fn<() => void>(),
            vi.fn<() => void>(),
        );

        expect(mocks.createTab).toHaveBeenCalled();
        expect(mocks.exists).toHaveBeenCalledWith(
            "/home/daniel/Downloads/Naroditsky-Bortnyk_Jobava-London.info",
        );
    });
});

describe("file lastModified is epoch seconds", () => {
    const NOW_MS = Date.UTC(2026, 10, 17, 12, 34, 56, 789);
    const NOW_S = Math.floor(NOW_MS / 1000);

    beforeEach(() => {
        vi.clearAllMocks();
        vi.useFakeTimers();
        vi.setSystemTime(NOW_MS);
        mocks.countPgnGames.mockResolvedValue({ status: "ok", data: 1 });
        mocks.readGames.mockResolvedValue({ status: "ok", data: ['[Event "A"]\n\n*\n\n'] });
        mocks.createTab.mockResolvedValue("tab-1");
        mocks.parsePGN.mockResolvedValue({ headers: { event: "A" } });
        mocks.exists.mockResolvedValue(false);
    });

    afterEach(() => {
        vi.useRealTimers();
    });

    test("nowEpochSeconds returns whole epoch seconds", () => {
        expect(nowEpochSeconds()).toBe(NOW_S);
    });

    test("openFile stamps the tab source with epoch seconds", async () => {
        await openFile("/docs/a.pgn", vi.fn<() => void>(), vi.fn<() => void>());

        expect(mocks.createTab).toHaveBeenCalledWith(
            expect.objectContaining({ srcInfo: expect.objectContaining({ lastModified: NOW_S }) }),
        );
    });

    test("createFile returns metadata stamped with epoch seconds", async () => {
        const result = await createFile({ filename: "new", filetype: "game", dir: "/docs" });

        expect(result.unwrap().lastModified).toBe(NOW_S);
    });

    test("createTempImportFile stamps epoch seconds", async () => {
        const file = await createTempImportFile("*");

        expect(file.lastModified).toBe(NOW_S);
    });
});

describe("openFile with an illegal move", () => {
    test("shows the error and opens no tab", async () => {
        vi.clearAllMocks();
        mocks.countPgnGames.mockResolvedValue({ status: "ok", data: 1 });
        mocks.readGames.mockResolvedValue({ status: "ok", data: ["1. e4 e5 2. Qh8 *"] });
        mocks.exists.mockResolvedValue(false);
        mocks.parsePGN.mockRejectedValue(new PgnParseError("Qh8", 3));
        const setTabs = vi.fn<() => void>();

        const opened = await openFileAndRemember("/docs/bad.pgn", setTabs, vi.fn<() => void>());

        expect(opened).toBeUndefined();
        expect(mocks.createTab).not.toHaveBeenCalled();
        expect(getRecentFiles().map((f) => f.path)).not.toContain("/docs/bad.pgn");
        expect(mocks.notify).toHaveBeenCalledWith(expect.objectContaining({ color: "red" }));
    });
});
