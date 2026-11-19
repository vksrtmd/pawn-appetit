import type { NavigateOptions } from "@tanstack/react-router";

/**
 * Navigation utilities for conditional database modal flows
 */

export interface DatabaseNavigationOptions {
    /** The tab to open in the AddDatabase modal */
    tab?: "games" | "puzzles";
    /** URL to redirect to after modal submission */
    redirectTo?: string;
}

/**
 * Navigate to databases page with conditional modal opening
 *
 * @param navigate - TanStack Router navigate function
 * @param options - Navigation options for modal behavior
 * @returns NavigateOptions for TanStack Router
 */
export function navigateToDatabasesWithModal(
    navigate: (options: NavigateOptions) => void,
    options: DatabaseNavigationOptions = {},
): void {
    const { tab = "games", redirectTo } = options;

    const searchParams: Record<string, string> = {
        value: "add",
    };

    if (tab) {
        searchParams.tab = tab;
    }

    if (redirectTo) {
        searchParams.redirect = redirectTo;
    }

    navigate({
        to: "/databases",
        search: searchParams,
    });
}
