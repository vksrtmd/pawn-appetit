import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
    deleteDbGame: vi.fn<(file: string, id: number) => Promise<unknown>>(),
}));

vi.mock("@/bindings", () => ({
    commands: { deleteDbGame: mocks.deleteDbGame },
}));

import { deleteGameAndRefresh } from "../db";

function cachedTables() {
    const client = new QueryClient();
    for (const key of ["players", "tournaments", "games"]) {
        client.setQueryData([key, {}, "/db/a.db3"], []);
    }
    return client;
}

const isStale = (client: QueryClient, key: string) =>
    client.getQueryState([key, {}, "/db/a.db3"])?.isInvalidated;

describe("deleteGameAndRefresh", () => {
    it("marks the player and tournament tables stale after a game is deleted", async () => {
        mocks.deleteDbGame.mockResolvedValue({ status: "ok", data: null });
        const client = cachedTables();

        await deleteGameAndRefresh(client, "/db/a.db3", 7);

        expect(mocks.deleteDbGame).toHaveBeenCalledWith("/db/a.db3", 7);
        expect(isStale(client, "players")).toBe(true);
        expect(isStale(client, "tournaments")).toBe(true);
        expect(isStale(client, "games")).toBe(false);
    });

    it("leaves the tables alone when the delete failed", async () => {
        mocks.deleteDbGame.mockResolvedValue({ status: "error", error: "locked" });
        const client = cachedTables();

        await deleteGameAndRefresh(client, "/db/a.db3", 7);

        expect(isStale(client, "players")).toBe(false);
    });
});
