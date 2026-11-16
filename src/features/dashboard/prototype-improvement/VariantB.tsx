// PROTOTYPE variant B, "Report": one long page with no drill-downs, placed above the current
// dashboard, which stays below it. Facets are drawn as interval plots (dot, Wilson band,
// baseline tick). Not enough games: a dashed placeholder at the same spot, with a count.
import {
  Badge,
  Button,
  Card,
  Divider,
  Group,
  Progress,
  SimpleGrid,
  Stack,
  Text,
  Title,
} from "@mantine/core";
import { type Data, MIN_GAMES, pct } from "./data";
import { ErrorRateChart } from "./ErrorRateChart";

const MAX = 8; // x-axis upper bound for interval plots, Errors per 100 moves

function Placeholder({
  have,
  h = 18,
  what = "games",
}: {
  have: number;
  h?: number;
  what?: string;
}) {
  return (
    <div
      style={{
        height: h,
        border: "1px dashed var(--mantine-color-gray-6)",
        borderRadius: 4,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        fontSize: 11,
        color: "var(--mantine-color-dimmed)",
      }}
    >
      {have} of {MIN_GAMES} {what}
    </div>
  );
}

function Interval({
  v,
  lo,
  hi,
  baseline,
}: {
  v: number;
  lo: number;
  hi: number;
  baseline: number | null;
}) {
  const x = (n: number) => `${(100 * n) / MAX}%`;
  return (
    <div style={{ position: "relative", height: 18 }}>
      <div
        style={{
          position: "absolute",
          top: 8,
          left: 0,
          right: 0,
          height: 1,
          background: "var(--mantine-color-gray-7)",
        }}
      />
      <div
        style={{
          position: "absolute",
          top: 4,
          height: 10,
          left: x(lo),
          width: `${(100 * (hi - lo)) / MAX}%`,
          background: "var(--mantine-color-blue-5)",
          opacity: 0.3,
          borderRadius: 3,
        }}
      />
      <div
        style={{
          position: "absolute",
          top: 3,
          left: `calc(${x(v)} - 6px)`,
          width: 12,
          height: 12,
          borderRadius: 6,
          background: "var(--mantine-color-blue-5)",
        }}
      />
      {baseline !== null && (
        <div
          style={{
            position: "absolute",
            top: 0,
            height: 18,
            width: 2,
            left: x(baseline),
            background: "var(--mantine-color-orange-5)",
          }}
        />
      )}
    </div>
  );
}

function Spark({ series, color }: { series: number[]; color: string }) {
  const min = Math.min(...series);
  const max = Math.max(...series);
  const pts = series
    .map((y, i) => `${(i * 60) / (series.length - 1)},${18 - ((y - min) / (max - min || 1)) * 16}`)
    .join(" ");
  return (
    <svg width={60} height={20}>
      <polyline points={pts} fill="none" stroke={color} strokeWidth={1.5} />
    </svg>
  );
}

