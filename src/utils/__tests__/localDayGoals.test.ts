import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { useUserStatsStore } from "@/state/userStatsStore";
import { getAchievements } from "../achievements";
import { getDailyGoals } from "../dailyGoals";

vi.mock("../gameRecords", () => ({ countGamesOnDate: async () => 0 }));

beforeEach(() => {
    vi.stubEnv("TZ", "America/New_York");
    // 20:00 on 18 Nov in New York is already 19 Nov in UTC.
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-11-19T01:00:00Z"));
    localStorage.clear();
});

afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllEnvs();
    useUserStatsStore.setState((s) => ({ userStats: { ...s.userStats, completionDates: [] } }));
});

function setCompletions(completionDates: string[]) {
    useUserStatsStore.setState((s) => ({ userStats: { ...s.userStats, completionDates } }));
}

test("a practice finished at 18:00 New York counts toward the daily goal at 20:00", async () => {
    setCompletions(["2026-11-18T23:00:00.000Z"]);
    const goals = await getDailyGoals();
    expect(goals.find((g) => g.id === "l")?.current).toBe(1);
});

test("a practice finished at 18:00 New York earns the Student achievement at 20:00", async () => {
    setCompletions(["2026-11-18T23:00:00.000Z"]);
    expect((await getAchievements()).map((a) => a.id)).toContain("student");
});

test("a practice finished the evening before in UTC but after midnight locally is not today's", async () => {
    setCompletions(["2026-11-18T03:00:00.000Z"]); // 22:00 on 17 Nov in New York
    const goals = await getDailyGoals();
    expect(goals.find((g) => g.id === "l")?.current).toBe(0);
});

test("completing practice at 20:00 New York marks today once, even when an earlier mark exists", () => {
    setCompletions(["2026-11-18T23:00:00.000Z"]);
    useUserStatsStore.getState().setUserStats({ completedPractice: { a: ["1"] } });
    expect(useUserStatsStore.getState().userStats.completionDates).toEqual([
        "2026-11-18T23:00:00.000Z",
    ]);
});
