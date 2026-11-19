import { commands } from "@/bindings";
import { dispose, open } from "@/state/tabSession";
import { unwrap } from "@/utils/unwrap";
import { parsePGN } from "./chess";
import { notifyPgnParseError } from "./pgnParseError";
import { defaultTree, type GameHeaders, type TreeState } from "./treeReducer";

import { genID, type Tab, type EntitySourceMetadata } from "./tabSchema";

export async function createTab({
    tab,
    setTabs,
    setActiveTab,
    pgn,
    headers,
    srcInfo,
    gameNumber,
    position,
    fen,
    tree: openedTree,
}: {
    tab: Omit<Tab, "value">;
    setTabs: React.Dispatch<React.SetStateAction<Tab[]>>;
    setActiveTab: React.Dispatch<React.SetStateAction<string | null>>;
    pgn?: string;
    headers?: GameHeaders;
    srcInfo?: EntitySourceMetadata;
    gameNumber?: number;
    position?: number[];
    /** Start the new tab from this position, with no move history. */
    fen?: string;
    /** Start the new tab from this parsed tree. */
    tree?: TreeState;
}): Promise<string | null> {
    const id = genID();

    if (openedTree) {
        // The quota is full: a tab without its tree would open empty over its source
        if (!open(id, openedTree)) return null;
    } else if (pgn !== undefined) {
        // For variants files, parse as normal PGN (with variations) but display in variants view
        // Don't use isVariantsMode for parsing - that's only for special PGNs where all sequences are variations
        let tree: TreeState;
        try {
            tree = await parsePGN(pgn, headers?.fen, false);
        } catch (e) {
            // The game is refused, not opened cut short; the user was told why.
            if (notifyPgnParseError(e)) return null;
            throw e;
        }
        // If headers are provided, only merge them if the parsed PGN headers are incomplete
        // This preserves complete headers from saved PGNs (like game.pgn) while allowing
        // updates for PGNs that were reconstructed from moves
        if (headers) {
            const parsedHeaders = tree.headers;
            // Check if parsed headers are complete (not just default values)
            const hasCompleteHeaders =
                parsedHeaders.event &&
                parsedHeaders.event !== "?" &&
                parsedHeaders.site &&
                parsedHeaders.site !== "?" &&
                parsedHeaders.white &&
                parsedHeaders.white !== "?" &&
                parsedHeaders.black &&
                parsedHeaders.black !== "?";

            if (hasCompleteHeaders) {
                // PGN has complete headers, preserve them (especially FEN which is the initial position)
                // Only update fields that are explicitly provided and missing in parsed headers
                tree.headers = {
                    ...parsedHeaders,
                    // Preserve FEN from parsed headers (it's the initial FEN from PGN)
                    fen: parsedHeaders.fen,
                    // Only override if provided and missing in parsed headers
                    time_control: parsedHeaders.time_control || headers.time_control,
                    variant: parsedHeaders.variant || headers.variant,
                };
            } else {
                // PGN headers are incomplete, merge with provided headers
                // But always preserve FEN from parsed headers if it exists
                tree.headers = {
                    ...parsedHeaders,
                    ...headers,
                    fen: parsedHeaders.fen || headers.fen,
                };
            }
            if (position) {
                tree.position = position;
            }
        }
        if (!open(id, tree)) return null;
    } else if (fen) {
        if (!open(id, defaultTree(fen))) return null;
    }

    setTabs((prev) => {
        if (
            prev.length === 0 ||
            (prev.length === 1 && prev[0].type === "new" && tab.type !== "new")
        ) {
            // Disposing twice (a re-run updater) is harmless
            if (prev.length === 1) void dispose(prev[0].value);
            return [
                {
                    ...tab,
                    value: id,
                    source: srcInfo,
                    gameNumber,
                },
            ];
        }
        return [
            ...prev,
            {
                ...tab,
                value: id,
                source: srcInfo,
                gameNumber,
            },
        ];
    });
    setActiveTab(id);
    return id;
}

// A tab is created or reloaded only after its PGN parsed, so the error leaves nothing half-open
function rethrowNotified(e: unknown): never {
    notifyPgnParseError(e);
    throw e;
}

export async function reloadTab(tab: Tab): Promise<TreeState | undefined> {
    let tree: TreeState | undefined;

    if (tab.source?.type === "file") {
        const game = unwrap(await commands.readGames(tab.source.path, 0, 0))[0];

        // For variants files, parse as normal PGN (with variations) but display in variants view
        // Don't use isVariantsMode for parsing - that's only for special PGNs where all sequences are variations
        tree = await parsePGN(game, undefined, false).catch(rethrowNotified);
    } else if (tab.source?.type === "db") {
        const game = unwrap(await commands.getGame(tab.source.db, tab.source.id));

        tree = await parsePGN(game.moves).catch(rethrowNotified);
        tree.headers = game;
    }

    if (tree != null) {
        open(tab.value, tree);
        return tree;
    }
}
