import { renderHook } from "@testing-library/react";
import { getDefaultStore } from "jotai";
import { beforeEach, expect, test, vi } from "vitest";

const { save, openConfirmModal } = vi.hoisted(() => ({
  save: vi.fn<(...a: unknown[]) => Promise<string>>(),
  openConfirmModal: vi.fn<(o: { onConfirm: () => Promise<void> }) => void>(),
}));

vi.mock("@tauri-apps/plugin-log", () => ({
  error: vi.fn<() => void>(),
  warn: vi.fn<() => void>(),
  info: vi.fn<() => void>(),
  debug: vi.fn<() => void>(),
  trace: vi.fn<() => void>(),
}));
vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (k: string) => k }) }));
vi.mock("@/bindings", () => ({
  commands: { killEngines: async () => ({ status: "ok", data: null }) },
}));
vi.mock("@/state/tabSession", () => ({ save }));
vi.mock("@mantine/modals", () => ({ modals: { openConfirmModal } }));
vi.mock("@tanstack/react-router", () => ({ useLoaderData: () => ({ documentDir: "/docs" }) }));

import { activeTabAtom, tabsAtom } from "@/state/atoms";
import type { Tab } from "@/utils/tabs";
import { defaultTree } from "@/utils/treeReducer";
import { useTabManagement } from "../useTabManagement";

const mk = (value: string): Tab =>
  ({
    name: value,
    value,
    type: "analysis",
    source: {
      type: "file",
      path: `/d/${value}.pgn`,
      name: value,
      numGames: 1,
      lastModified: 0,
      metadata: { type: "game", tags: [] },
    },
  }) as unknown as Tab;

beforeEach(() => {
  vi.clearAllMocks();
  const jotai = getDefaultStore();
  jotai.set(tabsAtom, [mk("a"), mk("b")]);
  jotai.set(activeTabAtom, "a");
  sessionStorage.setItem(
    "b",
    JSON.stringify({ version: 0, state: { ...defaultTree(), dirty: true } }),
  );
});

test("closing a dirty background tab shows the prompt and Save and close saves that tab", async () => {
  const { result } = renderHook(() => useTabManagement());
  await result.current.closeTab("b");

  expect(openConfirmModal).toHaveBeenCalledTimes(1);
  expect(getDefaultStore().get(tabsAtom)).toHaveLength(2);

  save.mockResolvedValue("saved");
  await openConfirmModal.mock.calls[0][0].onConfirm();

  expect(save).toHaveBeenCalledWith("b", { dir: "/docs" });
  expect(
    getDefaultStore()
      .get(tabsAtom)
      .map((t) => t.value),
  ).toEqual(["a"]);
});

test("a cancelled save-as keeps the tab open", async () => {
  const { result } = renderHook(() => useTabManagement());
  await result.current.closeTab("b");
  save.mockResolvedValue("cancelled");
  await openConfirmModal.mock.calls[0][0].onConfirm();
  expect(getDefaultStore().get(tabsAtom)).toHaveLength(2);
});
