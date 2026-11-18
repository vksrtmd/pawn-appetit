import { parseUci } from "chessops";
import type { BestMovesPayload, GoMode } from "@/bindings";
import type { TimeType } from "@/components/TimeInput";
import type { TimeControlField } from "@/utils/clock";
import type { EngineSettings, LocalEngine } from "@/utils/engines";
import type { GameHeaders } from "@/utils/treeReducer";

type TimeFields = {
    timeControl?: TimeControlField;
    timeUnit?: TimeType;
    incrementUnit?: TimeType;
};

export type HumanOpponent = TimeFields & { type: "human"; name?: string };

export type EngineOpponent = TimeFields & {
    type: "engine";
    engine: LocalEngine | null;
    go: GoMode;
    /** Options edited in the opponent form; the engine's own settings stay the defaults. */
    engineSettings?: EngineSettings;
};

export type PlayerSettings = HumanOpponent | EngineOpponent;

export function engineExtraOptions(player: EngineOpponent) {
    return (player.engineSettings ?? player.engine?.settings ?? [])
        .filter((s) => s.name !== "MultiPV")
        .map((s) => ({ name: s.name, value: s.value?.toString() ?? "" }));
}

/** A side without a clock gets no time (null), not 0 ms. */
export function engineGoMode(
    players: { white: PlayerSettings; black: PlayerSettings },
    turn: "white" | "black",
    whiteTime: number | null,
    blackTime: number | null,
): GoMode {
    const player = players[turn];
    if (!player.timeControl || player.type !== "engine") {
        return player.type === "engine" ? player.go : { t: "Infinite" };
    }
    return {
        t: "PlayersTime",
        c: {
            white: players.white.timeControl ? whiteTime : null,
            black: players.black.timeControl ? blackTime : null,
            winc: players.white.timeControl?.increment ?? 0,
            binc: players.black.timeControl?.increment ?? 0,
        },
    };
}

export function buildEngineGameHeaders(
    white: PlayerSettings,
    black: PlayerSettings,
    now: Date,
): Partial<GameHeaders> {
    const iso = now.toISOString();
    const engines = Number(white.type === "engine") + Number(black.type === "engine");
    return {
        event: ["Player Match", "Player vs Engine", "Engine Match"][engines],
        site: "Pawn Appétit",
        date: iso.slice(0, 10).replace(/-/g, "."),
        time: iso.slice(11, 19),
        white_elo: white.type === "engine" ? (white.engine?.elo ?? undefined) : undefined,
        black_elo: black.type === "engine" ? (black.engine?.elo ?? undefined) : undefined,
    };
}

/** Same-time-control mode: `to` gets `from`'s clock and the units it is shown in. */
export function copyTimeControl<T extends PlayerSettings>(from: PlayerSettings, to: T): T {
    return {
        ...to,
        timeControl: from.timeControl,
        timeUnit: from.timeUnit,
        incrementUnit: from.incrementUnit,
    };
}

/**
 * The move an engine game plays for an engine event: the engine's own `bestmove` of the final
 * payload (so strength limits take effect), else the first move of its top line. Only the final
 * payload (progress 100) counts; an empty result gives `undefined`.
 */
export function engineMoveFromPayload(payload: BestMovesPayload) {
    if (payload.progress < 100) return undefined;
    const uci = payload.bestmove ?? payload.bestLines[0]?.uciMoves[0];
    return uci ? parseUci(uci) : undefined;
}
