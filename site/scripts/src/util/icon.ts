export function updateIcon(
  $node: HTMLElement,
  oldName: string,
  newName: string,
) {
  const iconUrl = getIconUrl($node);
  const newIconUrl = iconUrl.replace(`#icon-${oldName}`, `#icon-${newName}`);
  setIconUrl($node, newIconUrl);
}

/**
 * Get the URL out of an icon `use` element.
 */
function getIconUrl($node: HTMLElement): string {
  const iconUrl = $node.getAttributeNS(XLINK_NS, "href");
  if (iconUrl === null) throw new IconError();
  return iconUrl;
}

/**
 * Get the URL on an icon `use` element.
 */
function setIconUrl($node: HTMLElement, url: string) {
  $node.setAttributeNS(XLINK_NS, "href", url);
}

/**
 * The namespace URL for the Xlink namespace
 */
const XLINK_NS: string = "http://www.w3.org/1999/xlink";

/**
 * Error arising when trying to update the copy-to-clipboard icon.
 */
class IconError extends Error {
  constructor() {
    super(`could not find copy icon`);
  }
}
