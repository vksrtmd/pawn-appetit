import { beforeEach, expect, test, vi } from "vitest";

const toBlob = vi.hoisted(() => vi.fn());
vi.mock("dom-to-image-more", () => ({ default: { toBlob } }));

import { renderBoardSnapshot } from "../boardSnapshot";

beforeEach(() => {
    toBlob.mockReset().mockResolvedValue(new Blob());
    document.body.innerHTML = `
        <div id="host"><div class="cg-wrap">
            <cg-container><cg-board></cg-board><svg class="cg-shapes"></svg></cg-container>
        </div></div>`;
    document.querySelector("cg-board")!.getBoundingClientRect = () =>
        ({ width: 400, height: 400 }) as DOMRect;
});

test("snapshot captures the whole board wrapper (arrows included), cropped to the board size", async () => {
    const host = document.getElementById("host") as HTMLDivElement;
    await renderBoardSnapshot(host);
    const [target, options] = toBlob.mock.calls[0];
    expect(target).toBe(host.querySelector(".cg-wrap"));
    expect(target.querySelector("svg.cg-shapes")).not.toBeNull();
    expect(options).toMatchObject({ width: 400, height: 400 });
});

test("snapshot of an empty host does nothing", async () => {
    const empty = document.createElement("div");
    expect(await renderBoardSnapshot(empty)).toBeNull();
    expect(toBlob).not.toHaveBeenCalled();
});
