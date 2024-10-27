import { getConfiguredTheme, setPageTheme } from "../util/theme.ts";

/**
 * Sets up the logic for updating theme pre-load.
 */
export function setupTheme() {
  setPageTheme(getConfiguredTheme());
}
