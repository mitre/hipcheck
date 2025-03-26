import { setupTheme } from "./theme.ts";
import { setupnav } from "./docs.ts";


/**
 * Run all page setup operations, initializing all interactive widgets.
 */
function setup() {
  setupTheme();
  setupnav();
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
