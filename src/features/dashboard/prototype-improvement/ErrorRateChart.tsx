// PROTOTYPE: rolling Error Rate with Wilson band, opponents' Error Rate and frozen baseline.
import {
  Area,
  CartesianGrid,
  ComposedChart,
  Legend,
  Line,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import type { TrendPoint } from "./data";

export function ErrorRateChart({
  trend,
  baseline,
  height = 220,
  showLegend = true,
}: {
  trend: TrendPoint[];
  baseline: number | null;
  height?: number;
  showLegend?: boolean;
}) {
  return (
    <ResponsiveContainer width="100%" height={height}>
      <ComposedChart data={trend} margin={{ top: 8, right: 16, bottom: 0, left: -16 }}>
        <CartesianGrid strokeDasharray="3 3" opacity={0.2} />
        <XAxis dataKey="date" fontSize={11} />
        <YAxis domain={[0, 6]} fontSize={11} />
        <Tooltip />
        {showLegend && <Legend wrapperStyle={{ fontSize: 11 }} />}
        <Area
          dataKey="band"
          name="95% band"
          stroke="none"
          fill="var(--mantine-color-blue-5)"
          fillOpacity={0.2}
          isAnimationActive={false}
        />
        <Line
          dataKey="player"
          name="You"
          stroke="var(--mantine-color-blue-5)"
          strokeWidth={2}
          dot={false}
        />
        <Line
          dataKey="opponents"
          name="Opponents (same games)"
          stroke="var(--mantine-color-gray-5)"
          strokeDasharray="4 4"
          dot={false}
        />
        {baseline !== null && (
          <ReferenceLine
            y={baseline}
            stroke="var(--mantine-color-orange-5)"
            strokeDasharray="2 2"
            label={{
              value: `Baseline ${baseline}`,
              fontSize: 11,
              fill: "var(--mantine-color-orange-5)",
              position: "insideTopRight",
            }}
          />
        )}
      </ComposedChart>
    </ResponsiveContainer>
  );
}
