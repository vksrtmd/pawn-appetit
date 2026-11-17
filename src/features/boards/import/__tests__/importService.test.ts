import { beforeEach, describe, expect, test, vi } from "vitest";
import { importGameContent } from "../importService";

const mocks = vi.hoisted(() => ({
    getLichessGame: vi.fn<(id: string) => Promise<string>>(),
    getChesscomGame: vi.fn<(url: string) => Promise<string | null>>(),
}));

vi.mock("@/utils/lichess/api", () => ({ getLichessGame: mocks.getLichessGame }));
vi.mock("@/utils/chess.com/api", () => ({ getChesscomGame: mocks.getChesscomGame }));
vi.mock("@/utils/chess", () => ({
    parsePGN: vi.fn<() => Promise<unknown>>(async () => ({ headers: {} })),
}));
vi.mock("@/features/files/components/PgnSourceInput", () => ({
    resolvePgnTarget: vi.fn<() => void>(),
}));
vi.mock("@/utils/files", () => ({
    createFile: vi.fn<() => void>(),
    createTempImportFile: vi.fn<() => void>(),
    openFile: vi.fn<() => void>(),
}));
vi.mock("@/utils/pgnUtils", () => ({ parseMultiplePgnGames: vi.fn<() => void>() }));
vi.mock("@/utils/treeReducer", () => ({
    defaultTree: vi.fn<() => void>(),
    getGameName: vi.fn<() => string>(() => "game"),
}));
vi.mock("@tauri-apps/api/path", () => ({ basename: vi.fn<() => void>() }));

const context = {
    setTabs: vi.fn<() => void>(),
    setActiveTab: vi.fn<() => void>(),
    setCurrentTab: vi.fn<() => void>(),
};

beforeEach(() => {
    mocks.getLichessGame.mockReset().mockResolvedValue("1. e4 *");
    mocks.getChesscomGame.mockReset().mockResolvedValue("1. e4 *");
});

function importLink(link: string) {
    return importGameContent({ source: "Link", link, analysisTitle: "" }, context);
}

describe("importing a Lichess game URL", () => {
    test.each([
        ["https://lichess.org/abcd1234", "abcd1234"],
        ["https://lichess.org/abcd1234/black?x=1", "abcd1234"],
        ["https://lichess.org/abcd1234?x=1#12", "abcd1234"],
        ["https://lichess.org/game/export/abcd1234", "abcd1234"],
        ["lichess.org/abcd1234", "abcd1234"],
        ["https://lichess.org/abcd1234efgh/white", "abcd1234efgh"],
    ])("%s requests game %s", async (link, id) => {
        const result = await importLink(link);

        expect(mocks.getLichessGame).toHaveBeenCalledWith(id);
        expect(result.successCount).toBe(1);
    });
});

describe("importing a Chess.com game URL", () => {
    test.each(["https://www.chess.com/game/123", "https://www.chess.com/game/live/123?foo=1"])(
        "%s is passed to the Chess.com importer",
        async (link) => {
            const result = await importLink(link);

            expect(mocks.getChesscomGame).toHaveBeenCalledWith(link);
            expect(result.successCount).toBe(1);
        },
    );
});
