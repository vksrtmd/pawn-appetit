import { fireEvent, renderHook, waitFor } from "@testing-library/react";
import { getDefaultStore } from "jotai";
import { beforeEach, expect, test, vi } from "vitest";
import { z } from "zod";

const { save, openConfirmModal, killEngines } = vi.hoisted(() => ({
  killEngines: vi.fn<(...a: unknown[]) => Promise<unknown>>(),
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
vi.mock("@/bindings", () => ({ commands: { killEngines } }));
vi.mock("@/state/tabSession", async (original) => ({
  ...(await original<typeof import("@/state/tabSession")>()),
  save,
}));
vi.mock("@/features/files/utils/file", () => ({
  fileMetadataSchema: z.any(),
  invalidateFileIndex: vi.fn<() => void>(),
}));
vi.mock("@mantine/modals", () => ({ modals: { openConfirmModal } }));
// No boards loader here: the hook must work from any page, with only the router context.
vi.mock("@tanstack/react-router", () => ({
  useRouter: () => ({
    options: { context: { loadDirs: async () => ({ documentDir: "/docs" }) } },
  }),
}));

import { activeTabAtom, tabsAtom } from "@/state/atoms";
import { createTreeStore } from "@/state/store/tree";
import { open, registerTabStore } from "@/state/tabSession";
import type { Tab } from "@/utils/tabSchema";
import { defaultTree } from "@/utils/treeReducer";
import { useCloseActiveTab } from "../tabClose";

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

const tabIds = () =>
  getDefaultStore()
    .get(tabsAtom)
    .map((t) => t.value);

beforeEach(() => {
  vi.clearAllMocks();
  sessionStorage.clear();
  killEngines.mockResolvedValue({ status: "ok", data: null });
  const jotai = getDefaultStore();
  jotai.set(tabsAtom, [mk("a"), mk("b")]);
  jotai.set(activeTabAtom, "b");
});

function makeDirty(id: string) {
  open(id, { ...defaultTree(), dirty: true });
}

test("Ctrl/Cmd+W on a dirty tab, from any page, shows the unsaved-changes dialog", async () => {
  makeDirty("b");
  renderHook(() => useCloseActiveTab());

  fireEvent.keyDown(document.body, { key: "w", code: "KeyW", ctrlKey: true, metaKey: true });

  await waitFor(() => expect(openConfirmModal).toHaveBeenCalledTimes(1));
  expect(tabIds()).toEqual(["a", "b"]);
});

test("the menu's close-current-tab on a dirty tab shows the dialog, and Save and close saves it", async () => {
  makeDirty("b");
  const { result } = renderHook(() => useCloseActiveTab());

  await result.current();
  expect(openConfirmModal).toHaveBeenCalledTimes(1);
  expect(tabIds()).toEqual(["a", "b"]);

  save.mockResolvedValue("saved");
  await openConfirmModal.mock.calls[0][0].onConfirm();
  expect(save).toHaveBeenCalledWith("b", { dir: "/docs" });
  expect(tabIds()).toEqual(["a"]);
});

test("the menu path and the hotkey dispose a clean tab without a dialog", async () => {
  open("b", defaultTree());
  const { result } = renderHook(() => useCloseActiveTab());

  await result.current();

  expect(openConfirmModal).not.toHaveBeenCalled();
  expect(tabIds()).toEqual(["a"]);
  expect(killEngines).toHaveBeenCalledWith("b");
  expect(sessionStorage.getItem("b")).toBeNull();
});

test("a dirty tab whose stored tree could not be written still asks before closing", async () => {
  const store = createTreeStore();
  store.setState({ ...defaultTree(), dirty: true });
  registerTabStore("b", store);
  expect(sessionStorage.getItem("b")).toBeNull();
  const { result } = renderHook(() => useCloseActiveTab());

  await result.current();

  expect(openConfirmModal).toHaveBeenCalledTimes(1);
  expect(tabIds()).toEqual(["a", "b"]);
});
