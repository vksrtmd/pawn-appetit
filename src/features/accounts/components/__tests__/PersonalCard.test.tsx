import { MantineProvider } from "@mantine/core";
import { Provider as JotaiProvider } from "jotai";
import { renderToString } from "react-dom/server";
import { describe, expect, test, vi } from "vitest";
import PersonalPlayerCard from "../PersonalCard";

vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock("@/features/databases/components/drawers/FideInfo", () => ({ default: () => null }));
vi.mock("../PersonalCardPanels/OpeningsPanel", () => ({ default: () => null }));
vi.mock("../PersonalCardPanels/OverviewPanel", () => ({ default: () => null }));
vi.mock("../PersonalCardPanels/RatingsPanel", () => ({ default: () => null }));

describe("Account database drawer card", () => {
  test("renders without a database view store provider", () => {
    const render = () =>
      renderToString(
        <JotaiProvider>
          <MantineProvider>
            <PersonalPlayerCard name="Stats" info={{ site_stats_data: [] }} />
          </MantineProvider>
        </JotaiProvider>,
      );

    expect(render).not.toThrow();
  });
});
