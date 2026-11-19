import { screen } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";

const { readGames, lexPgn } = vi.hoisted(() => ({
  readGames: vi.fn<(...a: unknown[]) => unknown>(),
  lexPgn: vi.fn<(pgn: string) => unknown>(),
}));

vi.mock("@tauri-apps/plugin-log", () => ({
  error: vi.fn<() => void>(),
  warn: vi.fn<() => void>(),
  info: vi.fn<() => void>(),
  debug: vi.fn<() => void>(),
  trace: vi.fn<() => void>(),
}));
vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (k: string) => k }) }));
vi.mock("@/bindings", () => ({ commands: { readGames, lexPgn } }));
// jsdom has no layout: show every row
vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getVirtualItems: () =>
      Array.from({ length: count }, (_, index) => ({ index, size: 38, start: index * 38 })),
    getTotalSize: () => count * 38,
  }),
}));

import { useState } from "react";
import { renderWithProviders } from "@/test/render";
import GameSelector from "../GameSelector";

const header = (white: string, black: string) => [
  { type: "Header", value: { tag: "White", value: white } },
  { type: "Header", value: { tag: "Black", value: black } },
];

function Harness() {
  const [games, setGames] = useState<Map<number, string>>(new Map());
  return (
    <GameSelector
      games={games}
      setGames={setGames}
      setPage={() => {}}
      total={3}
      path="/d/a.pgn"
      activePage={0}
    />
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  readGames.mockResolvedValue({ status: "ok", data: ["g0", "g1", "g2"] });
});

test("a game the lexer fails on gets a placeholder and the other games keep their names", async () => {
  lexPgn.mockImplementation((pgn) =>
    pgn === "g1"
      ? { status: "error", error: "lex" }
      : { status: "ok", data: header(`W${pgn}`, `B${pgn}`) },
  );

  renderWithProviders(<Harness />);

  expect(await screen.findByText("Wg0 - Bg0")).toBeTruthy();
  expect(await screen.findByText("Wg2 - Bg2")).toBeTruthy();
  expect(await screen.findByText("features.files.game.unreadable")).toBeTruthy();
});

test("a game with an illegal move is still listed by its headers", async () => {
  lexPgn.mockImplementation((pgn) => ({
    status: "ok",
    data: [...header(`W${pgn}`, `B${pgn}`), { type: "San", value: "Qh8" }],
  }));

  renderWithProviders(<Harness />);

  expect(await screen.findByText("Wg1 - Bg1")).toBeTruthy();
});
