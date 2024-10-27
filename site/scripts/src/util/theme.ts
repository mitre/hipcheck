import { querySelectorAll } from "./web.ts";

/**
 * Get the theme that's currently set by the user.
 */
export function getConfiguredTheme(): KnownTheme {
  const theme = localStorage.getItem("theme") ?? "system";

  switch (theme) {
    case "dark":
      return theme;
    case "light":
      return theme;
    case "system":
      return theme;
    default:
      throw new ThemeError(theme);
  }
}

/**
 * Update the theme on the page and in local storage.
 */
export function setPageTheme(theme: string | undefined) {
  if (theme === undefined) throw new ThemeError(theme);

  switch (theme) {
    case "system":
      localStorage.removeItem("theme");
      switch (preferredTheme()) {
        case "dark":
          document.documentElement.classList.add("dark");
          break;
        case "light":
          document.documentElement.classList.remove("dark");
          break;
      }

      break;

    case "light":
      localStorage.setItem("theme", theme);
      document.documentElement.classList.remove("dark");
      break;

    case "dark":
      localStorage.setItem("theme", theme);
      document.documentElement.classList.add("dark");
      break;

    default:
      throw new ThemeError(theme);
  }

  setButtons(theme);
}

/**
 * The known theme selector options.
 */
type KnownTheme = "dark" | "light" | "system";

/**
 * A theme that can be pulled explicitly from local storage.
 */
type StoredTheme = "dark" | "light";

/**
 * Set as active the button that matches the theme.
 *
 * Make sure to set all another buttons as inactive.
 */
function setButtons(theme: KnownTheme) {
  querySelectorAll(".theme-option").forEach(($option) => {
    if ($option.dataset.theme === theme) $option.dataset.active = "true";
    else delete $option.dataset.active;
  });
}

/**
 * Get the user's preferred theme based on a media query.
 */
function preferredTheme(): StoredTheme {
  const prefersDark =
    globalThis.window.matchMedia("(prefers-color-scheme: dark)").matches;

  if (prefersDark) return "dark";
  return "light";
}

/**
 * Indicates an error during theme selection.
 */
class ThemeError extends Error {
  constructor(theme: string | undefined) {
    super(
      `could not determine theme: '${theme || "undefined"}'`,
    );
  }
}
