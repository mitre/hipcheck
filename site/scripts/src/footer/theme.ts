import { querySelectorAll } from "../util/web.ts";
import { setPageTheme } from "../util/theme.ts";

/**
 * Sets up the logic for updating theme post-load.
 *
 * Note that this does _not_ handle setting the theme initially on page load.
 * That's done in a separate file since it needs to happen in the head, whereas
 * this code runs at the end of the body.
 */
export function setupThemeController() {
  querySelectorAll(".theme-option").forEach(($option) => {
    $option.addEventListener("click", (e) => {
      e.preventDefault();
      setPageTheme($option.dataset.theme);
    });
  });
}
