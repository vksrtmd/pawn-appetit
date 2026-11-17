import { fetch } from "@tauri-apps/plugin-http";
import { expect, test, vi } from "vitest";
import { getBestMoves } from "@/utils/chessdb/api";

vi.mock("@tauri-apps/plugin-http", () => ({ fetch: vi.fn<typeof fetch>() }));

test("the querypv request asks for stable results", async () => {
    const urls: string[] = [];
    vi.mocked(fetch).mockImplementation((async (url: string) => {
        urls.push(url);
        const body = url.includes("action=queryall")
            ? {
                  status: "ok",
                  moves: [{ uci: "e2e4", san: "e4", score: 20, rank: 1, note: "" }],
              }
            : { status: "nobestmove" };
        return { json: async () => body };
    }) as never);

    await getBestMoves("tab", { t: "Depth", c: 20 } as never, {
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        moves: [],
        extraOptions: [],
    });

    const querypv = urls.find((u) => u.includes("action=querypv"));
    expect(querypv).toBeDefined();
    expect(new URL(querypv!).searchParams.get("stable")).toBe("1");
});
