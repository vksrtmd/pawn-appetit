import { Button, Modal, Stack } from "@mantine/core";
import { useLoaderData } from "@tanstack/react-router";
import { notifications } from "@mantine/notifications";
import { useSetAtom } from "jotai";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { FilenameInput } from "@/features/files/components/FilenameInput";
import { FileTypeSelector } from "@/features/files/components/FileTypeSelector";
import {
  PgnSourceInput,
  type PgnTarget,
  resolvePgnTarget,
} from "@/features/files/components/PgnSourceInput";
import { RenameExistsError, RenameSidecarError, renameFile } from "@/features/files/utils/fileOps";
import { tabsAtom } from "@/state/atoms";
import { followRename } from "@/utils/tabSources";
import { createFile } from "@/utils/files";
import type { Directory, FileMetadata, FileType } from "../../utils/file";

export function CreateModal({
  opened,
  setOpened,
  files,
  setFiles,
  setSelected,
}: {
  opened: boolean;
  setOpened: (opened: boolean) => void;
  files: (FileMetadata | Directory)[];
  setFiles: (files: (FileMetadata | Directory)[]) => void;
  setSelected: React.Dispatch<React.SetStateAction<FileMetadata | null>>;
}) {
  const { t } = useTranslation();
  const [pgnTarget, setPgnTarget] = useState<PgnTarget>({ type: "pgn", target: "" });
  const [filename, setFilename] = useState("");
  const [filetype, setFiletype] = useState<FileType>("game");
  const [error, setError] = useState("");
  const { documentDir } = useLoaderData({ from: "/files" });

  async function addFile() {
    const resolvedPgnTarget = await resolvePgnTarget(pgnTarget);
    const newFile = await createFile({
      filename,
      filetype,
      pgn: resolvedPgnTarget.content,
      dir: documentDir,
    });
    if (newFile.isErr) {
      setError(newFile.error.message);
    } else {
      setFiles([...files, newFile.value]);
      setSelected(newFile.value);
      setError("");
      setOpened(false);
      setFilename("");
      setFiletype("game");
    }
  }

  return (
    <Modal
      opened={opened}
      onClose={() => setOpened(false)}
      title={t("features.files.create.title")}
    >
      <form
        onSubmit={(e) => {
          e.preventDefault();
          addFile();
        }}
      >
        <Stack>
          <FilenameInput value={filename} onChange={setFilename} error={error} />
          <FileTypeSelector value={filetype} onChange={setFiletype} />

          <PgnSourceInput
            setFilename={setFilename}
            setPgnTarget={setPgnTarget}
            pgnTarget={pgnTarget}
          />

          <Button style={{ marginTop: "1rem" }} type="submit">
            {t("common.create")}
          </Button>
        </Stack>
      </form>
    </Modal>
  );
}

export function EditModal({
  opened,
  setOpened,
  mutate,
  setSelected,
  metadata,
}: {
  opened: boolean;
  setOpened: (opened: boolean) => void;
  mutate: () => void;
  setSelected: React.Dispatch<React.SetStateAction<FileMetadata | null>>;
  metadata: FileMetadata;
}) {
  const { t } = useTranslation();

  const [filename, setFilename] = useState(metadata.name);
  const [filetype, setFiletype] = useState<FileType>(metadata.metadata.type);
  const [error, setError] = useState("");
  const setTabs = useSetAtom(tabsAtom);

  async function editFile() {
    let renamed: FileMetadata;
    let failure = "";
    try {
      renamed = await renameFile(metadata, filename, filetype);
    } catch (e) {
      if (e instanceof RenameSidecarError) {
        // The file did move: the tabs and the list follow it, then the user hears about the sidecar
        renamed = e.renamed;
        failure = t("fileRename.sidecarFailed", { name: filename });
      } else {
        setError(
          e instanceof RenameExistsError
            ? t("fileRename.exists", { name: filename })
            : t("fileRename.failed", { name: filename }),
        );
        return;
      }
    }
    mutate();
    setSelected((selected) => (selected?.path === metadata.path ? renamed : selected));
    setTabs((tabs) => followRename(tabs, metadata.path, renamed));

    setError("");
    setOpened(false);
    if (failure) notifications.show({ color: "red", message: failure });
  }

  return (
    <Modal opened={opened} onClose={() => setOpened(false)} title={t("features.files.edit.title")}>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          editFile();
        }}
      >
        <Stack>
          <FilenameInput value={filename} onChange={setFilename} error={error} />
          <FileTypeSelector value={filetype} onChange={setFiletype} />

          <Button style={{ marginTop: "1rem" }} type="submit">
            {t("common.edit")}
          </Button>
        </Stack>
      </form>
    </Modal>
  );
}
