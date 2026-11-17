import { atom, createStore } from "jotai";
import { describe, expect, it, vi } from "vitest";
import { loadable } from "../utils";

vi.mock("@tauri-apps/plugin-fs", () => ({ BaseDirectory: { AppData: 0 } }));

describe("loadable", () => {
    it("reports a sync value as data", () => {
        const store = createStore();
        expect(store.get(loadable(atom(1)))).toEqual({ state: "hasData", data: 1 });
    });

    it("reports loading, then data, for a promise", async () => {
        const store = createStore();
        let resolve!: (v: number) => void;
        const l = loadable(atom(new Promise<number>((r) => (resolve = r))));
        store.sub(l, () => {});
        expect(store.get(l)).toEqual({ state: "loading" });
        resolve(2);
        await vi.waitFor(() => expect(store.get(l)).toEqual({ state: "hasData", data: 2 }));
    });

    it("reports a rejected promise as an error", async () => {
        const store = createStore();
        const err = new Error("boom");
        const l = loadable(atom(Promise.reject(err)));
        store.sub(l, () => {});
        await vi.waitFor(() => expect(store.get(l)).toEqual({ state: "hasError", error: err }));
    });

    it("reports a throwing atom as an error", () => {
        const store = createStore();
        const err = new Error("sync");
        const l = loadable(
            atom(() => {
                throw err;
            }),
        );
        expect(store.get(l)).toEqual({ state: "hasError", error: err });
    });
});
