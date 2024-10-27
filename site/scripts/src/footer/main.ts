import { setupDocsNav } from "./docs.ts";
import { setupInstallerPicker } from "./installer.ts";
import { setupSmoothScrolling } from "./scroll.ts";
import { setupSearch, setupSearchModal } from "./search.ts";
import { setupThemeController } from "./theme.ts";

/**
 * Run all page setup operations, initializing all interactive widgets.
 *
 * There are currently three widgets:
 *
 * - Theme Controller in the navigation bar.
 * - Installer Picker on the homepage.
 * - Search button in the navigation bar.
 */
function setup() {
  setupThemeController();
  setupInstallerPicker();
  setupSearchModal();
  setupSearch();
  setupSmoothScrolling();
  setupDocsNav();
}

/**
 * Do setup, logging errors to the console.
 */
(function () {
  try {
    setup();
  } catch (e) {
    console.error(e);
  }
})();
