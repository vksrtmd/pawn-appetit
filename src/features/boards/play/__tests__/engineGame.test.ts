import { expect, test } from "vitest";
import {
    buildEngineGameHeaders,
    copyTimeControl,
    engineGoMode,
    engineExtraOptions,
    type EngineOpponent,
    type PlayerSettings,
} from "../engineGame";

const engine = {
    type: "local" as const,
    name: "Stockfish",
    version: "18",
    path: "/sf",
    elo: 3600,
    settings: [{ name: "Threads", value: 1 }],
};

function enginePlayer(overrides: Partial<EngineOpponent> = {}): EngineOpponent {
    return { type: "engine", engine, go: { t: "Depth", c: 5 }, ...overrides };
}

test("engine request sends the options edited in the opponent form, not the engine defaults", () => {
    const player = enginePlayer({ engineSettings: [{ name: "Threads", value: 8 }] });
    expect(engineExtraOptions(player)).toEqual([{ name: "Threads", value: "8" }]);
});

test("engine request falls back to the engine defaults and drops MultiPV", () => {
    const player = enginePlayer({
        engine: {
            ...engine,
            settings: [
                { name: "MultiPV", value: 3 },
                { name: "Hash", value: 16 },
            ],
        },
    });
    expect(engineExtraOptions(player)).toEqual([{ name: "Hash", value: "16" }]);
});

test("engine without a clock searches with its go mode", () => {
    const human: PlayerSettings = { type: "human", timeControl: { seconds: 60000 } };
    const players = { white: human, black: enginePlayer() };
    expect(engineGoMode(players, "black", 60000, null)).toEqual({ t: "Depth", c: 5 });
});

test("engine with a clock against an unclocked human is not told the human has 0 ms", () => {
    const player = enginePlayer({ timeControl: { seconds: 60000, increment: 1000 } });
    const players = { white: { type: "human" as const }, black: player };
    expect(engineGoMode(players, "black", null, 59000)).toEqual({
        t: "PlayersTime",
        c: { white: null, black: 59000, winc: 0, binc: 1000 },
    });
});

test("PGN headers carry the engine Elo, Event, Site and Date", () => {
    const h = buildEngineGameHeaders(
        { type: "human", name: "Me" },
        enginePlayer(),
        new Date("2026-03-04T05:06:07Z"),
    );
    expect(h).toMatchObject({
        black_elo: 3600,
        event: "Player vs Engine",
        site: "Pawn Appétit",
        date: "2026.03.04",
        time: "05:06:07",
    });
    expect(h.white_elo).toBeUndefined();
});

test("same time control copies the time units along with the time", () => {
    const first: PlayerSettings = {
        type: "human",
        timeControl: { seconds: 5000, increment: 2000 },
        timeUnit: "s",
        incrementUnit: "ms",
    };
    expect(copyTimeControl(first, { type: "human", timeUnit: "m", incrementUnit: "s" })).toEqual({
        type: "human",
        timeControl: { seconds: 5000, increment: 2000 },
        timeUnit: "s",
        incrementUnit: "ms",
    });
});
