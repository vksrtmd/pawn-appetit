import { useUserStatsStore } from "@/state/userStatsStore";
import { countGamesOnDate } from "./gameRecords";
import { dateKey, getTodayPuzzleCount } from "./puzzleStreak";

export type DailyGoal = {
    id: string;
    label: string;
    current: number;
    total: number;
};

export async function getDailyGoals(): Promise<DailyGoal[]> {
    const todayPuzzles = getTodayPuzzleCount();

    const { userStats } = useUserStatsStore.getState();
    const today = dateKey();
    const todayPracticeCompletions = (userStats.completionDates || []).filter(
        (d: string) => dateKey(new Date(d)) === today,
    ).length;

    const gamesToday = await countGamesOnDate();

    const goals: DailyGoal[] = [
        { id: "g", label: "Play 2 games", current: gamesToday, total: 2 },
        { id: "p", label: "Solve 10 puzzles", current: todayPuzzles, total: 10 },
        {
            id: "l",
            label: "Finish 1 practice",
            current: todayPracticeCompletions > 0 ? 1 : 0,
            total: 1,
        },
    ];
    return goals;
}
