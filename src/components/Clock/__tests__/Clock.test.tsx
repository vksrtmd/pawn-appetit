import { MantineProvider } from "@mantine/core";
import { renderToStaticMarkup } from "react-dom/server";
import { expect, test } from "vitest";
import { TreeStateContext } from "@/components/TreeStateContext";
import { createTreeStore } from "@/state/store/tree";
import { defaultTree } from "@/utils/treeReducer";
import Clock from "..";

function render(whiteTime?: number) {
  const store = createTreeStore("t", defaultTree());
  return renderToStaticMarkup(
    <MantineProvider>
      <TreeStateContext.Provider value={store}>
        <Clock color="white" turn="white" whiteTime={whiteTime} />
      </TreeStateContext.Provider>
    </MantineProvider>,
  );
}

test("a clock reading of 0 is visible and shows 0:00", () => {
  const html = render(0);
  expect(html).toContain("0:00");
  expect(html).not.toContain("visibility:hidden");
});

test("no clock reading stays hidden", () => {
  expect(render(undefined)).toContain("visibility:hidden");
});
