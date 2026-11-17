import { describe, expect, test } from "vitest";
import {
    type LocalEngine,
    joinSyzygyPaths,
    syncSelectedEngine,
    threadOptions,
} from "@/utils/engines";

const engine = (id: string, name: string) =>
    ({ id, name, path: `/e/${id}` }) as unknown as LocalEngine;

describe("syncSelectedEngine", () => {
    test("returns the refreshed engine when its settings were edited", () => {
        const old = engine("a", "Old name");
        const edited = engine("a", "New name");
        expect(syncSelectedEngine([edited, engine("b", "B")], old)).toBe(edited);
    });

    test("selects the first engine when none is selected", () => {
        const first = engine("a", "A");
        expect(syncSelectedEngine([first], null)).toBe(first);
    });

    test("changes nothing when the selected engine is unchanged or gone", () => {
        const a = engine("a", "A");
        expect(syncSelectedEngine([a], a)).toBeUndefined();
        expect(syncSelectedEngine([engine("b", "B")], a)).toBeUndefined();
        expect(syncSelectedEngine([], null)).toBeUndefined();
    });
});

describe("threadOptions", () => {
    test("offers powers of two up to the core count", () => {
        expect(threadOptions(1, 8)).toEqual([1, 2, 4, 8]);
    });

    test("includes a stored non-power-of-two value in order", () => {
        expect(threadOptions(6, 8)).toEqual([1, 2, 4, 6, 8]);
    });

    test("does not include a spurious option for a non-power-of-two core count", () => {
        expect(threadOptions(1, 12)).toEqual([1, 2, 4, 8]);
    });
});

describe("joinSyzygyPaths", () => {
    test("joins with ; on windows and : elsewhere", () => {
        expect(joinSyzygyPaths(["C:\\a", "C:\\b"], "windows")).toBe("C:\\a;C:\\b");
        expect(joinSyzygyPaths(["/a", "/b"], "linux")).toBe("/a:/b");
        expect(joinSyzygyPaths(["/a"], "macos")).toBe("/a");
    });
});
