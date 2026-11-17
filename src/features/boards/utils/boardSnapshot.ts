import domtoimage from "dom-to-image-more";

/**
 * Render the board in `host` to a PNG blob. The whole `.cg-wrap` is captured so
 * the arrows and highlights drawn next to `cg-board` are included, then cropped
 * to the board's size to drop the wrapper's padding.
 */
export async function renderBoardSnapshot(host: HTMLElement): Promise<Blob | null> {
    const wrap = host.querySelector<HTMLElement>(".cg-wrap");
    const board = wrap?.querySelector("cg-board");
    if (!wrap || !board) return null;

    const { width, height } = board.getBoundingClientRect();
    return domtoimage.toBlob(wrap, {
        width,
        height,
        style: { padding: "0", margin: "0", overflow: "hidden" },
    });
}
