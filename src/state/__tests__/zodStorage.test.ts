import { expect, test, vi } from "vitest";
import { z } from "zod";
import { createAsyncZodStorage, createZodStorage } from "../utils";

vi.mock("@tauri-apps/plugin-fs", () => ({
    BaseDirectory: {},
    readTextFile: vi.fn<() => void>(),
    writeTextFile: vi.fn<() => void>(),
    remove: vi.fn<() => void>(),
}));

const schema = z.object({ n: z.number() });

function memory(initial: Record<string, string>) {
    const data = { ...initial };
    return {
        data,
        getItem: (k: string) => data[k] ?? null,
        setItem: (k: string, v: string) => void (data[k] = v),
        removeItem: (k: string) => void delete data[k],
    };
}

test("a stored value failing validation reads as the default and stays untouched", () => {
    const store = memory({ k: '{"n":"x"}' });
    const storage = createZodStorage(schema, store);
    expect(storage.getItem("k", { n: 1 })).toEqual({ n: 1 });
    expect(store.data.k).toBe('{"n":"x"}');
});

test("an async stored value failing validation reads as the default and stays untouched", async () => {
    const data: Record<string, string> = { k: '{"n":"x"}' };
    const storage = createAsyncZodStorage(schema, {
        getItem: async (k) => data[k] ?? null,
        setItem: async (k, v) => void (data[k] = v),
        removeItem: async (k) => void delete data[k],
    });
    expect(await storage.getItem("k", { n: 1 })).toEqual({ n: 1 });
    expect(data.k).toBe('{"n":"x"}');
});

test("a failing async write surfaces to the caller", async () => {
    const storage = createAsyncZodStorage(schema, {
        getItem: async () => null,
        setItem: async () => {
            throw new Error("disk full");
        },
        removeItem: async () => {
            throw new Error("locked");
        },
    });
    await expect(storage.setItem("k", { n: 1 })).rejects.toThrow("disk full");
    await expect(storage.removeItem("k")).rejects.toThrow("locked");
});
