import { getDefaultStore } from "jotai";
import { beforeEach, expect, test, vi } from "vitest";

const {
    killEngines,
    writeGame,
    updateGame,
    countPgnGames,
    readGames,
    save: saveDialog,
    createFile,
} = vi.hoisted(() => ({
    killEngines: vi.fn<(...a: unknown[]) => unknown>(),
    writeGame: vi.fn<(...a: unknown[]) => unknown>(),
    updateGame: vi.fn<(...a: unknown[]) => unknown>(),
    countPgnGames: vi.fn<(...a: unknown[]) => unknown>(),
    readGames: vi.fn<(...a: unknown[]) => unknown>(),
    save: vi.fn<() => unknown>(),
    createFile: vi.fn<(...a: unknown[]) => unknown>(),
}));

vi.mock("@/bindings", () => ({
    commands: { writeGame, updateGame, countPgnGames, readGames, killEngines },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: saveDialog }));
vi.mock("@tauri-apps/plugin-log", () => ({ error: vi.fn<() => void>() }));
vi.mock("@mantine/notifications", () => ({ notifications: { show: vi.fn<() => void>() } }));
vi.mock("@/features/files/utils/file", async () => ({
    fileMetadataSchema: (await import("zod")).z.any(),
    invalidateFileIndex: vi.fn<() => void>(),
}));
vi.mock("@/utils/files", () => ({
    createFile,
    isTempImportFile: (p: string) => p.startsWith("/tmp/import"),
    nowEpochSeconds: () => 123,
    getFileNameWithoutExtension: async (p: string) =>
        (p.split("/").pop() ?? "").replace(/\.pgn$/, ""),
}));

import { tabsAtom } from "@/state/uiAtoms";
import { createTreeStore } from "@/state/store/tree";
import { defaultTree } from "@/utils/treeReducer";
import type { Tab } from "@/utils/tabs";
import { missingMovesAtom } from "../gameAtoms";
import { engineMovesFamily } from "../engineAtoms";
import { activeTabAtom } from "../uiAtoms";
import {
    closeActiveTab,
    closeAllTabs,
    closeOtherTabs,
    closeTabsToRight,
    dispose,
    duplicate,
    getSideKey,
    open,
    readTree,
    registerTabStore,
    save,
    setSideKey,
} from "../tabSession";

const ok = { status: "ok", data: null };
const fileSource = (path: string) => ({
    type: "file",
    path,
    name: "n",
    numGames: 1,
    metadata: { type: "game", tags: [] },
});
const mkTab = (value: string, source?: unknown): Tab =>
    ({ name: value, value, type: "analysis", source }) as unknown as Tab;

const jotai = getDefaultStore();
const tabOf = (id: string) => jotai.get(tabsAtom).find((t) => t.value === id)!;

function addTab(tab: Tab, moves: string[] = []) {
    const store = createTreeStore();
    store.setState({ ...defaultTree(), dirty: true });
    for (const san of moves) store.getState().makeMove({ payload: san, changePosition: true });
    store.setState({ dirty: true });
    jotai.set(tabsAtom, [...jotai.get(tabsAtom), tab]);
    registerTabStore(tab.value, store);
    return store;
}

beforeEach(() => {
    vi.clearAllMocks();
    jotai.set(tabsAtom, []);
    sessionStorage.clear();
    killEngines.mockResolvedValue(ok);
    writeGame.mockResolvedValue(ok);
    updateGame.mockResolvedValue(ok);
});

test("saving a background tab by id writes its own moves to its own file and leaves the active tab alone", async () => {
    const active = mkTab("active", fileSource("/d/active.pgn"));
    const bg = mkTab("bg", fileSource("/d/bg.pgn"));
    const activeStore = addTab(active, ["e4"]);
    const bgStore = addTab(bg, ["d4"]);
    jotai.set(tabsAtom, [active, bg]);
    const activeBefore = tabOf("active");

    await save("bg", { dir: "/d" });

    expect(writeGame).toHaveBeenCalledTimes(1);
    expect(writeGame.mock.calls[0][0]).toBe("/d/bg.pgn");
    expect(String(writeGame.mock.calls[0][2])).toContain("d4");
    expect(String(writeGame.mock.calls[0][2])).not.toContain("e4");
    expect(bgStore.getState().dirty).toBe(false);
    expect(activeStore.getState().dirty).toBe(true);
    expect(tabOf("active")).toBe(activeBefore);
});

