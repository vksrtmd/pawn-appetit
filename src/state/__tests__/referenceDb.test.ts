import { createStore } from "jotai";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
    getDbInfo: vi.fn<(file: string) => Promise<unknown>>(),
}));

vi.mock("@/bindings", () => ({
    commands: { getDbInfo: mocks.getDbInfo },
}));

import { forgetReferenceDbAtom, referenceDbAtom, storedReferenceDbAtom } from "../gameAtoms";

async function settled(store: ReturnType<typeof createStore>) {
    store.get(referenceDbAtom);
    await new Promise((resolve) => setTimeout(resolve, 0));
    return store.get(referenceDbAtom);
}

describe("reference database", () => {
    beforeEach(() => {
        localStorage.clear();
        mocks.getDbInfo.mockReset();
    });

    it("is the stored path while the file is still a readable database", async () => {
        mocks.getDbInfo.mockResolvedValue({ status: "ok", data: {} });
        const store = createStore();
        store.set(storedReferenceDbAtom, "/db/a.db3");
        expect(await settled(store)).toBe("/db/a.db3");
    });

    it("is cleared when the stored path no longer opens", async () => {
        mocks.getDbInfo.mockResolvedValue({ status: "error", error: "no such file" });
        const store = createStore();
        store.set(storedReferenceDbAtom, "/db/gone.db3");
        expect(await settled(store)).toBeNull();
    });

    it("is cleared when checking the file throws", async () => {
        mocks.getDbInfo.mockRejectedValue(new Error("boom"));
        const store = createStore();
        store.set(storedReferenceDbAtom, "/db/gone.db3");
        expect(await settled(store)).toBeNull();
    });

    it("is none when nothing is stored, without asking the backend", async () => {
        const store = createStore();
        expect(await settled(store)).toBeNull();
        expect(mocks.getDbInfo).not.toHaveBeenCalled();
    });

    it("is cleared when the reference database is deleted", async () => {
        mocks.getDbInfo.mockResolvedValue({ status: "ok", data: {} });
        const store = createStore();
        store.set(storedReferenceDbAtom, "/db/a.db3");
        store.set(forgetReferenceDbAtom, "/db/a.db3");
        expect(store.get(storedReferenceDbAtom)).toBeNull();
        expect(await settled(store)).toBeNull();
    });

    it("is kept when another database is deleted", () => {
        const store = createStore();
        store.set(storedReferenceDbAtom, "/db/a.db3");
        store.set(forgetReferenceDbAtom, "/db/b.db3");
        expect(store.get(storedReferenceDbAtom)).toBe("/db/a.db3");
    });
});
