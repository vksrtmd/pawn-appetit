import { describe, expect, it } from "vitest";
import { isFiftyMoveRule, isThreefoldRepetition } from "../drawRules";

const fen = (placement: string, turn: string, castling: string, ep: string, half = 0, full = 1) =>
    `${placement} ${turn} ${castling} ${ep} ${half} ${full}`;
const KINGS = "4k3/8/8/8/8/8/8/4K3";

describe("isFiftyMoveRule", () => {
    it("draws when the halfmove clock reaches 100", () => {
        expect(isFiftyMoveRule(fen(KINGS, "w", "-", "-", 100, 80))).toBe(true);
    });

    it("does not draw at 99 halfmoves, e.g. after a capture on move 49 reset the count", () => {
        expect(isFiftyMoveRule(fen(KINGS, "w", "-", "-", 99, 80))).toBe(false);
        expect(isFiftyMoveRule(fen(KINGS, "b", "-", "-", 1, 80))).toBe(false);
    });

    it("does not draw on an unparseable fen", () => {
        expect(isFiftyMoveRule("garbage")).toBe(false);
    });
});

describe("isThreefoldRepetition", () => {
    const start = fen(KINGS, "w", "-", "-", 0);
    const other = fen("4k3/8/8/8/8/8/8/3K4", "b", "-", "-", 1);

    it("detects the third occurrence regardless of move counters", () => {
        const history = [start, other, fen(KINGS, "w", "-", "-", 4, 3), other];
        expect(isThreefoldRepetition(history, fen(KINGS, "w", "-", "-", 8, 5))).toBe(true);
    });

    it("does not detect a second occurrence", () => {
        expect(isThreefoldRepetition([start, other], start)).toBe(false);
    });

    it("ignores an en passant square that cannot be captured", () => {
        // 1.e4 pushed twice-equivalent: e3 is set but black has no pawn to capture.
        const placement = "4k3/8/8/8/4P3/8/8/4K3";
        const withEp = fen(placement, "b", "-", "e3");
        const withoutEp = fen(placement, "b", "-", "-");
        expect(isThreefoldRepetition([withEp, other, withoutEp, other], withEp)).toBe(true);
    });

    it("distinguishes positions where en passant is legal in only one of them", () => {
        const placement = "4k3/8/8/8/3pP3/8/8/4K3";
        const legalEp = fen(placement, "b", "-", "e3");
        const noEp = fen(placement, "b", "-", "-");
        expect(isThreefoldRepetition([legalEp, other, legalEp, other], noEp)).toBe(false);
        expect(isThreefoldRepetition([noEp, other, noEp, other], legalEp)).toBe(false);
    });

    it("distinguishes castling rights", () => {
        const placement = "r3k3/8/8/8/8/8/8/R3K3";
        const rights = fen(placement, "w", "KQkq", "-");
        const none = fen(placement, "w", "-", "-");
        expect(isThreefoldRepetition([rights, other, rights, other], none)).toBe(false);
    });
});
