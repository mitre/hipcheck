import { updateIcon } from "../util/icon.ts";
import {
  copyToClipboard,
  querySelector,
  querySelectorAll,
} from "../util/web.ts";

/**
 * Setup behavior for the install picker.
 *
 * This makes sure the platform buttons are clickable and update the install
 * command appropriately, and that the copy-to-clipboard button works.
 */
export function setupInstallerPicker() {
  // The buttons used to select the platform.
  let $buttons: Array<HTMLElement>;

  // The block containing the install command.
  let $cmd: HTMLElement;

  // The copy-to-clipboard button.
  let $copy: HTMLElement;

  // The icon inside the copy-to-clipboard button.
  let $copyIcon: HTMLElement;

  try {
    $buttons = querySelectorAll(".installer-button");
    $cmd = querySelector("#installer-cmd");
    $copy = querySelector("#installer-copy");
    $copyIcon = querySelector("#installer-copy > svg > use");
  } catch (_e) {
    // Swallow these errors.
    return;
  }

  $buttons.forEach(($button) => {
    $button.addEventListener("click", (e) => {
      e.preventDefault();
      $buttons.forEach(($button) => delete $button.dataset.active);
      $button.dataset.active = "true";
      $cmd.innerText = installerForPlatform($button.dataset.platform);
    });

    if ($button.dataset.active && $button.dataset.active === "true") {
      $cmd.innerText = installerForPlatform($button.dataset.platform);
    }
  });

  $copy.addEventListener("click", (e) => {
    e.preventDefault();
    copyToClipboard($cmd.innerText);
    updateIcon($copyIcon, "clipboard", "check");
    // Set the icon back on a timer.
    setTimeout(() => updateIcon($copyIcon, "check", "clipboard"), 1_500);
  });
}

/**
 * Get the install script based on the chosen platform.
 */
function installerForPlatform(platform: string | undefined): string {
  if (platform === undefined) throw new UnknownPlatformError(platform);

  switch (platform) {
    case "macos":
      return UNIX_INSTALLER;
    case "linux":
      return UNIX_INSTALLER;
    case "windows":
      return WINDOWS_INSTALLER;
    default:
      throw new UnknownPlatformError(platform);
  }
}

/**
 * The current host of the site.
 */
const HOST: string =
  `${globalThis.window.location.protocol}//${globalThis.window.location.host}`;

/**
 * The install script to use for Unix (macOS and Linux) platforms.
 */
const UNIX_INSTALLER: string = `curl -LsSf ${HOST}/dl/install.sh | sh`;

/**
 * The install script to use for Windows.
 */
const WINDOWS_INSTALLER: string = `irm ${HOST}/dl/install.ps1 | iex`;

/**
 * Indicates an error while trying to detect the user's install platform.
 */
class UnknownPlatformError extends Error {
  constructor(platform: string | undefined) {
    super(
      `could not determine platform: '${platform || "undefined"}'`,
    );
  }
}
