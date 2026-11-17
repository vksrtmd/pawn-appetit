import type { GoMode } from "@/bindings";

export type ReportGoMode = Extract<GoMode, { t: "Depth" | "Time" | "Nodes" }>;

/** The limit a Game Review starts from when its limit type is switched to `type`. */
export function defaultGoMode(type: ReportGoMode["t"]): ReportGoMode {
    switch (type) {
        case "Depth":
            return { t: "Depth", c: 20 };
        case "Time":
            return { t: "Time", c: 500 };
        case "Nodes":
            return { t: "Nodes", c: 1000000 };
    }
}

/** The engine path to select: `current` while still installed, else the first engine, else "". */
export function resolveReportEngine(engines: { path: string }[], current: string): string {
    if (engines.length === 0) return "";
    return engines.some((e) => e.path === current) ? current : engines[0].path;
}

/** An accuracy of 0 means "no scored moves", so the block is hidden rather than shown as 0. */
export function shouldShowAccuracy(stats: { whiteAccuracy: number; blackAccuracy: number }) {
    return stats.whiteAccuracy > 0 && stats.blackAccuracy > 0;
}