export function VariantB({ d, legacy }: { d: Data; legacy: React.ReactNode }) {
  const s = d.session;
  const planned = s.blocks.reduce((a, b) => a + b.due + b.new, 0);
  const done = s.blocks.reduce((a, b) => a + b.done, 0);
  return (
    <Stack p="md" gap="lg">
      <Card withBorder p="sm">
        <Group justify="space-between">
          <Group gap="md">
            <Text fw={700}>Training Session</Text>
            <Text size="sm">
              {s.doneMin}/{s.budgetMin} min · {done}/{planned}
            </Text>
            <Progress value={(100 * s.doneMin) / s.budgetMin} w={200} />
          </Group>
          <Button size="xs">{s.started ? "Continue" : "Start"}</Button>
        </Group>
      </Card>

      <Stack gap={4}>
        <Group justify="space-between" align="flex-end">
          <Stack gap={0}>
            <Text size="xs" c="dimmed" tt="uppercase" fw={700}>
              Error Rate · Mistakes + Blunders per 100 moves · rolling 100 games
            </Text>
            {d.errorRate ? (
              <Group gap="lg" align="baseline">
                <Title order={1}>{d.errorRate.value}</Title>
                <Text size="sm">
                  opponents {d.opponents} · baseline {d.baseline?.value} ({d.baseline?.games} games
                  before your first Session) · Blunders {d.blunderRate?.value}
                </Text>
              </Group>
            ) : (
              <Title order={1} c="dimmed">
                –
              </Title>
            )}
          </Stack>
          <Text size="xs" c="dimmed">
            {d.standard.current} of {d.standard.total} games at the current standard · queue{" "}
            {d.queue.pending}, {d.queue.state}
          </Text>
        </Group>
        {d.trend.length >= 2 ? (
          <ErrorRateChart trend={d.trend} baseline={d.baseline?.value ?? null} height={240} />
        ) : (
          <Placeholder
            have={d.countedGames}
            h={240}
            what="reviewed games; the trend starts at 20"
          />
        )}
      </Stack>

      <SimpleGrid cols={{ base: 1, md: 2 }} spacing="lg">
        {d.facets.map((f) => (
          <Stack key={f.facet} gap={6}>
            <Text fw={700} size="sm">
              {f.facet}
            </Text>
            {f.values.map((v) => (
              <Group key={v.label} wrap="nowrap" gap="sm">
                <Text size="xs" w={130} truncate>
                  {v.label}
                </Text>
                <div style={{ flex: 1 }}>
                  {v.rate ? (
                    <Interval
                      v={v.rate.value}
                      lo={v.rate.lo}
                      hi={v.rate.hi}
                      baseline={v.baseline}
                    />
                  ) : (
                    <Placeholder have={v.games} />
                  )}
                </div>
                <Text size="xs" w={40} ta="right">
                  {v.rate?.value ?? ""}
                </Text>
              </Group>
            ))}
          </Stack>
        ))}
      </SimpleGrid>

      <Stack gap={6}>
        <Text fw={700} size="sm">
          Weaknesses (now vs baseline)
        </Text>
        {d.weaknesses.length === 0 && (
          <Text size="sm" c="dimmed">
            None yet. A Weakness needs 5 games with the same facet value and a rate above your
            overall one.
          </Text>
        )}
        {d.weaknesses.map((w) => (
          <Group key={w.id} wrap="nowrap">
            <Text size="sm" w={220}>
              {w.name}
            </Text>
            <Text size="xs" c="dimmed" w={110}>
              {w.facet}
            </Text>
            <Spark
              series={w.series}
              color={
                w.direction === "better" ? "teal" : w.direction === "worse" ? "tomato" : "gray"
              }
            />
            <Text size="sm" w={110}>
              {w.rate} vs {w.baseline}
            </Text>
            <Text size="xs" c="dimmed">
              {w.games} games · {w.cards} Cards
            </Text>
          </Group>
        ))}
        {d.fixed.length > 0 && (
          <Group gap="xs">
            <Text size="sm" fw={600}>
              Fixed:
            </Text>
            {d.fixed.map((f) => (
              <Badge key={f.name} color="teal" variant="light">
                {f.name} · {f.date}
              </Badge>
            ))}
          </Group>
        )}
      </Stack>

      <SimpleGrid cols={{ base: 2, md: 4 }}>
        <Stack gap={0}>
          <Text size="xs" c="dimmed">
            Repeat-Error rate
          </Text>
          <Text fw={700}>
            {d.repeatError ? `${pct(d.repeatError.rate)} (${d.repeatError.n})` : "–"}
          </Text>
        </Stack>
        <Stack gap={0}>
          <Text size="xs" c="dimmed">
            Repertoire adherence
          </Text>
          <Text fw={700}>{d.adherence ? pct(d.adherence.rate) : "–"}</Text>
        </Stack>
        <Stack gap={0}>
          <Text size="xs" c="dimmed">
            Retention (target 90%)
          </Text>
          <Text fw={700}>
            {d.retention.map((x) => (x.value === null ? "–" : pct(x.value))).join(" · ")}
          </Text>
        </Stack>
        <Stack gap={0}>
          <Text size="xs" c="dimmed">
            Effort, 30 days
          </Text>
          <Text fw={700}>
            {d.effort.days} d · {d.effort.minutes} min · {d.effort.cards} Cards
          </Text>
        </Stack>
      </SimpleGrid>

      <Group gap="xl">
        {d.ratings.map((x) => (
          <Group key={x.account + x.tc} gap={6}>
            <Text size="xs" c="dimmed">
              {x.account} {x.tc}
            </Text>
            <Text size="sm" fw={600}>
              {x.rating}
            </Text>
            {x.series.length > 2 && <Spark series={x.series} color="gray" />}
          </Group>
        ))}
      </Group>

      <Divider label="Current dashboard" />
      {legacy}
    </Stack>
  );
}
