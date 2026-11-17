import { expect, test, vi } from "vitest";

vi.mock("@/bindings", async (orig) => ({
    ...(await orig<typeof import("@/bindings")>()),
    commands: {
        lexPgn: async () => ({
            status: "ok",
            data: [
                { type: "San", value: "e4" },
                { type: "Comment", value: "Good [%clk 0:01:00] [%timestamp 12] move" },
                { type: "San", value: "e5" },
                { type: "Comment", value: "[%timestamp 7]" },
            ],
        }),
    },
}));

import { parsePGN } from "../chess";

test("[%timestamp N] does not appear in the displayed comment", async () => {
    const tree = await parsePGN("1. e4");
    const first = tree.root.children[0];
    expect(first.comment).toBe("Good move");
    expect(first.clock).toBe(60);
    expect(first.children[0].comment).toBe("");
});
