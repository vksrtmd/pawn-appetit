import { makeFen } from "chessops/fen";
import { positionFromFen } from "./chessops";

/** The fifty-move rule: 100 halfmoves without a capture or pawn move. */
export function isFiftyMoveRule(fen: string): boolean {
    const [pos] = positionFromFen(fen);
    return pos !== null && pos.halfmoves >= 100;
}

// Board, side to move, castling rights and legal en passant only. Chessops'
// toSetup drops an en passant square that no pawn can legally capture on.
function repetitionKey(fen: string): string {
    const [pos] = positionFromFen(fen);
    const normalized = pos ? makeFen(pos.toSetup()) : fen;
    return normalized.split(" ").slice(0, 4).join(" ");
}

/** True when `fen` already occurred twice in `previousFens`, so it is the third time. */
export function isThreefoldRepetition(previousFens: string[], fen: string): boolean {
    const key = repetitionKey(fen);
    return previousFens.filter((f) => repetitionKey(f) === key).length >= 2;
}
