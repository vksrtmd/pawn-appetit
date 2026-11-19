import { exists, readDir, remove, rename, writeTextFile } from "@tauri-apps/plugin-fs";
import {
    type Directory,
    type FileMetadata,
    type FileType,
    infoPathFor,
    invalidateFileIndex,
} from "./file";

/** The new name is taken by another file. */
export class RenameExistsError extends Error {
    constructor(readonly path: string) {
        super(`${path} already exists`);
    }
}

/** The file was renamed but its `.info` sidecar could not be moved: `renamed` is the file now. */
export class RenameSidecarError extends Error {
    constructor(
        readonly renamed: FileMetadata,
        cause: unknown,
    ) {
        super(`${renamed.path} was renamed but its .info file was not updated`, { cause });
    }
}

/** Whether `newPath` is taken by a file other than `oldPath` (a case-only rename is not). */
async function isTaken(oldPath: string, newPath: string): Promise<boolean> {
    if (!(await exists(newPath))) return false;
    if (oldPath.toLowerCase() !== newPath.toLowerCase()) return true;
    // Same name but for case: on a case-insensitive file system that is the file itself, so only
    // an entry with exactly the new name in the listing is a second file.
    const dir = newPath.slice(0, Math.max(newPath.lastIndexOf("/"), newPath.lastIndexOf("\\")));
    const entries = await readDir(dir);
    return entries.some((e) => e.name === newPath.slice(dir.length + 1));
}

/**
 * Renames a PGN file and its `.info` sidecar. Refuses when the target exists
 * (`RenameExistsError`), and keeps the file's tags. When only the sidecar fails after the rename,
 * throws `RenameSidecarError` with the renamed file.
 */
export async function renameFile(
    file: FileMetadata,
    newName: string,
    type: FileType,
): Promise<FileMetadata> {
    // A replacer function: `$&` and `$\`` in the name are text, not replacement patterns
    const newPath = file.path.replace(/[^/\\]+\.pgn$/, () => `${newName}.pgn`);
    if (newPath !== file.path && (await isTaken(file.path, newPath))) {
        throw new RenameExistsError(newPath);
    }
    const metadata = { type, tags: file.metadata.tags };
    const renamed = { ...file, name: newName, path: newPath, metadata };
    if (newPath !== file.path) await rename(file.path, newPath);
    try {
        if (newPath !== file.path) {
            const oldInfo = infoPathFor(file.path);
            if (await exists(oldInfo)) await remove(oldInfo);
        }
        await writeTextFile(infoPathFor(newPath), JSON.stringify(metadata));
    } catch (e) {
        invalidateFileIndex();
        throw new RenameSidecarError(renamed, e);
    }
    invalidateFileIndex();
    return renamed;
}

/** Deletes a file with its `.info` sidecar, or a directory with everything in it. */
export async function deleteFile(path: string, isDirectory: boolean): Promise<void> {
    if (isDirectory) {
        await remove(path, { recursive: true });
    } else {
        await remove(path);
        const info = infoPathFor(path);
        if (await exists(info)) await remove(info);
    }
    invalidateFileIndex();
}

/** The listing without the entry at `path` (also inside directories). */
export function removeEntry(
    entries: (FileMetadata | Directory)[],
    path: string,
): (FileMetadata | Directory)[] {
    return entries
        .filter((e) => e.path !== path)
        .map((e) =>
            e.type === "directory" ? { ...e, children: removeEntry(e.children, path) } : e,
        );
}
