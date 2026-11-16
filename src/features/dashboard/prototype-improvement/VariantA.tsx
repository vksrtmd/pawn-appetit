// PROTOTYPE variant A, "Cockpit": replaces the dashboard. First screen = today's Training
// Session + 4 headline tiles; every metric detail sits in a drawer behind its tile.
// Not enough games: the tile greys out and shows progress toward the 20-game minimum.
import {
  Badge,
  Button,
  Card,
  Drawer,
  Group,
  Progress,
  SimpleGrid,
  Stack,
  Table,
  Text,
  Title,
  UnstyledButton,
} from "@mantine/core";
import { IconPlayerPlay } from "@tabler/icons-react";
import { useState } from "react";
import { type Data, MIN_GAMES, pct } from "./data";
import { ErrorRateChart } from "./ErrorRateChart";

type Panel = "error" | "weak" | "training" | "standard" | null;

function Tile({
  title,
  children,
  locked,
  onClick,
}: {
  title: string;
  children: React.ReactNode;
  locked?: { have: number };
  onClick: () => void;
}) {
  return (
    <UnstyledButton onClick={onClick}>
      <Card withBorder p="md" h="100%" style={{ opacity: locked ? 0.6 : 1 }}>
        <Text size="xs" c="dimmed" tt="uppercase" fw={700}>
          {title}
        </Text>
        {locked ? (
          <Stack gap={4} mt="sm">
            <Text fw={600}>Not enough games</Text>
            <Progress value={(locked.have / MIN_GAMES) * 100} size="sm" />
            <Text size="xs" c="dimmed">
              {locked.have} of {MIN_GAMES} reviewed games; shows up at {MIN_GAMES}
            </Text>
          </Stack>
        ) : (
          children
        )}
      </Card>
    </UnstyledButton>
  );
}

