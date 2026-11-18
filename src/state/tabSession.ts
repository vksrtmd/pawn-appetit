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
import type { Tab } from "@/utils/tabs";
import { unwrap } from "@/utils/unwrap";
import { createTreeStore, type TreeStore } from "./store/tree";
import { tabsAtom } from "./uiAtoms";

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
        if (source?.type === "file") {
            const count = unwrap(await commands.countPgnGames(source.path));
            const games = unwrap(await commands.readGames(source.path, 0, count - 1));
            await createFile({ filename: fileName, filetype: "game", pgn: games.join(""), dir });
        }
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
    }

    unwrap(await commands.writeGame(filePath, tab?.gameNumber || 0, pgn));
    invalidateFileIndex();
    if (newSource) {
        jotai.set(tabsAtom, (prev) =>
            prev.map((t) => (t.value === tabId ? { ...t, source: newSource } : t)),
        );
    }
    store.getState().save();
    return "saved";
}
