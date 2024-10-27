import { setupTheme } from "./theme.ts";

/**
 * Run all page setup operations, initializing all interactive widgets.
 */
function setup() {
  setupTheme();
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
