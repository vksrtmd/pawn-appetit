import { getGameStats, parsePGN } from "@/utils/chess";
import { readHistoryFile, updateHistoryFile } from "@/utils/historyFile";

export interface GameRecord {
    id: string;
    white: {
        type: "human" | "engine";
        name?: string;
        engine?: string;
    };
    black: {
        type: "human" | "engine";
        name?: string;
        engine?: string;
    };
    result: string;
    timeControl?: string;
    timestamp: number;
    moves: string[];
    variant?: string;
    fen: string; // Final FEN position
    initialFen?: string; // Initial FEN position (if different from standard)
    pgn?: string; // Full PGN with headers and moves
}

const FILENAME = "played_games.json";

function notifyGamesUpdated() {
    if (typeof window !== "undefined") {
        try {
            window.dispatchEvent(new Event("games:updated"));
        } catch {
            // ignore
        }
    }
}

export async function saveGameRecord(record: GameRecord) {
    await updateHistoryFile<GameRecord[]>(FILENAME, [], (records) => [record, ...records]);
    notifyGamesUpdated();
}

export async function getRecentGames(limit = 20): Promise<GameRecord[]> {
    try {
        const records = await readHistoryFile<GameRecord[]>(FILENAME, []);

        // Filter out invalid/corrupted games
        const validRecords = records.filter((record) => {
            // Must have an id
            if (!record.id) return false;

            // Must have valid player information
            if (!record.white || !record.black) return false;
            if (!record.white.type || !record.black.type) return false;

            // Must have moves array (can be empty but must exist)
            if (!Array.isArray(record.moves)) return false;

            // Must have a valid timestamp
            if (!record.timestamp || typeof record.timestamp !== "number") return false;

            // Must have a result (can be "*" for unfinished games)
            if (!record.result || typeof record.result !== "string") return false;

            // Must have a FEN
            if (!record.fen || typeof record.fen !== "string") return false;

            return true;
        });

        return validRecords.slice(0, limit);
    } catch {
        return [];
    }
}

export async function countGamesOnDate(date: Date = new Date()): Promise<number> {
    try {
        const records = await readHistoryFile<GameRecord[]>(FILENAME, []);
        const y = date.getFullYear();
        const m = date.getMonth();
        const d = date.getDate();
        return records.filter((r) => {
            const dt = new Date(r.timestamp);
            return dt.getFullYear() === y && dt.getMonth() === m && dt.getDate() === d;
        }).length;
    } catch {
        return 0;
    }
}

export async function clearAllGames(): Promise<void> {
    await updateHistoryFile<GameRecord[]>(FILENAME, [], () => []);
    notifyGamesUpdated();
}

export async function updateGameRecord(
    gameId: string,
    updates: Partial<GameRecord>,
): Promise<void> {
    const changed = await updateHistoryFile<GameRecord[]>(FILENAME, [], (records) => {
        const index = records.findIndex((r) => r.id === gameId);
        if (index === -1) return undefined;
        return records.map((r, i) => (i === index ? { ...r, ...updates } : r));
    });
    if (changed) notifyGamesUpdated();
}

export async function deleteGameRecord(gameId: string): Promise<void> {
    const changed = await updateHistoryFile<GameRecord[]>(FILENAME, [], (records) =>
        records.some((r) => r.id === gameId) ? records.filter((r) => r.id !== gameId) : undefined,
    );
    if (changed) notifyGamesUpdated();
}

export interface GameStats {
    accuracy: number;
    acpl: number; // Average Centipawns Loss
}

/**
 * Calculate accuracy and ACPL for a game record from its PGN.
 * Returns null if PGN is not available or doesn't contain evaluations.
 */
export async function calculateGameStats(game: GameRecord): Promise<GameStats | null> {
    if (!game.pgn) {
        return null;
    }

    try {
        // Parse the PGN to get the game tree with evaluations
        const tree = await parsePGN(game.pgn, game.initialFen);

        // Calculate stats using the same function used in the analysis panel
        const stats = getGameStats(tree.root);

        // Determine which color the user played
        const isUserWhite = game.white.type === "human";
        const userColor = isUserWhite ? "white" : "black";

        // Get stats for the user's color
        const accuracy = userColor === "white" ? stats.whiteAccuracy : stats.blackAccuracy;
        const acpl = userColor === "white" ? stats.whiteCPL : stats.blackCPL;

        // Return null if no evaluations were found (accuracy and ACPL would be 0)
        if (accuracy === 0 && acpl === 0) {
            return null;
        }

        return {
            accuracy,
            acpl,
        };
    } catch {
        // If parsing fails, return null
        return null;
    }
}
