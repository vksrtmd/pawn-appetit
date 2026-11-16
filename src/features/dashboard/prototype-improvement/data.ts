// PROTOTYPE (#34, Improvement dashboard UX). Throwaway stub data, no persistence.
// The numbers are invented; they follow the shapes fixed in #28 (Daily training plan)
// and #29 (Progress metrics), not real queries.

export type Scenario = "established" | "new" | "rereview";
export const SCENARIOS: { key: Scenario; label: string }[] = [
    { key: "established", label: "Established (1310 games)" },
    { key: "new", label: "New Player (12 games)" },
    { key: "rereview", label: "Re-review running" },
];

export const MIN_GAMES = 20; // #29 rule 12

export type Rate = { value: number; lo: number; hi: number; games: number };
export type TrendPoint = {
    date: string;
    player: number;
    band: [number, number];
    opponents: number;
};
export type FacetValue = {
    label: string;
    rate: Rate | null;
    games: number;
    baseline: number | null;
};
export type Facet = { facet: string; values: FacetValue[] };
export type Weakness = {
    id: string;
    name: string;
    facet: string;
    rate: number;
    baseline: number;
    games: number;
    cards: number;
    direction: "better" | "flat" | "worse";
    series: number[];
};
export type Fixed = { name: string; facet: string; date: string };
export type SessionBlock = { kind: string; due: number; new: number; done: number; note?: string };
export type Data = {
    scenario: Scenario;
    countedGames: number;
    session: { budgetMin: number; doneMin: number; started: boolean; blocks: SessionBlock[] };
    errorRate: Rate | null;
    blunderRate: Rate | null;
    opponents: number | null;
    baseline: { value: number; games: number } | null;
    trend: TrendPoint[];
    facets: Facet[];
    weaknesses: Weakness[];
    fixed: Fixed[];
    repeatError: { rate: number; n: number } | null;
    adherence: { rate: number; games: number } | null;
    retention: { kind: string; value: number | null; reviews: number }[];
    effort: { days: number; minutes: number; cards: number };
    ratings: { account: string; tc: string; rating: number; delta: number; series: number[] }[];
    standard: { current: number; total: number; label: string };
    queue: { pending: number; state: "idle" | "running" | "paused (on battery)" };
};

function trend(n: number, start: number, end: number): TrendPoint[] {
    const out: TrendPoint[] = [];
    const d0 = new Date("2026-05-01").getTime();
    for (let i = 0; i < n; i++) {
        const t = i / Math.max(1, n - 1);
        const wobble = Math.sin(i * 1.7) * 0.15;
        const v = +(start + (end - start) * t + wobble).toFixed(2);
        out.push({
            date: new Date(d0 + i * 7 * 86400000).toISOString().slice(5, 10),
            player: v,
            band: [+(v - 0.45).toFixed(2), +(v + 0.45).toFixed(2)],
            opponents: +(3.7 + Math.cos(i) * 0.12).toFixed(2),
        });
    }
    return out;
}

const r = (value: number, games: number, w = 0.5): Rate => ({
    value,
    lo: +(value - w).toFixed(2),
    hi: +(value + w).toFixed(2),
    games,
});

