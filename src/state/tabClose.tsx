import { Text } from "@mantine/core";
import { modals } from "@mantine/modals";
import type { TFunction } from "i18next";
import { useHotkeys } from "@mantine/hooks";
import { useRouter } from "@tanstack/react-router";
import { getDefaultStore, useAtomValue } from "jotai";
import { useCallback } from "react";
import { useTranslation } from "react-i18next";
import { keyMapAtom } from "@/state/keybindings";
import { closeActiveTab, readTree, save } from "@/state/tabSession";
import { activeTabAtom, tabsAtom } from "@/state/uiAtoms";

/**
 * The one path for a user closing a tab (close button, Ctrl/Cmd+W, app menu). A tab with a file
 * source and unsaved changes asks first; every other tab, and the user's answer, runs `close`.
 * "Save and close" saves that tab and stays open when the save fails or is cancelled.
 */
export function requestCloseTab(
  tabId: string,
  { dir, t, close }: { dir: string; t: TFunction; close: () => void | Promise<void> },
): void | Promise<void> {
  const tab = getDefaultStore()
    .get(tabsAtom)
    .find((x) => x.value === tabId);
  if (!tab?.source || !readTree(tabId)?.dirty) return close();

  modals.openConfirmModal({
    title: t("common.unsavedChanges.title"),
    withCloseButton: false,
    children: <Text>{t("common.unsavedChanges.desc")}</Text>,
    labels: {
      confirm: t("common.unsavedChanges.saveAndClose"),
      cancel: t("common.unsavedChanges.closeWithoutSaving"),
    },
    onConfirm: async () => {
      try {
        if ((await save(tabId, { dir })) === "cancelled") return;
      } catch {
        // The failed write was already reported; keep the tab open so the edits survive.
        return;
      }
      await close();
    },
    onCancel: () => void close(),
  });
}

/**
 * Closes the active tab from any page, asking about unsaved changes first. Returns the callback
 * for the app menu and binds the close-tab shortcut to it.
 */
export function useCloseActiveTab(): () => Promise<void> {
  const { t } = useTranslation();
  const router = useRouter();
  const keyMap = useAtomValue(keyMapAtom);
  const close = useCallback(async () => {
    const tabId = getDefaultStore().get(activeTabAtom);
    if (!tabId) return;
    const { documentDir } = await router.options.context.loadDirs();
    await requestCloseTab(tabId, { dir: documentDir, t, close: closeActiveTab });
  }, [router, t]);
  useHotkeys([[keyMap.CLOSE_BOARD_TAB.keys, close]]);
  return close;
}
