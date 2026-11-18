import { z } from "zod";
import { fileMetadataSchema } from "@/features/files/utils/file";

const dbGameMetadataSchema = z.object({
    type: z.literal("db"),
    db: z.string(),
    id: z.number(),
});
export type DbGameMetadata = z.infer<typeof dbGameMetadataSchema>;

const entitySourceMetadataSchema = z.union([fileMetadataSchema, dbGameMetadataSchema]);

export type EntitySourceMetadata = z.infer<typeof entitySourceMetadataSchema>;

export const tabSchema = z.object({
    name: z.string(),
    value: z.string(),
    type: z.enum(["new", "play", "analysis", "puzzles"]),
    gameNumber: z.number().nullish(),
    source: entitySourceMetadataSchema.nullish(),
    meta: z
        .object({
            timeControl: z.object({
                seconds: z.number(),
                increment: z.number(),
            }),
        })
        .optional(),
});

export type Tab = z.infer<typeof tabSchema>;

export function genID() {
    function S4() {
        return (((1 + Math.random()) * 0x10000) | 0).toString(16).substring(1);
    }
    return S4() + S4();
}
