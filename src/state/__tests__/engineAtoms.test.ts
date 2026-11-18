import { INITIAL_FEN } from "chessops/fen";
import { expect, test } from "vitest";
import type { BestMoves } from "@/bindings";
import { swapMove } from "@/utils/chessops";
import { arrowLines } from "../engineAtoms";

const line = (cp: number, uci: string): BestMoves => ({
    depth: 1,
    multipv: 1,
    nodes: 1,
    score: { value: { type: "cp", value: cp }, wdl: null },
    nps: 1,
    sanMoves: [uci],
    uciMoves: [uci],
});

test("threat arrows keep the lines within 10 points of the best for the threatening side", () => {
    // After 1.e4 Black is to move; the threat position has White to move.
    const afterE4 = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
    const threatLines = [line(300, "d2d4"), line(280, "g1f3"), line(-200, "a2a3")];
    const engineMoves = new Map([[`${swapMove(afterE4)}:`, threatLines]]);

    const lines = arrowLines(engineMoves, INITIAL_FEN, ["e2e4"]);

    expect(lines?.map((l) => l.pv[0])).toEqual(["d2d4", "g1f3"]);
});
