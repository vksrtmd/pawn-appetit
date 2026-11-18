import { appDataDir, resolve } from "@tauri-apps/api/path";
import { readTextFile, writeTextFile } from "@tauri-apps/plugin-fs";

/**
 * Stores analyzed PGNs for Chess.com and Lichess games.
 * Key: game identifier (URL for Chess.com, ID for Lichess)
 * Value: analyzed PGN string
 */
interface AnalyzedGamesMap {
    [gameId: string]: string;
}

const FILENAME = "analyzed_games.json";

/**
 * Save an analyzed PGN for a game
 * @param gameId - Unique identifier (URL for Chess.com, ID for Lichess)
 * @param analyzedPgn - The analyzed PGN string
 */
export async function saveAnalyzedGame(gameId: string, analyzedPgn: string): Promise<void> {
    const dir = await appDataDir();
    const file = await resolve(dir, FILENAME);
    let analyzedGames: AnalyzedGamesMap = {};
    try {
        const text = await readTextFile(file);
        analyzedGames = JSON.parse(text);
    } catch {
        // file may not exist yet
    }
    analyzedGames[gameId] = analyzedPgn;
    await writeTextFile(file, JSON.stringify(analyzedGames));
}

/**
 * Get all analyzed games
 * @returns Map of game IDs to analyzed PGNs
 */
export async function getAllAnalyzedGames(): Promise<AnalyzedGamesMap> {
    const dir = await appDataDir();
    const file = await resolve(dir, FILENAME);
    try {
        const text = await readTextFile(file);
        return JSON.parse(text);
    } catch {
        return {};
    }
}

/**
 * Remove all analyzed games for a specific account
 * @param username - Username of the account
 * @param type - Type of account ("lichess" or "chesscom")
 */
export async function removeAnalyzedGamesForAccount(
    username: string,
    type: "lichess" | "chesscom",
): Promise<void> {
    const dir = await appDataDir();
    const file = await resolve(dir, FILENAME);
    let analyzedGames: AnalyzedGamesMap = {};
    try {
        const text = await readTextFile(file);
        analyzedGames = JSON.parse(text);
    } catch {
        return;
    }

    // Filter out games that belong to this account
    const filteredGames: AnalyzedGamesMap = {};
    for (const [gameId, pgn] of Object.entries(analyzedGames)) {
        let belongsToAccount = false;

        if (type === "lichess") {
            // For Lichess, gameId is the game ID, check if PGN contains the username
            // Lichess PGNs typically have White/Black headers with usernames
            const whiteMatch = pgn.match(/\[White\s+"([^"]+)"/);
            const blackMatch = pgn.match(/\[Black\s+"([^"]+)"/);
            const whiteName = whiteMatch ? whiteMatch[1] : "";
            const blackName = blackMatch ? blackMatch[1] : "";

            // Check if username matches either white or black player
            belongsToAccount =
                whiteName.toLowerCase() === username.toLowerCase() ||
                blackName.toLowerCase() === username.toLowerCase();
        } else if (type === "chesscom") {
            // For Chess.com, gameId is the URL, check if URL contains the username
            // Chess.com URLs are like: https://www.chess.com/game/live/123456
            // We need to check the PGN headers for the username
            const whiteMatch = pgn.match(/\[White\s+"([^"]+)"/);
            const blackMatch = pgn.match(/\[Black\s+"([^"]+)"/);
            const whiteName = whiteMatch ? whiteMatch[1] : "";
            const blackName = blackMatch ? blackMatch[1] : "";

            // Check if username matches either white or black player
            belongsToAccount =
                whiteName.toLowerCase() === username.toLowerCase() ||
                blackName.toLowerCase() === username.toLowerCase();
        }

        // Keep the game only if it does NOT belong to this account
        if (!belongsToAccount) {
            filteredGames[gameId] = pgn;
        }
    }

    await writeTextFile(file, JSON.stringify(filteredGames));
}