test("a tab opened from a database saves back to the database", async () => {
    const store = addTab(mkTab("db", { type: "db", db: "/x.db3", id: 7 }), ["c4"]);

    await save("db", { dir: "/d" });

    expect(saveDialog).not.toHaveBeenCalled();
    expect(writeGame).not.toHaveBeenCalled();
    expect(updateGame.mock.calls[0].slice(0, 2)).toEqual(["/x.db3", 7]);
    expect(String((updateGame.mock.calls[0][2] as { moves: string }).moves)).toContain("c4");
    expect(store.getState().dirty).toBe(false);
});

test("a failed write leaves the tab dirty and save rejects", async () => {
    const store = addTab(mkTab("t", fileSource("/d/a.pgn")));
    writeGame.mockResolvedValue({ status: "error", error: "disk full" });

    await expect(save("t", { dir: "/d" })).rejects.toThrow("disk full");
    expect(store.getState().dirty).toBe(true);
});

test("a failed database update leaves the tab dirty and save rejects", async () => {
    const store = addTab(mkTab("t", { type: "db", db: "/x.db3", id: 1 }));
    updateGame.mockResolvedValue({ status: "error", error: "locked" });

    await expect(save("t", { dir: "/d" })).rejects.toThrow("locked");
    expect(store.getState().dirty).toBe(true);
});

test("a tab with no file goes through save-as and records the new file as its source", async () => {
    const store = addTab(mkTab("t"));
    saveDialog.mockResolvedValue("/d/game");

    expect(await save("t", { dir: "/d" })).toBe("saved");

    expect(writeGame.mock.calls[0][0]).toBe("/d/game.pgn");
    expect(tabOf("t").source).toMatchObject({ path: "/d/game.pgn", name: "game" });
    expect(store.getState().dirty).toBe(false);
});

test("a temp import file goes through save-as and its games are copied to the new file", async () => {
    addTab(mkTab("t", fileSource("/tmp/import-1.pgn")));
    saveDialog.mockResolvedValue("/d/kept.pgn");
    countPgnGames.mockResolvedValue({ status: "ok", data: 1 });
    readGames.mockResolvedValue({ status: "ok", data: ["1. e4 *"] });

    await save("t", { dir: "/d" });

    expect(createFile.mock.calls[0][0]).toMatchObject({ filename: "kept", pgn: "1. e4 *" });
    expect(writeGame.mock.calls[0][0]).toBe("/d/kept.pgn");
});

test("a cancelled save-as dialog resolves as cancelled, writes nothing and leaves the tab dirty", async () => {
    const store = addTab(mkTab("t"));
    saveDialog.mockResolvedValue(null);

    expect(await save("t", { dir: "/d" })).toBe("cancelled");
    expect(writeGame).not.toHaveBeenCalled();
    expect(store.getState().dirty).toBe(true);
});

test("a write error after save-as rejects and keeps the tab's old source", async () => {
    addTab(mkTab("t"));
    saveDialog.mockResolvedValue("/d/game.pgn");
    writeGame.mockResolvedValue({ status: "error", error: "disk full" });

    await expect(save("t", { dir: "/d" })).rejects.toThrow("disk full");
    expect(tabOf("t").source).toBeUndefined();
});

// ---------------------------------------------------------------------------
// dispose and the close paths
// ---------------------------------------------------------------------------

