/**
 * Tab session: the one place that saves a board tab.
 *
 * `registerTabStore` maps a tab id to that tab's tree store, so code above the boards (the close
 * prompt, the menu) can reach a tab's moves by id. A tab whose board is not mounted has no
 * registered store; its state is then rehydrated from session storage.
 *
 * `save(tabId, { dir })` writes the tab's game to where the tab came from: its file, its database,
 * or a file the user picks in a save-as dialog (no file, or a temp import file). On success the
 * tab's store is marked saved. A failed write rejects and leaves the tab dirty. A cancelled
 * dialog resolves "cancelled" without writing.
 *
 * The module also owns everything stored under a tab id: `open` writes a tab's first tree before
 * the tab is added, `duplicate` copies the tab with its tree, source and side keys, `dispose`
 * removes the tree, side keys, missing-moves entry and per-tab atoms and kills the tab's engines,
 * and the `close*` functions dispose each tab they remove. Nothing outside this module touches a
 * tab's session-storage keys.
 */
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { getDefaultStore } from "jotai";
import { commands } from "@/bindings";
import { invalidateFileIndex } from "@/features/files/utils/file";
import { getPGN } from "@/utils/chess";
import {
    createFile,
    getFileNameWithoutExtension,
    isTempImportFile,
    nowEpochSeconds,
} from "@/utils/files";
import { formatDateToPGN } from "@/utils/format";
import type { Tab } from "@/utils/tabSchema";
import { unwrap } from "@/utils/unwrap";
import {
    compressedSessionStorage,
    deserializeStorageValue,
    storeCompressed,
} from "@/utils/tabStateStorage";
import { genID } from "@/utils/tabSchema";
import type { TreeState } from "@/utils/treeReducer";
import { missingMovesAtom } from "./gameAtoms";
import { createTreeStore, type TreeStore } from "./store/tree";
import { disposeTabAtoms } from "./tabAtoms";
import { activeTabAtom, tabsAtom } from "./uiAtoms";

const stores = new Map<string, TreeStore>();

/** Registers `store` as the tree store of tab `tabId`. Returns the unregister function. */
export function registerTabStore(tabId: string, store: TreeStore): () => void {
    stores.set(tabId, store);
    return () => {
        if (stores.get(tabId) === store) stores.delete(tabId);
    };
}

export function getTabStore(tabId: string): TreeStore {
    return stores.get(tabId) ?? createTreeStore(tabId);
}

export type SaveResult = "saved" | "cancelled";

export async function save(tabId: string, { dir }: { dir: string }): Promise<SaveResult> {
    const jotai = getDefaultStore();
    const tab = jotai.get(tabsAtom).find((t) => t.value === tabId);
    const store = getTabStore(tabId);
    const { root, headers } = store.getState();
    const pgn = `${getPGN(root, {
        headers,
        comments: true,
        extraMarkups: true,
        glyphs: true,
        variations: true,
    })}\n\n`;
    const source = tab?.source;

    if (source?.type === "db") {
        unwrap(await commands.updateGame(source.db, source.id, { ...headers, moves: pgn }));
        store.getState().save();
        return "saved";
    }

    const setSource = (next: Tab["source"]) =>
        jotai.set(tabsAtom, (prev) =>
            prev.map((t) => (t.value === tabId ? { ...t, source: next } : t)),
        );

    let filePath: string;
    let newSource: Tab["source"];
    if (source?.type === "file" && !isTempImportFile(source.path)) {
        filePath = source.path;
    } else {
        const userChoice = await saveDialog({
            defaultPath: `${dir}/analyze-game-${formatDateToPGN(new Date())}.pgn`,
            filters: [{ name: "PGN", extensions: ["pgn"] }],
        });
        if (userChoice === null) return "cancelled";
        // On Linux the dialog filter doesn't append the extension.
        filePath = /\.pgn$/i.test(userChoice) ? userChoice : `${userChoice}.pgn`;
        const fileName = await getFileNameWithoutExtension(filePath);
        newSource = {
            ...(source ?? {
                type: "file",
                numGames: 1,
                metadata: { type: "game", tags: [] },
            }),
            name: fileName,
            path: filePath,
            lastModified: nowEpochSeconds(),
        } as Tab["source"];
        if (source?.type === "file") {
            const count = unwrap(await commands.countPgnGames(source.path));
            const games = unwrap(await commands.readGames(source.path, 0, count - 1));
            await createFile({ filename: fileName, filetype: "game", pgn: games.join(""), dir });
            // The copy exists now: the tab leaves the temp file even if the write below fails.
            setSource(newSource);
        }
    }

    unwrap(await commands.writeGame(filePath, tab?.gameNumber || 0, pgn));
    invalidateFileIndex();
    if (newSource) setSource(newSource);
    store.getState().save();
    return "saved";
}

// ---------------------------------------------------------------------------
// Everything stored under a tab id
// ---------------------------------------------------------------------------

/** The key of the zustand `persist` envelope the tree store writes (its default version). */
const TREE_VERSION = 0;

