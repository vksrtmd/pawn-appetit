/**
 * Game, puzzle, database selection, and practice atoms.
 * Per-tab game state uses `tabValue` imported from uiAtoms.
 */
import { atom } from "jotai";
import { atomWithStorage, createJSONStorage } from "jotai/utils";
import { atomFamily } from "jotai-family";
import type { SyncStorage } from "jotai/vanilla/utils/atomWithStorage";
import type { ReviewLog } from "ts-fsrs";
import { z } from "zod";
import { commands } from "@/bindings";
import type { OpponentSettings } from "@/features/boards/components/BoardGame";
import { type Position, positionSchema } from "@/features/files/utils/opening";
import type { SuccessDatabaseInfo } from "@/utils/db";
import type { MissingMove } from "@/utils/repertoire";
import { createZodStorage } from "./utils";
import { tabValue } from "./uiAtoms";

// ---------------------------------------------------------------------------
// Database selection
// ---------------------------------------------------------------------------

/** The path the user picked. It may point at a file that has since been deleted. */
export const storedReferenceDbAtom = atomWithStorage<string | null>("reference-database", null);

/** The reference database if its file still opens, otherwise null. */
export const referenceDbAtom = unwrap(
    atom(async (get) => {
        const path = get(storedReferenceDbAtom);
        if (!path) return null;
        try {
            const result = await commands.getDbInfo(path);
            return result.status === "ok" ? path : null;
        } catch {
            return null;
        }
    }),
    () => null,
);
/** Forget `file` as the reference database, if it is the one. For when it is deleted. */
export const forgetReferenceDbAtom = atom(null, (get, set, file: string) => {
    if (get(storedReferenceDbAtom) === file) set(storedReferenceDbAtom, null);
});

export const selectedPuzzleDbAtom = atomWithStorage<string | null>("puzzle-db", null);
export const selectedDatabaseAtom = atomWithStorage<SuccessDatabaseInfo | null>(
    "database-view",
    null,
    createJSONStorage(() => sessionStorage),
);

// ---------------------------------------------------------------------------
// Puzzle settings
// ---------------------------------------------------------------------------

export const hidePuzzleRatingAtom = atomWithStorage<boolean>("hide-puzzle-rating", false);
export const progressivePuzzlesAtom = atomWithStorage<boolean>("progressive-puzzles", false);
export const jumpToNextPuzzleAtom = atomWithStorage<"off" | "success" | "success-and-failure">(
    "puzzle-jump-next",
    "success",
);
export const puzzleRatingRangeAtom = atomWithStorage<[number, number]>(
    "puzzle-ratings",
    [1000, 1500],
);
export const inOrderPuzzlesAtom = atomWithStorage<boolean>("puzzle-in-order", false);
export const puzzlePlayerRatingAtom = atomWithStorage<number>("puzzle-player-rating", 1500);
export const maxPuzzlePlayerRatingAtom = atomWithStorage<number>("puzzle-max-player-rating", 1500);

// ---------------------------------------------------------------------------
// Practice / Repertoire training
// ---------------------------------------------------------------------------

export type PracticeAnimationSpeed =
    | "disabled"
    | "very-fast"
    | "fast"
    | "normal"
    | "slow"
    | "very-slow";
export const practiceAnimationSpeedAtom = atomWithStorage<PracticeAnimationSpeed>(
    "practice-animation-speed",
    "normal",
);

type TabMap<T> = Record<string, T>;
export const missingMovesAtom = atomWithStorage<TabMap<MissingMove[] | null>>(
    "missing-moves",
    {},
    createJSONStorage(() => sessionStorage),
);

const reviewLogSchema = z
    .object({
        fen: z.string(),
    })
    .passthrough();

const practiceDataSchema = z.object({
    positions: positionSchema.array(),
    logs: reviewLogSchema.array(),
});

export type PracticeData = {
    positions: Position[];
    logs: (ReviewLog & { fen: string })[];
};

export const deckAtomFamily = atomFamily(
    ({ file, game }: { file: string; game: number }) =>
        atomWithStorage<PracticeData>(
            `deck-${file}-${game}`,
            {
                positions: [],
                logs: [],
            },
            createZodStorage(practiceDataSchema, localStorage) as any as SyncStorage<PracticeData>, // TODO: fix types
        ),
    (a, b) => a.file === b.file && a.game === b.game,
);

// ---------------------------------------------------------------------------
// Per-tab game state
// ---------------------------------------------------------------------------

export type GameState = "settingUp" | "playing" | "gameOver";
const gameStateFamily = atomFamily((_tab: string) => atom<GameState>("settingUp"));
export const currentGameStateAtom = tabValue(gameStateFamily);

const playersFamily = atomFamily((_tab: string) =>
    atom<{
        white: OpponentSettings;
        black: OpponentSettings;
    }>({ white: {} as OpponentSettings, black: {} as OpponentSettings }),
);
export const currentPlayersAtom = tabValue(playersFamily);

const currentPuzzleFamily = atomFamily((_tab: string) => atom(0));
export const currentPuzzleAtom = tabValue(currentPuzzleFamily);
