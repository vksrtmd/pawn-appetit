import { Text } from "@mantine/core";
import { useHotkeys } from "@mantine/hooks";
import { modals } from "@mantine/modals";
import { notifications } from "@mantine/notifications";
import { useLoaderData } from "@tanstack/react-router";
import { useAtom, useAtomValue } from "jotai";
import { useCallback, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { MAX_TABS } from "@/features/boards/constants";
import { activeTabAtom, tabsAtom } from "@/state/atoms";
import { keyMapAtom } from "@/state/keybindings";
import { dispose, duplicate, readTree, save } from "@/state/tabSession";
import { createTab } from "@/utils/tabs";

export function useTabManagement() {
  const { t } = useTranslation();
  const [tabs, setTabs] = useAtom(tabsAtom);
  const [activeTab, setActiveTab] = useAtom(activeTabAtom);

  const { documentDir } = useLoaderData({ from: "/boards" });

  useEffect(() => {
    if (tabs.length === 0) {
      createTab({
        tab: { name: t("features.tabs.newTab"), type: "new" },
        setTabs,
        setActiveTab,
      });
    }
  }, [tabs, setActiveTab, setTabs, t]);

  const closeTab = useCallback(
    async (value: string | null, forced?: boolean) => {
      if (value !== null) {
        let hasSource = false;
        setTabs((prevTabs) => {
          const closedTab = prevTabs.find((tab) => tab.value === value);
          hasSource = !!closedTab?.source;
          return prevTabs;
        });

        if (readTree(value)?.dirty && hasSource && !forced) {
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
                if ((await save(value, { dir: documentDir })) === "cancelled") return;
              } catch {
                // The failed write was already reported; keep the tab open so the edits survive.
                return;
              }
              closeTab(value, true);
            },
            onCancel: () => {
              closeTab(value, true);
            },
          });
          return;
        }

        setTabs((prevTabs) => {
          const index = prevTabs.findIndex((tab) => tab.value === value);
          if (index === -1) return prevTabs;

          const newTabs = prevTabs.filter((tab) => tab.value !== value);

          setActiveTab((currentActiveTab) => {
            if (value === currentActiveTab) {
              if (newTabs.length === 0) {
                return null;
              }
              if (index === prevTabs.length - 1) {
                return newTabs[index - 1].value;
              }
              return newTabs[index].value;
            }
            return currentActiveTab;
          });

          return newTabs;
        });

        await dispose(value);
      }
    },
    [setTabs, setActiveTab, documentDir, t],
  );

  const selectTab = useCallback(
    (index: number) => {
      setTabs((prevTabs) => {
        const targetIndex = Math.min(index, prevTabs.length - 1);
        if (targetIndex >= 0 && prevTabs[targetIndex]) {
          setActiveTab(prevTabs[targetIndex].value);
        }
        return prevTabs;
      });
    },
    [setTabs, setActiveTab],
  );

  const cycleTabs = useCallback(
    (reverse = false) => {
      setTabs((prevTabs) => {
        setActiveTab((currentActiveTab) => {
          const index = prevTabs.findIndex((tab) => tab.value === currentActiveTab);
          if (reverse) {
            if (index === 0) {
              return prevTabs[prevTabs.length - 1].value;
            }
            return prevTabs[index - 1].value;
          }
          if (index === prevTabs.length - 1) {
            return prevTabs[0].value;
          }
          return prevTabs[index + 1].value;
        });
        return prevTabs;
      });
    },
    [setTabs, setActiveTab],
  );

  const renameTab = useCallback(
    (value: string, name: string) => {
      setTabs((prev) =>
        prev.map((tab) => {
          if (tab.value === value) {
            return { ...tab, name };
          }
          return tab;
        }),
      );
    },
    [setTabs],
  );

  const duplicateTab = useCallback((value: string) => {
    duplicate(value);
  }, []);

  const keyMap = useAtomValue(keyMapAtom);
  useHotkeys([
    [keyMap.CLOSE_BOARD_TAB.keys, () => closeTab(activeTab, true)],
    [keyMap.CYCLE_BOARD_TABS.keys, () => cycleTabs()],
    [keyMap.REVERSE_CYCLE_BOARD_TABS.keys, () => cycleTabs(true)],
    [keyMap.BOARD_TAB_ONE.keys, () => selectTab(0)],
    [keyMap.BOARD_TAB_TWO.keys, () => selectTab(1)],
    [keyMap.BOARD_TAB_THREE.keys, () => selectTab(2)],
    [keyMap.BOARD_TAB_FOUR.keys, () => selectTab(3)],
    [keyMap.BOARD_TAB_FIVE.keys, () => selectTab(4)],
    [keyMap.BOARD_TAB_SIX.keys, () => selectTab(5)],
    [keyMap.BOARD_TAB_SEVEN.keys, () => selectTab(6)],
    [keyMap.BOARD_TAB_EIGHT.keys, () => selectTab(7)],
    [
      keyMap.BOARD_TAB_LAST.keys,
      () => {
        setTabs((prevTabs) => {
          selectTab(prevTabs.length - 1);
          return prevTabs;
        });
      },
    ],
    [
      keyMap.DUPLICATE_TAB.keys,
      () => {
        setActiveTab((current) => {
          if (current) {
            duplicateTab(current);
          }
          return current;
        });
      },
    ],
    [
      keyMap.NEW_GAME.keys,
      () => {
        if (tabs.length >= MAX_TABS) {
          notifications.show({
            title: t("features.tabs.limitReached"),
            message: t("features.tabs.limitReachedDesc", { max: MAX_TABS }),
            color: "yellow",
            autoClose: 5000,
          });
          return;
        }
        createTab({
          tab: { name: "Play", type: "play" },
          setTabs,
          setActiveTab,
        });
      },
    ],
  ]);

  const canCreateNewTab = useCallback(() => {
    return tabs.length < MAX_TABS;
  }, [tabs.length]);

  const showTabLimitNotification = useCallback(() => {
    notifications.show({
      title: t("features.tabs.limitReached"),
      message: t("features.tabs.limitReachedDesc", { max: MAX_TABS }),
      color: "yellow",
      autoClose: 5000,
    });
  }, [t]);

  return {
    tabs,
    activeTab,
    setActiveTab,
    setTabs,
    closeTab,
    renameTab,
    duplicateTab,
    selectTab,
    cycleTabs,
    canCreateNewTab,
    showTabLimitNotification,
  };
}
