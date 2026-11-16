import { createFileRoute, redirect } from "@tanstack/react-router";
import { getDefaultStore } from "jotai";
import { z } from "zod";
import DashboardPage from "@/features/dashboard/DashboardPage";
import { showDashboardOnStartupAtom } from "@/state/atoms";

// PROTOTYPE: variant/scenario drive the improvement dashboard prototype switcher.
const searchSchema = z.object({
  variant: z.string().optional(),
  scenario: z.enum(["established", "new", "rereview"]).optional(),
});

export const Route = createFileRoute("/")({
  component: DashboardPage,
  validateSearch: searchSchema,
  loader: ({ context: { loadDirs } }) => loadDirs(),
  beforeLoad: () => {
    const store = getDefaultStore();
    const show = store.get(showDashboardOnStartupAtom);
    if (!show) {
      throw redirect({ to: "/boards" });
    }
    return null;
  },
});
