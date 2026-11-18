import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import { renderWithProviders } from "@/test/render";
import type { FileMetadata } from "../../../utils/file";

const m = vi.hoisted(() => ({
  menu: [] as { key: string; onClick: () => void }[],
  openConfirmModal: vi.fn<(o: { onConfirm: () => Promise<void> }) => void>(),
  remove: vi.fn<(path: string) => Promise<void>>(),
}));

vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (k: string) => k }) }));
vi.mock("@/hooks/useLanguageChangeListener", () => ({
  useLanguageChangeListener: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/plugin-log", () => ({
  error: vi.fn<() => void>(),
  warn: vi.fn<() => void>(),
  info: vi.fn<() => void>(),
  debug: vi.fn<() => void>(),
  trace: vi.fn<() => void>(),
}));
vi.mock("@/bindings", () => ({ commands: {} }));
vi.mock("@tanstack/react-router", () => ({ useNavigate: () => vi.fn<() => void>() }));
vi.mock("@mantine/modals", () => ({ modals: { openConfirmModal: m.openConfirmModal } }));
// jsdom has no layout, so the real table renders no rows; this stand-in keeps the row handlers.
vi.mock("mantine-datatable", () => ({
  DataTable: ({
    records,
    onRowContextMenu,
  }: {
    records: FileMetadata[];
    onRowContextMenu: (a: { record: FileMetadata; event: unknown }) => void;
  }) => (
    <div>
      {records.map((record) => (
        <button
          type="button"
          key={record.path}
          onContextMenu={(event) => onRowContextMenu({ record, event })}
        >
          {record.name}
        </button>
      ))}
    </div>
  ),
}));
vi.mock("mantine-contextmenu", () => ({
  useContextMenu: () => ({
    showContextMenu: (items: typeof m.menu) => () => {
      m.menu = items;
    },
  }),
}));
vi.mock("@tauri-apps/plugin-fs", () => ({
  BaseDirectory: {},
  readTextFile: vi.fn<() => void>(),
  writeTextFile: vi.fn<() => void>(),
  exists: async () => true,
  remove: m.remove,
}));
vi.mock("@/features/files/utils/file", async (orig) => ({
  ...(await orig<object>()),
  invalidateFileIndex: vi.fn<() => void>(),
}));

import DirectoryTable from "../DirectoryTable";

const file: FileMetadata = {
  type: "file",
  name: "games",
  path: "/d/games.pgn",
  numGames: 3,
  metadata: { type: "game", tags: [] },
  lastModified: 0,
};

beforeEach(() => {
  vi.clearAllMocks();
  m.menu = [];
  m.remove.mockResolvedValue(undefined);
});

test("right-click delete asks first and deletes the file, its sidecar and the row only on confirm", async () => {
  const setFiles = vi.fn<(files: unknown[]) => void>();
  renderWithProviders(
    <DirectoryTable
      files={[file]}
      isLoading={false}
      setFiles={setFiles}
      selectedFile={null}
      setSelectedFile={vi.fn<() => void>()}
      search=""
      filter="all"
    />,
  );

  fireEvent.contextMenu(await screen.findByText("games"));
  m.menu.find((i) => i.key === "delete-file")?.onClick();

  expect(m.openConfirmModal).toHaveBeenCalledTimes(1);
  expect(m.remove).not.toHaveBeenCalled();
  expect(setFiles).not.toHaveBeenCalled();

  await m.openConfirmModal.mock.calls[0][0].onConfirm();
  await waitFor(() => expect(setFiles).toHaveBeenCalledWith([]));
  expect(m.remove.mock.calls.map((c) => c[0])).toEqual(["/d/games.pgn", "/d/games.info"]);
});
