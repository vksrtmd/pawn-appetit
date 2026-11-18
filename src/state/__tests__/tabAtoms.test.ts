import { atom, getDefaultStore } from "jotai";
import { expect, test } from "vitest";
import { disposeTabAtoms, tabAtomFamily } from "../tabAtoms";

test("dispose removes every registered per-tab atom of the tab and no other", () => {
    const byId = tabAtomFamily((_tab: string) => atom(0));
    const byObject = tabAtomFamily((_: { tab: string; n: number }) => atom(0), {
        tabOf: (p) => p.tab,
        areEqual: (a, b) => a.tab === b.tab && a.n === b.n,
    });
    const store = getDefaultStore();
    store.set(byId("x"), 1);
    store.set(byId("y"), 1);
    store.set(byObject({ tab: "x", n: 1 }), 1);
    store.set(byObject({ tab: "x", n: 2 }), 1);
    store.set(byObject({ tab: "y", n: 1 }), 1);

    disposeTabAtoms("x");

    expect([...byId.getParams()]).toEqual(["y"]);
    expect([...byObject.getParams()]).toEqual([{ tab: "y", n: 1 }]);
});

const raw = import.meta.glob<string>(
    ["/src/**/*.{ts,tsx}", "!/src/**/*.test.*", "!/src/**/__tests__/**"],
    {
        query: "?raw",
        import: "default",
        eager: true,
    },
);
const src = "/src/";
const files = Object.keys(raw);
const read = (f: string) => raw[f] ?? "";

test("only the registry and the non-per-tab families call atomFamily", () => {
    const allowed = new Map([
        ["state/tabAtoms.ts", 1],
        ["state/engineAtoms.ts", 2], // scoreTypeFamily (per engine), bestMovesFamily (per position)
        ["state/gameAtoms.ts", 1], // deckAtomFamily (per file)
    ]);
    const found = files
        .map((f) => [f.slice(src.length), read(f).match(/\batomFamily\(/g)])
        .filter(([, m]) => m)
        .map(([f, m]) => [f, (m as RegExpMatchArray).length]);
    expect(new Map(found as [string, number][])).toEqual(allowed);
});

test("no code outside the Tab session reads or writes a tab's session-storage keys", () => {
    const owners = ["state/tabSession.ts", "utils/tabStateStorage.ts", "state/uiAtoms.ts"];
    const offenders = files
        .filter((f) => !owners.some((o) => f.endsWith(o)))
        .filter((f) =>
            /sessionStorage\.(get|set|remove)Item\(\s*(`\$\{|[a-zA-Z]*[tT]abId|id\b|tab\.value|value\b|prev\.value)/.test(
                read(f),
            ),
        )
        .map((f) => f.slice(src.length));
    expect(offenders).toEqual([]);
});

test("the persisted tree envelope is built only by the Tab session", () => {
    const offenders = files
        .filter((f) => /version:\s*\w+,\s*state:/.test(read(f)))
        .map((f) => f.slice(src.length));
    expect(offenders).toEqual(["state/tabSession.ts"]);
});
