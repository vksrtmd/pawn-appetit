import { expect, test } from "vitest";
import type { FileMetadata } from "@/features/files/utils/file";
import { dropFileLinks, followRename } from "../tabSources";
import type { Tab } from "../tabSchema";

const file = (path: string): FileMetadata => ({
    type: "file",
    name: path,
    path,
    numGames: 1,
    metadata: { type: "game", tags: [] },
    lastModified: 0,
});
const tab = (value: string, path?: string): Tab => ({
    name: value,
    value,
    type: "analysis",
    source: path ? file(path) : null,
});

test("renaming a file updates every tab whose source is that file", () => {
    const tabs = [tab("1", "/a.pgn"), tab("2", "/b.pgn"), tab("3", "/a.pgn")];
    const next = followRename(tabs, "/a.pgn", file("/c.pgn"));
    expect(next.map((t) => (t.source?.type === "file" ? t.source.path : null))).toEqual([
        "/c.pgn",
        "/b.pgn",
        "/c.pgn",
    ]);
});

test("deleting a file drops the file link of its tabs, so autosave has no path", () => {
    const tabs = [tab("1", "/a.pgn"), tab("2", "/b.pgn")];
    const next = dropFileLinks(tabs, "/a.pgn");
    expect(next[0].source).toBeNull();
    expect(next[1].source).toEqual(file("/b.pgn"));
});

test("deleting a directory drops the links of tabs for files inside it", () => {
    const tabs = [tab("1", "/dir/a.pgn"), tab("2", "/dir2/a.pgn")];
    const next = dropFileLinks(tabs, "/dir");
    expect(next.map((t) => t.source === null)).toEqual([true, false]);
});
