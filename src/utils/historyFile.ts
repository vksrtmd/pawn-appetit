import { appDataDir, resolve } from "@tauri-apps/api/path";
import { exists, readTextFile, rename, writeTextFile } from "@tauri-apps/plugin-fs";

const queues = new Map<string, Promise<unknown>>();

async function historyPath(filename: string) {
    return resolve(await appDataDir(), filename);
}

/**
 * Reads a JSON history file. Only a missing file counts as empty (`fallback`). A file that does
 * not parse is moved aside to `<name>.corrupt-<time>` and the read throws, so no later save
 * overwrites it.
 */
async function readHistory<T>(file: string, fallback: T): Promise<T> {
    if (!(await exists(file))) return fallback;
    const text = await readTextFile(file);
    try {
        return JSON.parse(text) as T;
    } catch (error) {
        await rename(file, `${file}.corrupt-${Date.now()}`);
        throw new Error(`${file} is not valid JSON; moved aside`, { cause: error });
    }
}

export async function readHistoryFile<T>(filename: string, fallback: T): Promise<T> {
    return readHistory(await historyPath(filename), fallback);
}

/**
 * Read-modify-write of a history file. Updates to the same file run one after another and the
 * result is written to a temp file, then renamed over the original. `update` returns the new
 * content, or `undefined` to leave the file as is.
 */
export async function updateHistoryFile<T>(
    filename: string,
    fallback: T,
    update: (current: T) => T | undefined,
): Promise<boolean> {
    const file = await historyPath(filename);
    const run = async () => {
        const next = update(await readHistory(file, fallback));
        if (next === undefined) return false;
        const tmp = `${file}.tmp`;
        await writeTextFile(tmp, JSON.stringify(next));
        await rename(tmp, file);
        return true;
    };
    const result = (queues.get(file) ?? Promise.resolve()).then(run, run);
    queues.set(
        file,
        result.catch(() => undefined),
    );
    return result;
}
