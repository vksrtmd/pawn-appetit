import { type Dispatch, type SetStateAction, useEffect, useRef } from "react";
import type { GameState } from "@/state/atoms";
import type { Outcome } from "@/bindings";
import type { PlayerSettings } from "./engineGame";

export const CLOCK_UPDATE_INTERVAL = 100; // ms

type Side = "white" | "black";

/**
 * Runs the clocks of a game in progress. Time is taken from the real time elapsed since the
 * last tick or move, not from counting ticks (webviews slow timers in hidden windows). The
 * side that has just moved gets its increment, from the first move on.
 */
export function useClockTimer(
    gameState: GameState,
    pos: { turn: Side } | null,
    whiteTime: number | null,
    blackTime: number | null,
    setWhiteTime: Dispatch<SetStateAction<number | null>>,
    setBlackTime: Dispatch<SetStateAction<number | null>>,
    players: { white: PlayerSettings; black: PlayerSettings },
    setGameState: (state: GameState) => void,
    setResult: (result: Outcome) => void,
) {
    const turn = pos?.turn ?? null;
    const turnRef = useRef<Side | null>(turn);
    const lastRef = useRef<number | null>(null);
    const playersRef = useRef(players);
    playersRef.current = players;

    const charge = (side: Side, now: number, increment: number) => {
        const elapsed = lastRef.current === null ? 0 : now - lastRef.current;
        const set = side === "white" ? setWhiteTime : setBlackTime;
        set((prev) => (prev === null ? prev : Math.max(0, prev - elapsed) + increment));
    };

    // A move: the side that moved pays the time it took and gets its increment.
    useEffect(() => {
        const moved = turnRef.current;
        turnRef.current = turn;
        if (gameState !== "playing" || !moved || !turn || moved === turn) return;
        const now = Date.now();
        charge(moved, now, playersRef.current[moved].timeControl?.increment ?? 0);
        lastRef.current = now;
        // oxlint-disable-next-line react-hooks/exhaustive-deps -- runs on a turn change only; charge reads refs
    }, [turn, gameState]);

    useEffect(() => {
        if (gameState !== "playing") {
            lastRef.current = null;
            return;
        }
        lastRef.current = Date.now();
        const id = setInterval(() => {
            const side = turnRef.current;
            if (!side) return;
            const now = Date.now();
            charge(side, now, 0);
            lastRef.current = now;
        }, CLOCK_UPDATE_INTERVAL);
        return () => clearInterval(id);
        // oxlint-disable-next-line react-hooks/exhaustive-deps -- one timer per game; charge reads refs
    }, [gameState]);

    useEffect(() => {
        if (gameState === "playing") {
            if (whiteTime !== null && whiteTime <= 0) {
                setGameState("gameOver");
                setResult("0-1");
            } else if (blackTime !== null && blackTime <= 0) {
                setGameState("gameOver");
                setResult("1-0");
            }
        }
    }, [gameState, whiteTime, blackTime, setGameState, setResult]);
}
