import { notifications } from "@mantine/notifications";
import i18n from "i18next";

/** A PGN move the parser can't read: the game is refused, not cut short there. */
export class PgnParseError extends Error {
    constructor(
        readonly san: string,
        readonly ply: number,
    ) {
        super(`pgnParse: illegal move "${san}" at ply ${ply}`);
    }
}

/** Shows the error to the user. Returns false when `error` is not a PgnParseError. */
export function notifyPgnParseError(error: unknown): boolean {
    if (!(error instanceof PgnParseError)) return false;
    notifications.show({
        color: "red",
        message: i18n.t("features.pgnParseIllegalMove", { san: error.san, ply: error.ply }),
    });
    return true;
}
