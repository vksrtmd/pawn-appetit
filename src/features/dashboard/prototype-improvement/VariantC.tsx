// PROTOTYPE variant C, "Tabs + master-detail": Today | Progress | Games, where Games is the
// current dashboard. Progress lists every metric on the left and shows one in detail on the right.
// Not enough games: a ring with n/20 replaces the number, and the detail says what's missing.
import {
  Badge,
  Button,
  Card,
  Group,
  NavLink,
  Progress,
  RingProgress,
  ScrollArea,
  Stack,
  Tabs,
  Text,
  Title,
} from "@mantine/core";
import { IconPlayerPlay } from "@tabler/icons-react";
import { useState } from "react";
import { type Data, MIN_GAMES, pct } from "./data";
import { ErrorRateChart } from "./ErrorRateChart";

type Item = {
  id: string;
  group: string;
  label: string;
  value: string | null;
  have: number;
  detail: React.ReactNode;
};

function Ring({ have }: { have: number }) {
  return (
    <RingProgress
      size={28}
      thickness={3}
      sections={[{ value: (100 * Math.min(have, MIN_GAMES)) / MIN_GAMES, color: "gray" }]}
      label={
        <Text size="8px" ta="center">
          {have}
        </Text>
      }
    />
  );
}

function items(d: Data): Item[] {
  const out: Item[] = [
    {
      id: "er",
      group: "Outcome",
      label: "Error Rate",
      value: d.errorRate ? `${d.errorRate.value}` : null,
      have: d.countedGames,
      detail: (
        <Stack>
          <Title order={3}>
            Error Rate {d.errorRate?.value ?? ""}{" "}
            <Text span size="sm" c="dimmed">
              opponents {d.opponents ?? "–"} · baseline {d.baseline?.value ?? "–"}
            </Text>
          </Title>
          <ErrorRateChart trend={d.trend} baseline={d.baseline?.value ?? null} height={300} />
        </Stack>
      ),
    },
  ];
  for (const f of d.facets)
    for (const v of f.values)
      out.push({
        id: `${f.facet}:${v.label}`,
        group: f.facet,
        label: v.label,
        value: v.rate ? `${v.rate.value}` : null,
        have: v.games,
        detail: v.rate ? (
          <Stack>
            <Title order={3}>
              {f.facet}: {v.label}
            </Title>
            <Text>
              {v.rate.value} ({v.rate.lo}–{v.rate.hi}) vs baseline {v.baseline ?? "–"} over{" "}
              {v.games} games
            </Text>
            <ErrorRateChart trend={d.trend} baseline={v.baseline} height={240} showLegend={false} />
          </Stack>
        ) : (
          <Text>
            {v.label}: {v.games} counted games so far; {MIN_GAMES - v.games} more reviewed games
            needed before a rate is shown.
          </Text>
        ),
      });
  for (const w of d.weaknesses)
    out.push({
      id: w.id,
      group: "Weaknesses",
      label: w.name,
      value: `${w.rate}`,
      have: MIN_GAMES,
      detail: (
        <Stack>
          <Title order={3}>{w.name}</Title>
          <Text>
            {w.facet}: {w.rate} now vs {w.baseline} baseline, {w.games} games, {w.cards} Cards
          </Text>
          <Group>
            <Button>Train this Weakness</Button>
            <Button variant="light">Show Mistake Positions</Button>
          </Group>
        </Stack>
      ),
    });
  for (const f of d.fixed)
    out.push({
      id: `fixed:${f.name}`,
      group: "Fixed",
      label: `✓ ${f.name}`,
      value: f.date.slice(5),
      have: MIN_GAMES,
      detail: (
        <Text>
          {f.name} stopped meeting the Weakness rule on {f.date}.
        </Text>
      ),
    });
  out.push(
    {
      id: "repeat",
      group: "Transfer",
      label: "Repeat-Error rate",
      value: d.repeatError ? pct(d.repeatError.rate) : null,
      have: d.repeatError?.n ?? 0,
      detail: <Text>Erring again at a position that already has a Card.</Text>,
    },
    {
      id: "adh",
      group: "Transfer",
      label: "Repertoire adherence",
      value: d.adherence ? pct(d.adherence.rate) : null,
      have: d.adherence?.games ?? d.countedGames,
      detail: <Text>Games where you stayed in your repertoire until the opponent left it.</Text>,
    },
  );
  for (const x of d.retention)
    out.push({
      id: `ret:${x.kind}`,
      group: "Card retention (target 90%)",
      label: x.kind,
      value: x.value === null ? null : pct(x.value),
      have: x.reviews,
      detail: <Text>{x.reviews} reviews with an interval of at least 1 day, last 30 days.</Text>,
    });
  for (const r of d.ratings)
    out.push({
      id: `rating:${r.account}${r.tc}`,
      group: "Rating (context)",
      label: `${r.account} ${r.tc}`,
      value: `${r.rating}`,
      have: MIN_GAMES,
      detail: (
        <Text>
          Rating over time per Account and time control (the existing RatingsPanel goes here).
        </Text>
      ),
    });
  return out;
}

