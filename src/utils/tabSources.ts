import type { FileMetadata } from "@/features/files/utils/file";
import type { Tab } from "./tabSchema";

/** Points every tab whose source is the file at `from` to the renamed file. */
export function followRename(tabs: Tab[], from: string, to: FileMetadata): Tab[] {
    return tabs.map((tab) =>
        tab.source?.type === "file" && tab.source.path === from
            ? { ...tab, source: { ...to, numGames: tab.source.numGames } }
            : tab,
    );
}

/**
 * Drops the file link of tabs whose source is the deleted `path` (or lies inside it, when it
 * is a directory), so autosave does not recreate the file.
 */
export function dropFileLinks(tabs: Tab[], path: string): Tab[] {
    return tabs.map((tab) =>
        tab.source?.type === "file" &&
        (tab.source.path === path || tab.source.path.startsWith(`${path}/`))
            ? { ...tab, source: null }
            : tab,
    );
}
