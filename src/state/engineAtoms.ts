// Engine management atoms: list, settings, best-move families, enable/disable.
import { parseUci } from "chessops";
import { INITIAL_FEN, makeFen } from "chessops/fen";
import equal from "fast-deep-equal";
import { atom } from "jotai";
import { atomWithStorage } from "jotai/utils";
import { atomFamily } from "jotai-family";
import { tabAtomFamily } from "./tabAtoms";
import type { BestMoves, GoMode } from "@/bindings";
import { positionFromFen, swapMove } from "@/utils/chessops";
import { type Engine, type EngineSettings, engineSchema } from "@/utils/engines";
import { getWinChance, normalizeScore } from "@/utils/score";
import { createAsyncZodStorage, fileStorage, loadable, zodArray } from "./utils";
import { activeTabAtom } from "./uiAtoms";

// Engine list
export const enginesAtom = atomWithStorage<Engine[]>(
    "engines/engines.json",
    [],
    createAsyncZodStorage(zodArray(engineSchema), fileStorage),
);

export const loadableEnginesAtom = loadable(enginesAtom);

// CP / WDL display mode
export const reportTypeAtom = atom<"CP" | "WDL">("CP");
export const scoreTypeFamily = atomFamily((_engine: string) => atom<"cp" | "wdl">("cp"));

// Per-engine-tab move/progress atoms
export const engineMovesFamily = tabAtomFamily(
    (_: { tab: string; engine: string }) => atom<Map<string, BestMoves[]>>(new Map()),
    { tabOf: (p) => p.tab, areEqual: (a, b) => a.tab === b.tab && a.engine === b.engine },
);

export const engineProgressFamily = tabAtomFamily(
    (_: { tab: string; engine: string }) => atom<number>(0),
    { tabOf: (p) => p.tab, areEqual: (a, b) => a.tab === b.tab && a.engine === b.engine },
);

// Per-engine-tab settings (enabled state, go mode, uci options)
export const tabEngineSettingsFamily = tabAtomFamily(
    ({
        defaultSettings,
        defaultGo,
    }: {
        tab: string;
        engineName: string;
        defaultSettings?: EngineSettings;
        defaultGo?: GoMode;
    }) => {
        return atom<{
            enabled: boolean;
            settings: EngineSettings;
            go: GoMode;
            synced: boolean;
        }>({
            enabled: false,
            settings: defaultSettings || [],
            go: defaultGo || { t: "Infinite" },
            synced: true,
        });
    },
    { tabOf: (p) => p.tab, areEqual: (a, b) => a.tab === b.tab && a.engineName === b.engineName },
);

// Aggregate enable/disable atoms
export const allEnabledAtom = loadable(
    atom(async (get) => {
        const engines = await get(enginesAtom);

        const v = engines
            .filter((e) => e.loaded)
            .every((engine) => {
                const a = tabEngineSettingsFamily({
                    tab: get(activeTabAtom)!,
                    engineName: engine.name,
                    defaultSettings: engine.type === "local" ? engine.settings || [] : undefined,
                    defaultGo: engine.go ?? undefined,
                });
                return get(a).enabled;
            });

        return v;
    }),
);

export const enableAllAtom = atom(null, (get, set, value: boolean) => {
    const engines = get(loadableEnginesAtom);
    if (!(engines.state === "hasData")) return;

    for (const engine of engines.data.filter((e) => e.loaded)) {
        const a = tabEngineSettingsFamily({
            tab: get(activeTabAtom)!,
            engineName: engine.name,
            defaultSettings: engine.type === "local" ? engine.settings || [] : undefined,
            defaultGo: engine.go ?? undefined,
        });
        set(a, { ...get(a), enabled: value });
    }
});

/**
 * The lines to draw as arrows for a position: the threat lines (searched with the side to move
 * swapped) when there are any, else the lines of the position itself. Win chances and the
 * 10-point cut-off are taken from the side to move of the position the lines belong to.
 */
export function arrowLines(
    engineMoves: Map<string, BestMoves[]>,
    fen: string,
    gameMoves: string[],
): { pv: string[]; winChance: number }[] | null {
    const [pos] = positionFromFen(fen);
    let finalFen = INITIAL_FEN;
    if (pos) {
        for (const move of gameMoves) {
            pos.play(parseUci(move)!);
        }
        finalFen = makeFen(pos.toSetup());
    }
    const threat = engineMoves.get(`${swapMove(finalFen)}:`);
    const moves = threat?.length ? threat : engineMoves.get(`${fen}:${gameMoves.join(",")}`);
    if (!moves?.length) return null;
    const turn = pos?.turn ?? "white";
    const color = threat?.length ? (turn === "white" ? "black" : "white") : turn;
    const winChanceOf = (m: BestMoves) => getWinChance(normalizeScore(m.score.value, color));
    const best = winChanceOf(moves[0]);
    return moves
        .map((m) => ({ pv: m.uciMoves, winChance: winChanceOf(m) }))
        .filter((m) => best - m.winChance < 10);
}

// Best moves derived atom (aggregates all engine outputs for a position)
/** Returns the best moves of each loaded engine for the given FEN + game moves. */
export const bestMovesFamily = atomFamily(
    ({ fen, gameMoves }: { fen: string; gameMoves: string[] }) =>
        atom<Map<number, { pv: string[]; winChance: number }[]>>((get) => {
            const tab = get(activeTabAtom);
            if (!tab) return new Map();
            const engines = get(loadableEnginesAtom);
            if (!(engines.state === "hasData")) return new Map();
            const bestMoves = new Map<number, { pv: string[]; winChance: number }[]>();
            let n = 0;
            for (const engine of engines.data.filter((e) => e.loaded)) {
                const engineMoves = get(engineMovesFamily({ tab, engine: engine.name }));
                const lines = arrowLines(engineMoves, fen, gameMoves);
                if (lines) bestMoves.set(n, lines);
                n++;
            }
            return bestMoves;
        }),
    (a, b) => a.fen === b.fen && equal(a.gameMoves, b.gameMoves),
);
