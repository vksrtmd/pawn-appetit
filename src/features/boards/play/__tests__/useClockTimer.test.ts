import { act, renderHook } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { PlayerSettings } from "../engineGame";
import { useClockTimer } from "../useClockTimer";

const human: PlayerSettings = {
    type: "human",
    timeControl: { seconds: 180_000, increment: 2_000 },
};

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

function setup() {
    return renderHook(
        ({ turn }: { turn: "white" | "black" }) => {
            const [white, setWhite] = useState<number | null>(180_000);
            const [black, setBlack] = useState<number | null>(180_000);
            useClockTimer(
                "playing",
                { turn },
                white,
                black,
                setWhite,
                setBlack,
                { white: human, black: human },
                () => {},
                () => {},
            );
            return { white, black };
        },
        { initialProps: { turn: "white" as "white" | "black" } },
    );
}

test("in a 3+2 game both players have 3:02 after their first moves", () => {
    const { result, rerender } = setup();

    rerender({ turn: "black" });
    rerender({ turn: "white" });

    expect(result.current).toEqual({ white: 182_000, black: 182_000 });
});

test("remaining time comes from elapsed real time, not from counted ticks", () => {
    const { result } = setup();

    // A hidden window runs one timer callback 5 s (4.9 s + the 100 ms tick) later: all of it is charged.
    act(() => {
        vi.setSystemTime(Date.now() + 4_900);
        vi.advanceTimersToNextTimer();
    });

    expect(result.current.white).toBe(175_000);
    expect(result.current.black).toBe(180_000);
});