const established: Data = {
    scenario: "established",
    countedGames: 1240,
    session: {
        budgetMin: 20,
        doneMin: 7,
        started: true,
        blocks: [
            { kind: "Repertoire", due: 18, new: 6, done: 24 },
            { kind: "Mistake Positions", due: 11, new: 10, done: 3 },
            { kind: "Puzzles", due: 0, new: 8, done: 0, note: "Missed fork (Weakness 1 of 3)" },
        ],
    },
    errorRate: r(3.1, 100, 0.4),
    blunderRate: r(0.9, 100, 0.2),
    opponents: 3.7,
    baseline: { value: 3.8, games: 300 },
    trend: trend(26, 3.9, 3.1),
    facets: [
        {
            facet: "Phase",
            values: [
                { label: "Opening", rate: r(1.2, 100), games: 100, baseline: 1.9 },
                { label: "Middlegame", rate: r(3.9, 100), games: 100, baseline: 4.4 },
                { label: "Endgame", rate: r(4.6, 61, 0.9), games: 61, baseline: 4.5 },
            ],
        },
        {
            facet: "Eval context",
            values: [
                { label: "Converting (≥75%)", rate: r(3.4, 48, 0.8), games: 48, baseline: 4.1 },
                { label: "Balanced", rate: r(2.8, 100), games: 100, baseline: 3.4 },
                { label: "Defending (≤25%)", rate: r(4.9, 39, 1.0), games: 39, baseline: 5.0 },
            ],
        },
        {
            facet: "Time control",
            values: [
                { label: "Blitz", rate: r(3.4, 71, 0.6), games: 71, baseline: 4.0 },
                { label: "Rapid", rate: r(2.4, 26, 0.9), games: 26, baseline: 3.1 },
                { label: "Classical", rate: null, games: 3, baseline: 2.6 },
            ],
        },
        {
            facet: "Opening (ECO)",
            values: [
                { label: "B90 Najdorf", rate: r(2.9, 22, 1.1), games: 22, baseline: 4.2 },
                { label: "C42 Petroff", rate: null, games: 14, baseline: 2.0 },
                { label: "D37 QGD", rate: r(1.8, 31, 0.8), games: 31, baseline: 2.2 },
            ],
        },
    ],
    weaknesses: [
        {
            id: "fork",
            name: "Missed fork",
            facet: "Motif missed",
            rate: 0.42,
            baseline: 0.61,
            games: 23,
            cards: 31,
            direction: "better",
            series: [0.62, 0.6, 0.55, 0.5, 0.47, 0.42],
        },
        {
            id: "conv",
            name: "Converting (≥75%)",
            facet: "Eval context",
            rate: 3.4,
            baseline: 4.1,
            games: 48,
            cards: 40,
            direction: "better",
            series: [4.2, 4.0, 3.9, 3.7, 3.5, 3.4],
        },
        {
            id: "rook",
            name: "Rook endgames",
            facet: "Endgame type",
            rate: 5.2,
            baseline: 5.0,
            games: 19,
            cards: 17,
            direction: "flat",
            series: [5.0, 5.3, 4.9, 5.1, 5.4, 5.2],
        },
        {
            id: "pin",
            name: "Allowed pin",
            facet: "Motif allowed",
            rate: 0.31,
            baseline: 0.22,
            games: 11,
            cards: 12,
            direction: "worse",
            series: [0.2, 0.22, 0.25, 0.27, 0.3, 0.31],
        },
        {
            id: "time",
            name: "Time trouble (<10% clock)",
            facet: "Time trouble",
            rate: 7.8,
            baseline: 8.6,
            games: 34,
            cards: 29,
            direction: "flat",
            series: [8.7, 8.4, 8.2, 8.0, 7.9, 7.8],
        },
    ],
    fixed: [
        { name: "Missed skewer", facet: "Motif missed", date: "2026-10-02" },
        { name: "B12 Caro-Kann Advance", facet: "Opening", date: "2026-09-14" },
    ],
    repeatError: { rate: 0.18, n: 57 },
    adherence: { rate: 0.83, games: 100 },
    retention: [
        { kind: "Repertoire", value: 0.92, reviews: 640 },
        { kind: "Mistake Positions", value: 0.81, reviews: 210 },
        { kind: "Puzzles", value: 0.86, reviews: 74 },
    ],
    effort: { days: 22, minutes: 431, cards: 1388 },
    ratings: [
        {
            account: "Lichess · vksrtmd",
            tc: "Blitz",
            rating: 2214,
            delta: 31,
            series: [2170, 2181, 2178, 2195, 2201, 2214],
        },
        {
            account: "Lichess · vksrtmd",
            tc: "Rapid",
            rating: 2287,
            delta: 12,
            series: [2270, 2275, 2268, 2281, 2280, 2287],
        },
        {
            account: "Chess.com · vksrtmd",
            tc: "Blitz",
            rating: 2041,
            delta: -8,
            series: [2050, 2061, 2039, 2044, 2049, 2041],
        },
    ],
    standard: { current: 1240, total: 1310, label: "SF 17.1 · 1M / 10M nodes" },
    queue: { pending: 70, state: "running" },
};

const fresh: Data = {
    scenario: "new",
    countedGames: 12,
    session: {
        budgetMin: 20,
        doneMin: 0,
        started: false,
        blocks: [
            { kind: "Repertoire", due: 0, new: 10, done: 0 },
            { kind: "Mistake Positions", due: 0, new: 10, done: 0 },
            {
                kind: "Puzzles",
                due: 0,
                new: 6,
                done: 0,
                note: "All themes (no motif Weakness yet)",
            },
        ],
    },
    errorRate: null,
    blunderRate: null,
    opponents: null,
    baseline: null,
    trend: [],
    facets: established.facets.map((f) => ({
        facet: f.facet,
        values: f.values.map((v) => ({
            ...v,
            rate: null,
            games: Math.min(v.games, 12) % 13,
            baseline: null,
        })),
    })),
    weaknesses: [],
    fixed: [],
    repeatError: null,
    adherence: null,
    retention: [
        { kind: "Repertoire", value: null, reviews: 0 },
        { kind: "Mistake Positions", value: null, reviews: 0 },
        { kind: "Puzzles", value: null, reviews: 0 },
    ],
    effort: { days: 0, minutes: 0, cards: 0 },
    ratings: established.ratings.map((x) => ({ ...x, series: x.series.slice(-2) })),
    standard: { current: 12, total: 412, label: "SF 17.1 · 1M / 10M nodes" },
    queue: { pending: 400, state: "running" },
};

const rereview: Data = {
    ...established,
    scenario: "rereview",
    countedGames: 96,
    errorRate: r(3.2, 96, 0.5),
    trend: trend(26, 3.9, 3.1).slice(-4),
    facets: established.facets.map((f) => ({
        ...f,
        values: f.values.map((v) =>
            v.games * 0.3 < MIN_GAMES ? { ...v, rate: null, games: Math.round(v.games * 0.3) } : v,
        ),
    })),
    standard: { current: 96, total: 1310, label: "SF 18 · 1M / 10M nodes (new, opted in)" },
    queue: { pending: 1214, state: "paused (on battery)" },
};

export function getData(s: Scenario): Data {
    return s === "new" ? fresh : s === "rereview" ? rereview : established;
}

export const pct = (x: number) => `${Math.round(x * 100)}%`;
