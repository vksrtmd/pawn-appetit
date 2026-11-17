import { describe, expect, test } from "vitest";
import {
    ENGINES,
    engineSchema,
    type LocalEngine,
    packageInstallArgs,
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

describe("package installs", () => {
    const linuxLc0 = ENGINES.filter(
        (e) => e.os === "linux" && e.name === "Leela Chess Zero" && e.installMethod === "package",
    );

    test("the Linux lc0 cards install with manager apt and package lc0", () => {
        expect(linuxLc0.length).toBeGreaterThan(0);
        for (const e of linuxLc0) expect(packageInstallArgs(e)).toEqual(["apt", "lc0"]);
    });

    test("an engine without a manager or package name cannot be installed", () => {
        expect(() => packageInstallArgs({ packageManager: "apt" })).toThrow(
            "Package manager or package name not found",
        );
        expect(() => packageInstallArgs({ packageName: "lc0" })).toThrow(
            "Package manager or package name not found",
        );
    });

    test("an engine stored with the old packageCommand field still parses", () => {
        const stored = {
            type: "local",
            name: "Leela",
            version: "0.30.0",
            path: "/usr/bin/lc0",
            packageCommand: "sudo apt-get install lc0",
        };
        expect(engineSchema.safeParse(stored).success).toBe(true);
    });
});
