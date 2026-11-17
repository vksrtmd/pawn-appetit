import { describe, expect, test } from "vitest";
import { defaultGoMode, resolveReportEngine, shouldShowAccuracy } from "../reportSettings";

describe("Game Review limit type", () => {
    test("switching to Depth gives depth 20", () => {
        expect(defaultGoMode("Depth")).toEqual({ t: "Depth", c: 20 });
    });

    test("switching to Time gives 500 ms", () => {
        expect(defaultGoMode("Time")).toEqual({ t: "Time", c: 500 });
    });

    test("switching to Nodes gives 1,000,000 nodes", () => {
        expect(defaultGoMode("Nodes")).toEqual({ t: "Nodes", c: 1000000 });
    });
});

describe("Game Review engine selection", () => {
    const a = { path: "/engines/a" };
    const b = { path: "/engines/b" };

    test("keeps the selected engine while it is installed", () => {
        expect(resolveReportEngine([a, b], "/engines/b")).toBe("/engines/b");
    });

    test("falls back to the first engine when the selected one is gone, even if the list length is unchanged", () => {
        expect(resolveReportEngine([a, { path: "/engines/c" }], "/engines/b")).toBe("/engines/a");
    });

    test("picks the first engine when none was selected", () => {
        expect(resolveReportEngine([a, b], "")).toBe("/engines/a");
    });

    test("is empty without engines", () => {
        expect(resolveReportEngine([], "/engines/a")).toBe("");
    });
});

describe("Game Review accuracy block", () => {
    test("is hidden when an accuracy is 0, so no literal 0 renders", () => {
        expect(shouldShowAccuracy({ whiteAccuracy: 0, blackAccuracy: 80 })).toBe(false);
        expect(shouldShowAccuracy({ whiteAccuracy: 80, blackAccuracy: 0 })).toBe(false);
    });

    test("is shown when both accuracies are positive", () => {
        expect(shouldShowAccuracy({ whiteAccuracy: 90, blackAccuracy: 80 })).toBe(true);
    });
});
