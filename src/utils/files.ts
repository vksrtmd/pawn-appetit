import { Result } from "@badrap/result";
import { useQuery } from "@tanstack/react-query";
import { BaseDirectory, basename, extname, resolve, tempDir } from "@tauri-apps/api/path";
import { exists, mkdir, readTextFile, writeTextFile } from "@tauri-apps/plugin-fs";
import { platform } from "@tauri-apps/plugin-os";
import { defaultGame, makePgn } from "chessops/pgn";
import { commands } from "@/bindings";
import { infoPathFor, invalidateFileIndex, type FileMetadata } from "@/features/files/utils/file";
import { unwrap } from "@/utils/unwrap";
import { parsePGN } from "./chess";
import { notifyPgnParseError } from "./pgnParseError";
import { recordRecentFile } from "./recentFiles";
import type { Tab } from "./tabSchema";
import { createTab } from "./tabs";
import { getGameName } from "./treeReducer";

export function usePlatform() {
    const r = useQuery({
        queryKey: ["os"],
        queryFn: async () => {
            return platform();
        },
        staleTime: Infinity,
    });
    return { os: r.data, ...r };
}

export async function getFileNameWithoutExtension(filePath: string): Promise<string> {
    const fileNameWithExtension = await basename(filePath);
    const extension = await extname(filePath);
    return fileNameWithExtension.replace(`.${extension}`, "");
}

/** Current time as whole epoch seconds, the unit the Files page and file index use. */
export function nowEpochSeconds(): number {
    return Math.floor(Date.now() / 1000);
}

export async function openFile(
    file: string,
    setTabs: React.Dispatch<React.SetStateAction<Tab[]>>,
    setActiveTab: React.Dispatch<React.SetStateAction<string | null>>,
): Promise<boolean> {
    const count = unwrap(await commands.countPgnGames(file));
    const firstGame = unwrap(await commands.readGames(file, 0, 0))[0];
    if (!firstGame) {
        throw new Error("PGN file does not contain any games");
    }

    const fileName = await getFileNameWithoutExtension(file);

    const metadataPath = file.replace(/\.pgn$/i, ".info");
    let fileType: "game" | "repertoire" | "tournament" | "puzzle" | "variants" | "other" = "game";
    try {
        if (await exists(metadataPath)) {
            const metadata = JSON.parse(await readTextFile(metadataPath));
            if (metadata.type) {
                fileType = metadata.type;
            }
        }
    } catch {}

    const fileInfo: FileMetadata = {
        type: "file",
        metadata: {
            tags: [],
            type: fileType,
        },
        name: fileName,
        path: file,
        numGames: count,
        lastModified: nowEpochSeconds(),
    };

    // Parse only the first game for session storage
    // For variants files, parse as normal PGN (with variations) but display in variants view
    // Don't use isVariantsMode for parsing - that's only for special PGNs where all sequences are variations
    let firstGameTree: Awaited<ReturnType<typeof parsePGN>>;
    try {
        firstGameTree = await parsePGN(firstGame);
    } catch (e) {
        // Nothing is opened: no tab, no truncated tree
        if (notifyPgnParseError(e)) return false;
        throw e;
    }

    const id = await createTab({
        tab: {
            name: getGameName(firstGameTree?.headers) || "Multiple Games",
            type: "analysis",
        },
        setTabs,
        setActiveTab,
        srcInfo: fileInfo,
        gameNumber: 0,
        tree: firstGameTree,
    });
    return id !== null;
}

export async function openFileAndRemember(
    file: string,
    setTabs: React.Dispatch<React.SetStateAction<Tab[]>>,
    setActiveTab: React.Dispatch<React.SetStateAction<string | null>>,
) {
    if (await openFile(file, setTabs, setActiveTab)) recordRecentFile(file);
}

export async function createFile({
    filename,
    filetype,
    pgn,
    dir,
}: {
    filename: string;
    filetype: "game" | "repertoire" | "tournament" | "puzzle" | "variants" | "other";
    pgn?: string;
    dir: string;
}): Promise<Result<FileMetadata>> {
    const file = await resolve(dir, `${filename}.pgn`);
    if (await exists(file)) {
        return Result.err(Error("File already exists"));
    }
    const metadata = {
        type: filetype,
        tags: [],
    };
    await mkdir(dir, { recursive: true });
    await writeTextFile(file, pgn || makePgn(defaultGame()));
    await writeTextFile(infoPathFor(file), JSON.stringify(metadata));

    const numGames = unwrap(await commands.countPgnGames(file));
    invalidateFileIndex(dir);

    return Result.ok({
        type: "file",
        name: filename,
        path: file,
        numGames,
        metadata,
        lastModified: nowEpochSeconds(),
    });
}

export async function createTempImportFile(pgn: string): Promise<FileMetadata> {
    const tempDirPath = await resolve(await tempDir(), "pawn-appetit");
    const tempFilePath = await resolve(tempDirPath, `temp_import_${Date.now()}.pgn`);

    // Ensure temp directory exists
    try {
        await mkdir("pawn-appetit", { baseDir: BaseDirectory.Temp });
    } catch {
        // Directory might already exist, continue
    }

    await writeTextFile(tempFilePath, pgn);

    const numGames = unwrap(await commands.countPgnGames(tempFilePath));

    return {
        type: "file",
        name: "Untitled",
        path: tempFilePath,
        numGames,
        metadata: {
            type: "game",
            tags: [],
        },
        lastModified: nowEpochSeconds(),
    };
}

export function isTempImportFile(filePath: string): boolean {
    return filePath.includes("temp_import_");
}
