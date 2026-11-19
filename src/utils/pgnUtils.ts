/**
 * Utility functions for handling multiple PGN games
 */

import { parsePGN } from "@/utils/chess";
import type { TreeState } from "@/utils/treeReducer";

export interface ParsedGame {
    tree: TreeState;
    originalIndex: number;
}

export interface GameParseError {
    gameIndex: number;
    error: string;
    gameContent?: string;
}

/**
 * Split PGN content containing multiple games into individual game strings
 */
function splitPgnGames(content: string): string[] {
    // Normalize line endings and remove excessive whitespace
    const normalized = content.replace(/\r\n/g, "\n").replace(/\r/g, "\n");

    // Split on [Event tag which typically starts a new game
    const games = normalized.split(/(?=\[Event\s)/);

    // Filter out empty or whitespace-only games
    return games
        .map((game) => game.trim())
        .filter((game) => game.length > 0 && game.includes("[Event"));
}

/**
 * Parse multiple PGN games from text content
 */
export async function parseMultiplePgnGames(content: string): Promise<{
    games: ParsedGame[];
    errors: GameParseError[];
}> {
    const gameStrings = splitPgnGames(content);
    const games: ParsedGame[] = [];
    const errors: GameParseError[] = [];

    for (let i = 0; i < gameStrings.length; i++) {
        try {
            const gameContent = gameStrings[i];
            const tree = await parsePGN(gameContent);
            games.push({
                tree,
                originalIndex: i,
            });
        } catch (error) {
            errors.push({
                gameIndex: i,
                error: error instanceof Error ? error.message : String(error),
                gameContent:
                    gameStrings[i].substring(0, 200) + (gameStrings[i].length > 200 ? "..." : ""),
            });
        }
    }

    return { games, errors };
}
