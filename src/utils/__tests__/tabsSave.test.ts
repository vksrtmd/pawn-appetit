import { beforeEach, expect, test, vi } from "vitest";

const { writeGame, save } = vi.hoisted(() => ({
    writeGame: vi.fn<(...a: unknown[]) => unknown>(),
    save: vi.fn<() => unknown>(),
}));

vi.mock("@/bindings", () => ({ commands: { writeGame } }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save }));
vi.mock("@tauri-apps/plugin-log", () => ({ error: vi.fn<() => void>() }));
vi.mock("@mantine/notifications", () => ({ notifications: { show: vi.fn<() => void>() } }));
vi.mock("@/features/files/utils/file", async () => ({
    fileMetadataSchema: (await import("zod")).z.any(),
    invalidateFileIndex: vi.fn<() => void>(),
}));
vi.mock("@/utils/files", () => ({
    createFile: vi.fn<() => void>(),
    isTempImportFile: () => false,
    getFileNameWithoutExtension: async (p: string) =>
        (p.split("/").pop() ?? "").replace(/\.pgn$/, ""),
}));

import { createTreeStore } from "@/state/store/tree";
import { defaultTree } from "@/utils/treeReducer";
import { saveTab, saveToFile, type Tab } from "../tabs";

const fileTab = {
    name: "t",
    value: "v",
    type: "analysis",
    source: {
        type: "file",
        path: "/d/a.pgn",
        name: "a",
        numGames: 1,
        metadata: { type: "game", tags: [] },
    },
} as unknown as Tab;

const noop = () => {};
let store: ReturnType<typeof createTreeStore>;
beforeEach(() => {
    vi.clearAllMocks();
    store = createTreeStore();
    store.setState({ ...defaultTree(), dirty: true });
});

test("saveTab rejects and leaves the store dirty when writeGame fails", async () => {
    writeGame.mockResolvedValue({ status: "error", error: "disk full" });
    await expect(saveTab(fileTab, store)).rejects.toThrow("disk full");
    expect(store.getState().dirty).toBe(true);
});

test("saveToFile rejects and leaves the store dirty when writeGame fails", async () => {
    writeGame.mockResolvedValue({ status: "error", error: "disk full" });
    await expect(
        saveToFile({ dir: "/d", tab: fileTab, setCurrentTab: noop, store }),
    ).rejects.toThrow("disk full");
    expect(store.getState().dirty).toBe(true);
});

test("saveToFile marks the store saved when writeGame succeeds", async () => {
    writeGame.mockResolvedValue({ status: "ok", data: null });
    await saveToFile({ dir: "/d", tab: fileTab, setCurrentTab: noop, store });
    expect(store.getState().dirty).toBe(false);
});

test("save-as with a name lacking .pgn writes game.pgn", async () => {
    save.mockResolvedValue("/d/game");
    writeGame.mockResolvedValue({ status: "ok", data: null });
    await saveToFile({ dir: "/d", tab: undefined, setCurrentTab: noop, store });
    expect(writeGame.mock.calls[0][0]).toBe("/d/game.pgn");
});

test("save-as keeps an existing .pgn extension", async () => {
    save.mockResolvedValue("/d/game.pgn");
    writeGame.mockResolvedValue({ status: "ok", data: null });
    await saveToFile({ dir: "/d", tab: undefined, setCurrentTab: noop, store });
    expect(writeGame.mock.calls[0][0]).toBe("/d/game.pgn");
});
