/**
 * Registry of per-tab atoms. `tabAtomFamily` is the only way to declare an atom keyed by a tab:
 * `disposeTabAtoms` (called by the Tab session's `dispose`) removes every registered family's
 * atoms for a closed tab.
 */
import type { Atom } from "jotai";
import { atomFamily } from "jotai-family";

type Registered = { tabOf: (param: never) => string; remove: (tabId: string) => void };
const registered: Registered[] = [];

/** `tabOf` names the tab a family param belongs to; it defaults to a param that is the tab id. */
export function tabAtomFamily<Param, A extends Atom<unknown>>(
    init: (param: Param) => A,
    options: { tabOf?: (param: Param) => string; areEqual?: (a: Param, b: Param) => boolean } = {},
) {
    const family = atomFamily(init, options.areEqual);
    const tabOf = options.tabOf ?? ((param: Param) => param as unknown as string);
    registered.push({
        tabOf: tabOf as (param: never) => string,
        remove: (tabId) => {
            for (const param of [...family.getParams()]) {
                if (tabOf(param) === tabId) family.remove(param);
            }
        },
    });
    return family;
}

export function disposeTabAtoms(tabId: string): void {
    for (const r of registered) r.remove(tabId);
}