export function VariantA({ d }: { d: Data }) {
  const [panel, setPanel] = useState<Panel>(null);
  const s = d.session;
  const planned = s.blocks.reduce((a, b) => a + b.due + b.new, 0);
  const done = s.blocks.reduce((a, b) => a + b.done, 0);
  const top = d.weaknesses[0];
  const locked = d.errorRate ? undefined : { have: d.countedGames };

  return (
    <Stack p="md" gap="md">
      <Card withBorder p="lg">
        <Group justify="space-between" align="flex-start">
          <Stack gap={4}>
            <Text size="xs" c="dimmed" tt="uppercase" fw={700}>
              Today's Training Session
            </Text>
            <Title order={2}>
              {s.doneMin} of {s.budgetMin} min
            </Title>
            <Text size="sm" c="dimmed">
              {done} of {planned} Cards and Puzzles
            </Text>
          </Stack>
          <Button size="lg" leftSection={<IconPlayerPlay />}>
            {s.started ? "Continue" : "Start"}
          </Button>
        </Group>
        <Progress.Root size="xl" mt="md">
          {s.blocks.map((b, i) => (
            <Progress.Section
              key={b.kind}
              value={(100 * (b.due + b.new)) / planned}
              color={["blue", "grape", "teal"][i]}
              style={{ opacity: b.done >= b.due + b.new ? 1 : 0.5 }}
            >
              <Progress.Label>{b.kind}</Progress.Label>
            </Progress.Section>
          ))}
        </Progress.Root>
        <Group mt="xs" gap="lg">
          {s.blocks.map((b) => (
            <Text key={b.kind} size="xs" c="dimmed">
              {b.kind}: {b.due} due · {b.new} new{b.note ? ` · ${b.note}` : ""}
            </Text>
          ))}
        </Group>
      </Card>

      <SimpleGrid cols={{ base: 1, sm: 2, lg: 4 }}>
        <Tile title="Error Rate" locked={locked} onClick={() => setPanel("error")}>
          <Title order={2} mt="xs">
            {d.errorRate?.value}
          </Title>
          <Text size="xs" c="dimmed">
            per 100 moves · last 100 games
          </Text>
          <Text size="sm" mt={4}>
            Opponents {d.opponents} · Baseline {d.baseline?.value}
          </Text>
        </Tile>
        <Tile title="Top Weakness" onClick={() => setPanel("weak")}>
          {top ? (
            <>
              <Text fw={700} mt="xs">
                {top.name}
              </Text>
              <Text size="sm">
                {top.rate} now vs {top.baseline} baseline
              </Text>
              <Text size="xs" c="dimmed">
                {d.weaknesses.length} Weaknesses · {d.fixed.length} Fixed
              </Text>
            </>
          ) : (
            <Text size="sm" mt="xs" c="dimmed">
              None found yet. A Weakness needs 5 games with the same facet.
            </Text>
          )}
        </Tile>
        <Tile title="Training" onClick={() => setPanel("training")}>
          <Text size="sm" mt="xs">
            {d.effort.days} days · {d.effort.minutes} min · {d.effort.cards} Cards (30 days)
          </Text>
          <Text size="sm">
            Retention {d.retention.map((x) => (x.value === null ? "–" : pct(x.value))).join(" / ")}
          </Text>
          <Text size="xs" c="dimmed">
            Repertoire / Mistake Positions / Puzzles · target 90%
          </Text>
        </Tile>
        <Tile title="Game Review" onClick={() => setPanel("standard")}>
          <Text size="sm" mt="xs">
            {d.standard.current} of {d.standard.total} games at the current standard
          </Text>
          <Progress value={(100 * d.standard.current) / d.standard.total} size="sm" mt={6} />
          <Text size="xs" c="dimmed" mt={4}>
            Queue: {d.queue.pending} games, {d.queue.state}
          </Text>
        </Tile>
      </SimpleGrid>

      <Card withBorder p="sm">
        <Text size="xs" c="dimmed" tt="uppercase" fw={700} mb={4}>
          Rating (context)
        </Text>
        <Group gap="xl">
          {d.ratings.map((x) => (
            <Text key={x.account + x.tc} size="sm">
              {x.account} {x.tc}: <b>{x.rating}</b>{" "}
              <Text span c={x.delta >= 0 ? "teal" : "red"} size="xs">
                {x.delta >= 0 ? "+" : ""}
                {x.delta} / 30d
              </Text>
            </Text>
          ))}
        </Group>
      </Card>

      <Drawer
        opened={panel !== null}
        onClose={() => setPanel(null)}
        position="right"
        size="xl"
        title={panel}
      >
        {panel === "error" && (
          <Stack>
            <ErrorRateChart trend={d.trend} baseline={d.baseline?.value ?? null} height={260} />
            <Text size="sm">Blunders: {d.blunderRate?.value} per 100 moves</Text>
            {d.facets.map((f) => (
              <Table key={f.facet}>
                <Table.Thead>
                  <Table.Tr>
                    <Table.Th>{f.facet}</Table.Th>
                    <Table.Th>Now</Table.Th>
                    <Table.Th>Baseline</Table.Th>
                    <Table.Th>Games</Table.Th>
                  </Table.Tr>
                </Table.Thead>
                <Table.Tbody>
                  {f.values.map((v) => (
                    <Table.Tr key={v.label} style={{ opacity: v.rate ? 1 : 0.5 }}>
                      <Table.Td>{v.label}</Table.Td>
                      <Table.Td>
                        {v.rate
                          ? `${v.rate.value} (${v.rate.lo}–${v.rate.hi})`
                          : "not enough games"}
                      </Table.Td>
                      <Table.Td>{v.baseline ?? "–"}</Table.Td>
                      <Table.Td>{v.games}</Table.Td>
                    </Table.Tr>
                  ))}
                </Table.Tbody>
              </Table>
            ))}
          </Stack>
        )}
        {panel === "weak" && (
          <Stack>
            {d.weaknesses.map((w) => (
              <Card key={w.id} withBorder p="sm">
                <Group justify="space-between">
                  <Stack gap={0}>
                    <Text fw={600}>{w.name}</Text>
                    <Text size="xs" c="dimmed">
                      {w.facet} · {w.games} games · {w.cards} Cards
                    </Text>
                  </Stack>
                  <Badge
                    color={
                      w.direction === "better" ? "teal" : w.direction === "worse" ? "red" : "gray"
                    }
                  >
                    {w.rate} vs {w.baseline}
                  </Badge>
                  <Button size="xs" variant="light">
                    Train this
                  </Button>
                </Group>
              </Card>
            ))}
            <Text fw={700}>Fixed</Text>
            {d.fixed.map((f) => (
              <Text key={f.name} size="sm">
                ✓ {f.name} ({f.facet}), {f.date}
              </Text>
            ))}
          </Stack>
        )}
        {panel === "training" && (
          <Stack>
            <Text>
              Repeat-Error rate:{" "}
              {d.repeatError
                ? `${pct(d.repeatError.rate)} of ${d.repeatError.n} repeats`
                : "no repeats yet"}
            </Text>
            <Text>
              Repertoire adherence: {d.adherence ? pct(d.adherence.rate) : "not enough games"}
            </Text>
            {d.retention.map((x) => (
              <Text key={x.kind}>
                {x.kind}:{" "}
                {x.value === null
                  ? "no reviews older than a day yet"
                  : `${pct(x.value)} of ${x.reviews} reviews`}
              </Text>
            ))}
          </Stack>
        )}
        {panel === "standard" && (
          <Stack>
            <Text>Review Standard: {d.standard.label}</Text>
            <Text>Metrics count only the {d.standard.current} games at this standard.</Text>
            <Button variant="light">Pause queue</Button>
          </Stack>
        )}
      </Drawer>
    </Stack>
  );
}
