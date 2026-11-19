import { createContext, useEffect, useMemo } from "react";
import { registerTabStore } from "@/state/tabSession";
import { createTreeStore, type TreeStore } from "@/state/store/tree";
import type { TreeState } from "@/utils/treeReducer";

export const TreeStateContext = createContext<TreeStore | null>(null);

export function TreeStateProvider({
  id,
  initial,
  children,
}: {
  id?: string;
  initial?: TreeState;
  children: React.ReactNode;
}) {
  const store = useMemo(() => createTreeStore(id, initial), [id, initial]);

  useEffect(() => (id ? registerTabStore(id, store) : undefined), [id, store]);

  return <TreeStateContext.Provider value={store}>{children}</TreeStateContext.Provider>;
}