/** Gives tab `id` everything the module stores for a tab. */
function fillTab(id: string) {
    open(id, defaultTree());
    setSideKey(id, "localGameId", "g1");
    setSideKey(id, "chessComGameUrl", "https://chess.com/g");
    setSideKey(id, "lichessGameId", "li1");
    jotai.set(missingMovesAtom, (prev) => ({ ...prev, [id]: [] }));
    jotai.set(engineMovesFamily({ tab: id, engine: "sf" }), new Map());
}

function expectDisposed(id: string) {
    expect(Object.keys(sessionStorage).filter((k) => k === id || k.startsWith(`${id}_`))).toEqual(
        [],
    );
    expect(Object.keys(jotai.get(missingMovesAtom))).not.toContain(id);
    expect([...engineMovesFamily.getParams()].filter((p) => p.tab === id)).toEqual([]);
    expect(killEngines).toHaveBeenCalledWith(id);
}

async function settle() {
    await new Promise((r) => setTimeout(r, 0));
}

test("dispose removes the tree, side keys, missing-moves entry and per-tab atoms and kills engines", async () => {
    fillTab("a");
    fillTab("b");

    await dispose("a");

    expectDisposed("a");
    expect(readTree("b")).not.toBeNull();
    expect(getSideKey("b", "lichessGameId")).toBe("li1");
});

test("Ctrl/Cmd+W closes the active tab and disposes it", async () => {
    fillTab("a");
    fillTab("b");
    jotai.set(tabsAtom, [mkTab("a"), mkTab("b")]);
    jotai.set(activeTabAtom, "a");

    closeActiveTab();
    await settle();

    expect(jotai.get(tabsAtom).map((t) => t.value)).toEqual(["b"]);
    expect(jotai.get(activeTabAtom)).toBe("b");
    expectDisposed("a");
    expect(killEngines).not.toHaveBeenCalledWith("b");
});

test("close others disposes every other tab", async () => {
    for (const id of ["a", "b", "c"]) fillTab(id);
    jotai.set(tabsAtom, [mkTab("a"), mkTab("b"), mkTab("c")]);

    closeOtherTabs("b");
    await settle();

    expect(jotai.get(tabsAtom).map((t) => t.value)).toEqual(["b"]);
    expectDisposed("a");
    expectDisposed("c");
    expect(readTree("b")).not.toBeNull();
});

test("close to the right disposes the tabs to the right", async () => {
    for (const id of ["a", "b", "c"]) fillTab(id);
    jotai.set(tabsAtom, [mkTab("a"), mkTab("b"), mkTab("c")]);

    closeTabsToRight("a");
    await settle();

    expect(jotai.get(tabsAtom).map((t) => t.value)).toEqual(["a"]);
    expectDisposed("b");
    expectDisposed("c");
    expect(readTree("a")).not.toBeNull();
});

test("close all disposes every tab", async () => {
    for (const id of ["a", "b"]) fillTab(id);
    jotai.set(tabsAtom, [mkTab("a"), mkTab("b")]);

    closeAllTabs();
    await settle();

    expect(jotai.get(tabsAtom)).toEqual([]);
    expectDisposed("a");
    expectDisposed("b");
});

test("a duplicated tab keeps the source and side keys and saves to the same file", async () => {
    const source = fileSource("/d/orig.pgn");
    jotai.set(tabsAtom, [mkTab("orig", source)]);
    open("orig", defaultTree());
    setSideKey("orig", "localGameId", "g1");

    const id = duplicate("orig")!;

    expect(tabOf(id).source).toEqual(source);
    expect(getSideKey(id, "localGameId")).toBe("g1");
    expect(jotai.get(activeTabAtom)).toBe(id);
    getTabStoreDirty(id);
    await save(id, { dir: "/d" });
    expect(writeGame.mock.calls[0][0]).toBe("/d/orig.pgn");
});

function getTabStoreDirty(id: string) {
    const store = createTreeStore(id);
    registerTabStore(id, store);
    store.setState({ dirty: true });
}

test("a stored tree reads back as the opened tree", () => {
    const tree = defaultTree();
    tree.headers.event = "Opened";
    open("t", tree);
    expect(readTree("t")?.headers.event).toBe("Opened");
});
