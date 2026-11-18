import { fireEvent } from "@testing-library/react";
import { getDefaultStore } from "jotai";
import { beforeEach, expect, test, vi } from "vitest";

const { save } = vi.hoisted(() => ({ save: vi.fn<(...a: unknown[]) => Promise<string>>() }));

vi.mock("@tauri-apps/plugin-log", () => ({
  error: vi.fn<() => void>(),
  warn: vi.fn<() => void>(),
  info: vi.fn<() => void>(),
  debug: vi.fn<() => void>(),
  trace: vi.fn<() => void>(),
}));
vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (k: string) => k }) }));
vi.mock("@/bindings", () => ({ commands: {} }));
vi.mock("@/state/tabSession", () => ({ save, registerTabStore: () => () => {} }));
vi.mock("@tanstack/react-router", () => ({ useLoaderData: () => ({ documentDir: "/docs" }) }));
vi.mock("@/hooks/useResponsiveLayout", () => ({
  useResponsiveLayout: () => ({ layout: { chessBoard: { layoutType: "desktop" } } }),
}));
vi.mock("@/components/MoveControls", () => ({ default: () => null }));
vi.mock("../EditingCard", () => ({ default: () => null }));
vi.mock("../EvalListener", () => ({ default: () => null }));
vi.mock("../GameNotationWrapper", () => ({ default: () => null }));
vi.mock("../ResponsiveAnalysisPanels", () => ({ default: () => null }));
vi.mock("../ResponsiveBoard", () => ({ default: () => null }));
vi.mock("../VariantsNotation", () => ({ default: () => null }));

import { TreeStateProvider } from "@/components/TreeStateContext";
import { getBoardCommandTarget } from "@/features/boards/commands/boardCommandRegistry";
import { activeTabAtom, tabsAtom } from "@/state/atoms";
import { renderWithProviders } from "@/test/render";
import type { Tab } from "@/utils/tabs";
import BoardVariants from "../BoardVariants";

beforeEach(() => {
  vi.clearAllMocks();
  save.mockResolvedValue("saved");
  const jotai = getDefaultStore();
  jotai.set(tabsAtom, [
    {
      name: "v",
      value: "v1",
      type: "analysis",
      source: {
        type: "file",
        path: "/d/v.pgn",
        name: "v",
        numGames: 1,
        lastModified: 0,
        metadata: { type: "variants", tags: [] },
      },
    } as unknown as Tab,
  ]);
  jotai.set(activeTabAtom, "v1");
});

function mount() {
  renderWithProviders(
    <TreeStateProvider id="v1">
      <BoardVariants />
    </TreeStateProvider>,
  );
}

test("Ctrl+S saves the variants tab", () => {
  mount();
  fireEvent.keyDown(document.body, { key: "s", code: "KeyS", ctrlKey: true });
  expect(save).toHaveBeenCalledWith("v1", { dir: "/docs" });
});

test("menu Save runs the variants tab's registered save command", () => {
  mount();
  getBoardCommandTarget("v1")?.commands.save?.run();
  expect(save).toHaveBeenCalledWith("v1", { dir: "/docs" });
});
