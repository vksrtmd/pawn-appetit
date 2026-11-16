// PROTOTYPE host: swaps the dashboard's rendering by `?variant=` (Current, A, B, C) and the stub
// data by `?scenario=`. `legacy` is the current dashboard's JSX, untouched.
import { useNavigate, useSearch } from "@tanstack/react-router";
import { PrototypeSwitcher } from "@/components/PrototypeSwitcher";
import { getData, SCENARIOS, type Scenario } from "./data";
import { VariantA } from "./VariantA";
import { VariantB } from "./VariantB";
import { VariantC } from "./VariantC";

const VARIANTS = [
  { key: "A", name: "Cockpit: replaces, drill-down drawers" },
  { key: "B", name: "Report: long page above current" },
  { key: "C", name: "Tabs: Today | Progress | Games" },
  { key: "Current", name: "today's dashboard" },
];

export function ImprovementDashboardPrototype({ legacy }: { legacy: React.ReactNode }) {
  const search = useSearch({ from: "/" });
  const navigate = useNavigate({ from: "/" });
  if (!import.meta.env.DEV) return legacy;

  const variant = search.variant ?? "A";
  const scenario: Scenario = search.scenario ?? "established";
  const d = getData(scenario);
  const set = (next: { variant?: string; scenario?: Scenario }) =>
    navigate({ search: (prev) => ({ ...prev, ...next }), replace: true });

  return (
    <>
      {variant === "A" && <VariantA d={d} />}
      {variant === "B" && <VariantB d={d} legacy={legacy} />}
      {variant === "C" && <VariantC d={d} legacy={legacy} />}
      {variant === "Current" && legacy}
      <div style={{ height: 64 }} />
      <PrototypeSwitcher
        variants={VARIANTS}
        current={variant}
        onChange={(v) => set({ variant: v })}
        extra={{
          value: scenario,
          options: SCENARIOS,
          onChange: (v) => set({ scenario: v as Scenario }),
        }}
      />
    </>
  );
}
