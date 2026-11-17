import { beforeEach, expect, test, vi } from "vitest";
import type { Token } from "@/bindings";

const mocks = vi.hoisted(() => ({ lexPgn: vi.fn<() => Promise<unknown>>() }));

vi.mock("@/bindings", async (importOriginal) => ({
    ...(await importOriginal<typeof import("@/bindings")>()),
    commands: { lexPgn: mocks.lexPgn },
}));

import { getPGN, parsePGN } from "@/utils/chess";

const tokens: Token[] = [
    { type: "San", value: "e4" },
    { type: "Comment", value: "[%clk 0:00:00]" },
    { type: "San", value: "e5" },
    { type: "Comment", value: "[%clk 0:10:00]" },
];

beforeEach(() => {
    mocks.lexPgn.mockResolvedValue({ status: "ok", data: tokens });
});

test("a [%clk 0:00:00] annotation is kept when parsing and written back on save", async () => {
    const tree = await parsePGN("1. e4 {[%clk 0:00:00]} e5 {[%clk 0:10:00]}");
    const e4 = tree.root.children[0];
    expect(e4.clock).toBe(0);
    expect(e4.children[0].clock).toBe(600);
    expect(
        getPGN(tree.root, {
            headers: null,
            glyphs: true,
            comments: true,
            variations: true,
            extraMarkups: true,
        }),
    ).toContain("[%clk 0:00:00]");
});
