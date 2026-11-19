import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { getDefaultStore } from "jotai";
import { beforeEach, expect, test, vi } from "vitest";

const { save, readGames, openConfirmModal, show } = vi.hoisted(() => ({
  show: vi.fn<(...a: unknown[]) => void>(),
  save: vi.fn<(...a: unknown[]) => Promise<string>>(),
  readGames: vi.fn<(...a: unknown[]) => unknown>(),
  openConfirmModal: vi.fn<(o: { onConfirm: () => Promise<void>; onCancel?: () => void }) => void>(),
}));

vi.mock("@tauri-apps/plugin-log", () => ({
  error: vi.fn<() => void>(),
  warn: vi.fn<() => void>(),
  info: vi.fn<() => void>(),
  debug: vi.fn<() => void>(),
  trace: vi.fn<() => void>(),
}));
vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (k: string) => k }) }));
vi.mock("@/bindings", () => ({ commands: { readGames } }));
vi.mock("@/state/tabSession", () => ({ save, registerTabStore: () => () => {} }));
vi.mock("@mantine/modals", () => ({ modals: { openConfirmModal } }));
vi.mock("@tanstack/react-router", () => ({ useLoaderData: () => ({ documentDir: "/docs" }) }));
vi.mock("@/components/GameInfo", () => ({ default: () => null }));
vi.mock("../FenSearch", () => ({ default: () => null }));
vi.mock("../FileInfo", () => ({ default: () => null }));
vi.mock("../PgnInput", () => ({ default: () => null }));
vi.mock("../GameSelector", () => ({
  default: ({ setPage }: { setPage: (p: number) => void }) => (
    <button type="button" onClick={() => setPage(2)}>
      pick game 3
    </button>
  ),
}));
vi.mock("@mantine/notifications", () => ({ notifications: { show } }));
vi.mock("@/utils/chess", () => ({
  parsePGN: async (pgn: string) => {
    if (pgn.includes("BAD"))
      throw new (await import("@/utils/pgnParseError")).PgnParseError("Qh8", 3);
    return { ...(await import("@/utils/treeReducer")).defaultTree(), pgn };
  },
}));

import { TreeStateProvider } from "@/components/TreeStateContext";
import { currentTabAtom, tabsAtom, activeTabAtom } from "@/state/atoms";
import { renderWithProviders } from "@/test/render";
import type { Tab } from "@/utils/tabSchema";
import { defaultTree } from "@/utils/treeReducer";
import InfoPanel from "../InfoPanel";

beforeEach(() => {
  vi.clearAllMocks();
  const tab = {
    name: "t",
    value: "tab1",
    type: "analysis",
    gameNumber: 0,
    source: {
      type: "file",
      path: "/d/a.pgn",
      name: "a",
      numGames: 5,
      lastModified: 0,
      metadata: { type: "game", tags: [] },
    },
  } as unknown as Tab;
  const jotai = getDefaultStore();
  jotai.set(tabsAtom, [tab]);
  jotai.set(activeTabAtom, "tab1");
  sessionStorage.setItem(
    "tab1",
    JSON.stringify({ version: 0, state: { ...defaultTree(), dirty: true } }),
  );
  readGames.mockResolvedValue({ status: "ok", data: ["1. e4 *"] });
});

test("the game switch saves the tab, then opens the game that was clicked", async () => {
  save.mockResolvedValue("saved");
  renderWithProviders(
    <TreeStateProvider id="tab1">
      <InfoPanel />
    </TreeStateProvider>,
  );
  await userEvent.click(await screen.findByText(/units\.count/));
  await userEvent.click(await screen.findByText("pick game 3"));
  expect(openConfirmModal).toHaveBeenCalled();

  await openConfirmModal.mock.calls[0][0].onConfirm();

  expect(save).toHaveBeenCalledWith("tab1", { dir: "/docs" });
  await waitFor(() => expect(readGames).toHaveBeenCalledWith("/d/a.pgn", 2, 2));
  expect(getDefaultStore().get(currentTabAtom)?.gameNumber).toBe(2);
});

test("a game that does not parse is reported and the tab stays on its current game", async () => {
  readGames.mockResolvedValue({ status: "ok", data: ["1. e4 BAD *"] });
  renderWithProviders(
    <TreeStateProvider id="tab1">
      <InfoPanel />
    </TreeStateProvider>,
  );
  await userEvent.click(await screen.findByText(/units\.count/));
  await userEvent.click(await screen.findByText("pick game 3"));
  await openConfirmModal.mock.calls[0][0].onCancel?.();

  await waitFor(() => expect(show).toHaveBeenCalledTimes(1));
  expect(getDefaultStore().get(currentTabAtom)?.gameNumber).toBe(0);
});
