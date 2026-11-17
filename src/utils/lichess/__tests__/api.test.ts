import { beforeEach, describe, expect, test, vi } from "vitest";
import { downloadLichess } from "../api";

const mocks = vi.hoisted(() => ({
    downloadFile:
        vi.fn<
            (
                id: string,
                url: string,
                path: string,
                token: string | null,
                body: null,
                size: number | null,
            ) => Promise<{ status: "ok"; data: null }>
        >(),
}));

vi.mock("@/bindings", () => ({ commands: { downloadFile: mocks.downloadFile } }));
vi.mock("@tauri-apps/api/path", () => ({
    appDataDir: vi.fn<() => Promise<string>>(async () => "/data"),
    resolve: vi.fn<(...parts: string[]) => Promise<string>>(async (...parts) => parts.join("/")),
}));

beforeEach(() => {
    mocks.downloadFile.mockReset().mockResolvedValue({ status: "ok", data: null });
});

describe("downloadLichess", () => {
    test("requests games oldest first", async () => {
        await downloadLichess("alice", null, 10, () => {});

        const url = new URL(mocks.downloadFile.mock.calls[0][1]);
        expect(url.searchParams.get("sort")).toBe("dateAsc");
    });

    test("requests games oldest first when resuming from a timestamp", async () => {
        await downloadLichess("alice", 1700000000000, 10, () => {});

        const url = new URL(mocks.downloadFile.mock.calls[0][1]);
        expect([url.searchParams.get("sort"), url.searchParams.get("since")]).toEqual([
            "dateAsc",
            "1700000000000",
        ]);
    });
});
