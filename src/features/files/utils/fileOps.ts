import { exists, remove, rename, writeTextFile } from "@tauri-apps/plugin-fs";
import {
    type Directory,
    type FileMetadata,
    type FileType,
    infoPathFor,
    invalidateFileIndex,
} from "./file";

/**
 * Renames a PGN file and its `.info` sidecar. Refuses when the target exists, and keeps the
 * file's tags.
 */
export async function renameFile(
    file: FileMetadata,
    newName: string,
    type: FileType,
): Promise<FileMetadata> {
    const newPath = file.path.replace(/[^/\\]+\.pgn$/, `${newName}.pgn`);
    if (newPath !== file.path && (await exists(newPath))) {
        throw new Error(`${newPath} already exists`);
    }
    const metadata = { type, tags: file.metadata.tags };
    if (newPath !== file.path) {
        await rename(file.path, newPath);
        const oldInfo = infoPathFor(file.path);
        if (await exists(oldInfo)) await remove(oldInfo);
    }
    await writeTextFile(infoPathFor(newPath), JSON.stringify(metadata));
    invalidateFileIndex();
    return { ...file, name: newName, path: newPath, metadata };
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