export function VariantC({ d, legacy }: { d: Data; legacy: React.ReactNode }) {
  const list = items(d);
  const [sel, setSel] = useState("er");
  const current = list.find((i) => i.id === sel) ?? list[0];
  const s = d.session;
  let lastGroup = "";

  return (
    <Tabs defaultValue="today" p="md">
      <Tabs.List mb="md">
        <Tabs.Tab value="today">Today</Tabs.Tab>
        <Tabs.Tab value="progress">Progress</Tabs.Tab>
        <Tabs.Tab value="games">Games</Tabs.Tab>
      </Tabs.List>

      <Tabs.Panel value="today">
        <Group align="stretch" grow>
          <Card withBorder p="lg">
            <Title order={3}>Training Session</Title>
            <Text c="dimmed" size="sm">
              {s.doneMin} of {s.budgetMin} min
            </Text>
            <Stack gap="xs" my="md">
              {s.blocks.map((b) => (
                <Stack key={b.kind} gap={2}>
                  <Group justify="space-between">
                    <Text size="sm">{b.kind}</Text>
                    <Text size="xs" c="dimmed">
                      {b.done}/{b.due + b.new}
                      {b.note ? ` · ${b.note}` : ""}
                    </Text>
                  </Group>
                  <Progress value={(100 * b.done) / Math.max(1, b.due + b.new)} />
                </Stack>
              ))}
            </Stack>
            <Button leftSection={<IconPlayerPlay />}>{s.started ? "Continue" : "Start"}</Button>
          </Card>
          <Card withBorder p="lg">
            <Title order={3}>Since last week</Title>
            {d.errorRate ? (
              <Stack gap={4} mt="sm">
                <Text>
                  Error Rate {d.errorRate.value} (opponents {d.opponents}, baseline{" "}
                  {d.baseline?.value})
                </Text>
                {d.fixed[0] && <Text>✓ Fixed: {d.fixed[0].name}</Text>}
                {d.weaknesses
                  .filter((w) => w.direction === "worse")
                  .map((w) => (
                    <Text key={w.id} c="red">
                      ↑ {w.name}: {w.rate} vs {w.baseline}
                    </Text>
                  ))}
              </Stack>
            ) : (
              <Group mt="sm">
                <Ring have={d.countedGames} />
                <Text size="sm">Progress shows up after {MIN_GAMES} reviewed games.</Text>
              </Group>
            )}
            <Text size="xs" c="dimmed" mt="md">
              Game Review: {d.standard.current} of {d.standard.total} at the current standard ·
              queue {d.queue.pending}, {d.queue.state}
            </Text>
          </Card>
        </Group>
      </Tabs.Panel>

      <Tabs.Panel value="progress">
        <Group align="flex-start" wrap="nowrap">
          <ScrollArea h="70vh" w={300}>
            {list.map((i) => {
              const header = i.group !== lastGroup ? i.group : null;
              lastGroup = i.group;
              return (
                <div key={i.id}>
                  {header && (
                    <Text size="xs" c="dimmed" tt="uppercase" fw={700} mt="sm" mb={2}>
                      {header}
                    </Text>
                  )}
                  <NavLink
                    active={i.id === current.id}
                    label={i.label}
                    onClick={() => setSel(i.id)}
                    rightSection={
                      i.value === null ? (
                        <Ring have={i.have} />
                      ) : (
                        <Badge variant="light">{i.value}</Badge>
                      )
                    }
                  />
                </div>
              );
            })}
          </ScrollArea>
          <Card withBorder p="lg" style={{ flex: 1 }}>
            {current.value === null && current.group !== "Weaknesses" ? (
              <Stack>
                <Title order={3} c="dimmed">
                  {current.label}: not enough games
                </Title>
                {current.detail}
              </Stack>
            ) : (
              current.detail
            )}
          </Card>
        </Group>
      </Tabs.Panel>

      <Tabs.Panel value="games">{legacy}</Tabs.Panel>
    </Tabs>
  );
}