const SIDE_KEYS = ["localGameId", "chessComGameUrl", "lichessGameId"] as const;
/** Ids a tab keeps next to its tree, for saving the analyzed game back to where it came from. */
export type SideKey = (typeof SIDE_KEYS)[number];

export function getSideKey(tabId: string, key: SideKey): string | null {
    return sessionStorage.getItem(`${tabId}_${key}`);
}

export function setSideKey(tabId: string, key: SideKey, value: string): void {
    sessionStorage.setItem(`${tabId}_${key}`, value);
}

/** The tab's stored tree, or null when it has none or it is unreadable. */
export function readTree(tabId: string): TreeState | null {
    const raw = compressedSessionStorage.getItem(tabId);
    if (typeof raw !== "string") return null;
    return deserializeStorageValue<{ state?: TreeState }>(raw)?.state ?? null;
}

/**
 * Stores `tree` as tab `tabId`'s tree, so the board's first mount (or the next one) loads it. Call
 * it before the tab is added to the tab list. Returns false when the tree could not be stored
 * (storage full): the caller must not open a tab for it.
 */
export function open(tabId: string, tree: TreeState): boolean {
    return storeCompressed(tabId, JSON.stringify({ version: TREE_VERSION, state: tree }));
}

/** Whether tab `tabId` has unsaved changes: its live store when mounted, else its stored tree. */
export function isDirty(tabId: string): boolean {
    const store = stores.get(tabId);
    return store ? store.getState().dirty : (readTree(tabId)?.dirty ?? false);
}

/** Adds a copy of tab `tabId` with its tree, source and side keys, and makes it the active tab. */
export function duplicate(tabId: string): string | null {
    const jotai = getDefaultStore();
    const tab = jotai.get(tabsAtom).find((t) => t.value === tabId);
    if (!tab) return null;
    const id = genID();
    try {
        const tree = compressedSessionStorage.getItem(tabId);
        if (typeof tree === "string") compressedSessionStorage.setItem(id, tree);
        for (const key of SIDE_KEYS) {
            const value = getSideKey(tabId, key);
            if (value !== null) setSideKey(id, key, value);
        }
    } catch (error) {
        console.error(`Failed to duplicate tab state from ${tabId} to ${id}`, error);
    }
    jotai.set(tabsAtom, (prev) => [
        ...prev,
        {
            name: tab.name,
            value: id,
            type: tab.type,
            source: tab.source,
            gameNumber: tab.gameNumber,
        },
    ]);
    jotai.set(activeTabAtom, id);
    return id;
}

/** Removes everything stored for tab `tabId` and kills its engines. Safe to call twice. */
export async function dispose(tabId: string): Promise<void> {
    compressedSessionStorage.removeItem(tabId);
    for (const key of SIDE_KEYS) sessionStorage.removeItem(`${tabId}_${key}`);
    getDefaultStore().set(missingMovesAtom, (prev) => {
        if (!(tabId in prev)) return prev;
        const { [tabId]: _removed, ...rest } = prev;
        return rest;
    });
    disposeTabAtoms(tabId);
    try {
        unwrap(await commands.killEngines(tabId));
    } catch (error) {
        console.error(`Failed to kill engines for tab: ${tabId}`, error);
    }
}

// ---------------------------------------------------------------------------
// Closing tabs: each removes tabs from the list and disposes them
// ---------------------------------------------------------------------------

function removeTabs(keep: (tab: Tab, index: number) => boolean, active?: string | null): void {
    const jotai = getDefaultStore();
    const tabs = jotai.get(tabsAtom);
    const kept = tabs.filter(keep);
    jotai.set(tabsAtom, kept);
    if (active !== undefined) jotai.set(activeTabAtom, active);
    for (const tab of tabs) if (!kept.includes(tab)) void dispose(tab.value);
}

/**
 * Closes tab `tabId`. When it is the active tab, its neighbour (the tab that takes its place, or
 * the previous one at the end) becomes active; otherwise the active tab stays.
 */
export function closeTab(tabId: string): void {
    const jotai = getDefaultStore();
    const tabs = jotai.get(tabsAtom);
    const index = tabs.findIndex((t) => t.value === tabId);
    if (index === -1) return;
    const next = tabs.filter((_, i) => i !== index);
    const active =
        jotai.get(activeTabAtom) === tabId
            ? (next[Math.min(index, next.length - 1)]?.value ?? null)
            : undefined;
    removeTabs((_, i) => i !== index, active);
}

/** Closes the active tab (Ctrl/Cmd+W) and activates its neighbour. */
export function closeActiveTab(): void {
    const tabId = getDefaultStore().get(activeTabAtom);
    if (tabId) closeTab(tabId);
}

export function closeOtherTabs(tabId: string): void {
    removeTabs((t) => t.value === tabId, tabId);
}

export function closeTabsToRight(tabId: string): void {
    const index = getDefaultStore()
        .get(tabsAtom)
        .findIndex((t) => t.value === tabId);
    if (index === -1) return;
    removeTabs((_, i) => i <= index, tabId);
}

export function closeAllTabs(): void {
    removeTabs(() => false, null);
}
