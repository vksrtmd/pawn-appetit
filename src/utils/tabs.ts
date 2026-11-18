import { z } from "zod";
import { commands } from "@/bindings";
import { fileMetadataSchema } from "@/features/files/utils/file";
import { serializeStorageValue } from "./tabStateStorage";
import { unwrap } from "@/utils/unwrap";
import { parsePGN } from "./chess";
import { defaultTree, type GameHeaders, type TreeState } from "./treeReducer";

const dbGameMetadataSchema = z.object({
    type: z.literal("db"),
    db: z.string(),
    id: z.number(),
});
export type DbGameMetadata = z.infer<typeof dbGameMetadataSchema>;

const entitySourceMetadataSchema = z.union([fileMetadataSchema, dbGameMetadataSchema]);

export type EntitySourceMetadata = z.infer<typeof entitySourceMetadataSchema>;

export const tabSchema = z.object({
    name: z.string(),
    value: z.string(),
    type: z.enum(["new", "play", "analysis", "puzzles"]),
    gameNumber: z.number().nullish(),
    source: entitySourceMetadataSchema.nullish(),
    meta: z
        .object({
            timeControl: z.object({
                seconds: z.number(),
                increment: z.number(),
            }),
        })
        .optional(),
});

export type Tab = z.infer<typeof tabSchema>;

export function genID() {
    function S4() {
        return (((1 + Math.random()) * 0x10000) | 0).toString(16).substring(1);
    }
    return S4() + S4();
}

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
}) {
    const id = genID();

    if (pgn !== undefined) {
        // For variants files, parse as normal PGN (with variations) but display in variants view
        // Don't use isVariantsMode for parsing - that's only for special PGNs where all sequences are variations
        const tree = await parsePGN(pgn, headers?.fen, false);
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
        sessionStorage.setItem(id, serializeStorageValue({ version: 0, state: tree }));
    } else if (fen) {
        const tree = defaultTree(fen);
        sessionStorage.setItem(id, serializeStorageValue({ version: 0, state: tree }));
    }

    setTabs((prev) => {
        if (
            prev.length === 0 ||
            (prev.length === 1 && prev[0].type === "new" && tab.type !== "new")
        ) {
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

export async function reloadTab(tab: Tab): Promise<TreeState | undefined> {
    let tree: TreeState | undefined;

    if (tab.source?.type === "file") {
        const game = unwrap(await commands.readGames(tab.source.path, 0, 0))[0];

        // For variants files, parse as normal PGN (with variations) but display in variants view
        // Don't use isVariantsMode for parsing - that's only for special PGNs where all sequences are variations
        tree = await parsePGN(game, undefined, false);
    } else if (tab.source?.type === "db") {
        const game = unwrap(await commands.getGame(tab.source.db, tab.source.id));

        tree = await parsePGN(game.moves);
        tree.headers = game;
    }

    if (tree != null) {
        sessionStorage.setItem(tab.value, serializeStorageValue({ version: 0, state: tree }));
        return tree;
    }
}
