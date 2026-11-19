import { getDefaultStore } from "jotai";
import { beforeEach, expect, test, vi } from "vitest";
import { z } from "zod";

const { killEngines, parsePGN } = vi.hoisted(() => ({
    killEngines: vi.fn<(...a: unknown[]) => Promise<unknown>>(),
    parsePGN: vi.fn<(...a: unknown[]) => Promise<unknown>>(),
}));

vi.mock("@/bindings", () => ({ commands: { killEngines } }));
vi.mock("../chess", () => ({ parsePGN }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn<() => void>() }));
vi.mock("@tauri-apps/plugin-log", () => ({ error: vi.fn<() => void>() }));
const { show } = vi.hoisted(() => ({ show: vi.fn<(...a: unknown[]) => void>() }));
vi.mock("@mantine/notifications", () => ({ notifications: { show } }));
vi.mock("@/features/files/utils/file", () => ({
    fileMetadataSchema: z.any(),
    invalidateFileIndex: vi.fn<() => void>(),
}));

import { open, readTree } from "@/state/tabSession";
import { tabsAtom } from "@/state/uiAtoms";
import { defaultTree } from "../treeReducer";
import { PgnParseError } from "../pgnParseError";
import type { Tab } from "../tabSchema";
import { createTab } from "../tabs";

const jotai = getDefaultStore();

beforeEach(() => {
    vi.clearAllMocks();
    sessionStorage.clear();
    killEngines.mockResolvedValue({ status: "ok", data: null });
    jotai.set(tabsAtom, []);
});

test("a tab created from a tree has its tree stored before it is added to the tab list", async () => {
    const tree = defaultTree();
    tree.headers.event = "First render";
    let storedWhenAdded: string | undefined;
    const setTabs = (update: React.SetStateAction<Tab[]>) => {
        const next = typeof update === "function" ? update([]) : update;
        storedWhenAdded = readTree(next[0].value)?.headers.event;
    };

    await createTab({
        tab: { name: "g", type: "analysis" },
        setTabs,
        setActiveTab: vi.fn<() => void>(),
        tree,
    });

    expect(storedWhenAdded).toBe("First render");
});

test("replacing the lone new tab disposes the replaced tab", async () => {
    jotai.set(tabsAtom, [{ name: "New", value: "lone", type: "new" }]);
    open("lone", defaultTree());

    const id = await createTab({
        tab: { name: "g", type: "analysis" },
        setTabs: (update) => jotai.set(tabsAtom, update as (prev: Tab[]) => Tab[]),
        setActiveTab: vi.fn<() => void>(),
        fen: "8/8/8/8/8/8/8/K6k w - - 0 1",
    });
    await new Promise((r) => setTimeout(r, 0));

    expect(jotai.get(tabsAtom).map((t) => t.value)).toEqual([id]);
    expect(readTree("lone")).toBeNull();
    expect(killEngines).toHaveBeenCalledWith("lone");
});

test("a game that does not parse opens no tab: createTab notifies and resolves null", async () => {
    parsePGN.mockRejectedValueOnce(new PgnParseError("Qh8", 20));
    const setTabs = vi.fn<() => void>();

    const id = await createTab({
        tab: { name: "g", type: "analysis" },
        setTabs,
        setActiveTab: vi.fn<() => void>(),
        pgn: "1. e4 *",
    });

    expect(id).toBeNull();
    expect(setTabs).not.toHaveBeenCalled();
    expect(show).toHaveBeenCalledTimes(1);
});

test("a tab whose tree could not be stored is not opened", async () => {
    const spy = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
        throw new DOMException("full", "QuotaExceededError");
    });
    const setTabs = vi.fn<() => void>();
    try {
        const id = await createTab({
            tab: { name: "g", type: "analysis" },
            setTabs,
            setActiveTab: vi.fn<() => void>(),
            tree: defaultTree(),
        });
        expect(id).toBeNull();
        expect(setTabs).not.toHaveBeenCalled();
    } finally {
        spy.mockRestore();
    }
});
