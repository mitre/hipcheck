import { updateIcon } from "../util/icon.ts";
import { querySelectorAll } from "../util/web.ts";

/**
 * Sets up the collapse/expand functionality for docs navigation.
 */
export function setupDocsNav() {
  let $toggles;

  try {
    $toggles = querySelectorAll(".docs-nav-toggle");
  } catch (_e) {
    // Swallow these errors.
    return;
  }

  $toggles.forEach(($toggle) => {
    // Get the icon inside the toggle.
    const $toggleIcon = $toggle.querySelector(
      ".toggle-icon use",
    ) as HTMLElement;
    if ($toggleIcon === null) {
      console.log(`no icon found for toggle: '${$toggle}'`);
      return;
    }

    $toggle.addEventListener("click", (e) => {
      e.preventDefault();

      // Find the subsection list associated with the current toggle.
      const $parent = $toggle.parentElement?.parentElement;
      if ($parent === null || $parent === undefined) return;
      const $subsection = $parent.querySelector(
        ".docs-nav-section",
      ) as HTMLElement;
      if ($subsection === null) return;

      // Hide or show it.
      $subsection.classList.toggle("hidden");

      if (sectionIsHidden($subsection)) {
        updateIcon($toggleIcon, "chevron-right", "chevron-down");
      } else {
        updateIcon($toggleIcon, "chevron-down", "chevron-right");
      }
    });
  });
}

function sectionIsHidden($subsection: HTMLElement): boolean {
  return $subsection.classList.contains("hidden");
}
