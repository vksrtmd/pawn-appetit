// PROTOTYPE: floating bar that cycles UI variants via the `variant` search param (← / → keys too),
// plus an optional row of extra options (e.g. a data scenario). Renders nothing in production.
import { ActionIcon, Group, Paper, SegmentedControl, Text } from "@mantine/core";
import { IconChevronLeft, IconChevronRight } from "@tabler/icons-react";
import { useEffect } from "react";

export function PrototypeSwitcher({
  variants,
  current,
  onChange,
  extra,
}: {
  variants: { key: string; name: string }[];
  current: string;
  onChange: (key: string) => void;
  extra?: {
    value: string;
    options: { key: string; label: string }[];
    onChange: (v: string) => void;
  };
}) {
  const i = Math.max(
    0,
    variants.findIndex((v) => v.key === current),
  );
  const go = (step: number) =>
    onChange(variants[(i + step + variants.length) % variants.length].key);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      if (t.closest("input, textarea, [contenteditable]")) return;
      if (e.key === "ArrowLeft") go(-1);
      if (e.key === "ArrowRight") go(1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  if (!import.meta.env.DEV) return null;
  return (
    <Paper
      shadow="xl"
      radius="xl"
      p={6}
      bg="yellow.3"
      c="dark"
      style={{
        position: "fixed",
        bottom: 16,
        left: "50%",
        transform: "translateX(-50%)",
        zIndex: 1000,
      }}
    >
      <Group gap="xs" wrap="nowrap">
        <ActionIcon variant="subtle" color="dark" onClick={() => go(-1)}>
          <IconChevronLeft />
        </ActionIcon>
        <Text fw={700} size="sm" miw={220} ta="center">
          {variants[i].key} ({variants[i].name})
        </Text>
        <ActionIcon variant="subtle" color="dark" onClick={() => go(1)}>
          <IconChevronRight />
        </ActionIcon>
        {extra && (
          <SegmentedControl
            size="xs"
            value={extra.value}
            onChange={extra.onChange}
            data={extra.options.map((o) => ({ value: o.key, label: o.label }))}
          />
        )}
      </Group>
    </Paper>
  );
}
