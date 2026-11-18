import { getDefaultStore } from "jotai";
import { beforeEach, expect, test, vi } from "vitest";
import { z } from "zod";

const { killEngines } = vi.hoisted(() => ({
    killEngines: vi.fn<(...a: unknown[]) => Promise<unknown>>(),
}));

vi.mock("@/bindings", () => ({ commands: { killEngines } }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn<() => void>() }));
vi.mock("@tauri-apps/plugin-log", () => ({ error: vi.fn<() => void>() }));
vi.mock("@mantine/notifications", () => ({ notifications: { show: vi.fn<() => void>() } }));
vi.mock("@/features/files/utils/file", () => ({
    fileMetadataSchema: z.any(),
    invalidateFileIndex: vi.fn<() => void>(),
}));

import { open, readTree } from "@/state/tabSession";
import { tabsAtom } from "@/state/uiAtoms";
import { defaultTree } from "../treeReducer";
import { createTab, type Tab } from "../tabs";

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
