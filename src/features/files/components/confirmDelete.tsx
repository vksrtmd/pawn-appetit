import { Text } from "@mantine/core";
import { modals } from "@mantine/modals";
import type { TFunction } from "i18next";

/** Asks before deleting `name`; `onConfirm` runs only when the user confirms. */
export function confirmDelete(t: TFunction, name: string, onConfirm: () => Promise<void>) {
  modals.openConfirmModal({
    title: t("features.files.delete.title"),
    withCloseButton: false,
    children: (
      <>
        <Text>{t("features.files.delete.message", { fileName: name })}</Text>
        <Text>{t("common.cannotUndo")}</Text>
      </>
    ),
    labels: { confirm: t("common.remove"), cancel: t("common.cancel") },
    confirmProps: { color: "red" },
    onConfirm,
  });
}
