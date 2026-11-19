import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { type ReactNode, useState } from "react";
import { createStore } from "zustand";
import { beforeEach, expect, test, vi } from "vitest";
import { TreeStateContext } from "@/components/TreeStateContext";
import type { TreeStore } from "@/state/store/tree";
import { renderWithProviders } from "@/test/render";
import ReportModal from "../ReportModal";

const mocks = vi.hoisted(() => ({
  analyzeGame: vi.fn<(...args: unknown[]) => Promise<unknown>>(),
  cancelAnalysis: vi.fn<(id: string) => Promise<unknown>>(),
}));

vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (k: string) => k }) }));
vi.mock("@/bindings", () => ({
  commands: { analyzeGame: mocks.analyzeGame, cancelAnalysis: mocks.cancelAnalysis },
}));
vi.mock("@/state/atoms", async () => {
  const { atom } = await import("jotai");
  return {
    enginesAtom: atom([{ type: "local", name: "Stockfish", version: "1", path: "/sf" }]),
    referenceDbAtom: atom("/db.sqlite"),
  };
});

const store = createStore(() => ({ addAnalysis: vi.fn<() => void>() })) as unknown as TreeStore;

function Harness({ children }: { children?: ReactNode }) {
  const [inProgress, setInProgress] = useState(false);
  return (
    <TreeStateContext.Provider value={store}>
      <ReportModal
        tab="t1"
        initialFen="startpos"
        moves={[]}
        is960={false}
        reportingMode
        toggleReportingMode={() => {}}
        inProgress={inProgress}
        setInProgress={setInProgress}
      />
      {children}
    </TreeStateContext.Provider>
  );
}

beforeEach(() => {
  localStorage.clear();
  mocks.analyzeGame.mockReset().mockReturnValue(new Promise(() => {}));
  mocks.cancelAnalysis.mockReset().mockResolvedValue(undefined);
});

// #85: "Switching Time -> Depth gives depth 20"
test("switching the limit type from Time to Depth resets the limit to 20", async () => {
  const user = userEvent.setup();
  renderWithProviders(<Harness />);

  const limit = screen
    .getAllByRole("textbox")
    .find((el) => (el as HTMLInputElement).value === "500");
  expect(limit).toBeDefined();

  await user.click(screen.getByDisplayValue("features.board.analysis.time"));
  await user.click(await screen.findByRole("option", { name: "chess.goMode.depth", hidden: true }));

  await waitFor(() => expect(screen.getByDisplayValue("20")).toBeInTheDocument());
  expect(screen.queryByDisplayValue("500")).not.toBeInTheDocument();
}, 15_000);

// #85: "Stop ends the review"
test("Stop cancels the running analysis", async () => {
  const user = userEvent.setup();
  renderWithProviders(<Harness />);

  await user.click(screen.getByRole("button", { name: "features.board.analysis.analyze" }));
  await waitFor(() => expect(mocks.analyzeGame).toHaveBeenCalled());
  expect(mocks.analyzeGame.mock.calls[0][0]).toBe("report_t1");

  await user.click(await screen.findByRole("button", { name: "keybindings.stopEngine" }));

  await waitFor(() => expect(mocks.cancelAnalysis).toHaveBeenCalledWith("report_t1"));
  expect(
    await screen.findByRole("button", { name: "features.board.analysis.analyze" }),
  ).toBeInTheDocument();
}, 15_000);
