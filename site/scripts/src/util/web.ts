/**
 * document.querySelector with type conversion and error handling.
 */
export function querySelector(selector: string): HTMLElement {
  const $elem = document.querySelector(selector);
  if ($elem === null) throw new QueryError(`could not find ${selector}`);
  return $elem as HTMLElement;
}

/**
 * document.querySelectorAll with type conversions and error handling.
 */
export function querySelectorAll(selector: string): Array<HTMLElement> {
  const $elems = document.querySelectorAll(selector);
  if ($elems === null) throw new QueryError(`could not find all '${selector}'`);
  return Array.from($elems) as Array<HTMLElement>;
}

/**
 * navigator.clipboard.writeText with error handling.
 */
export function copyToClipboard(text: string) {
  navigator.clipboard.writeText(text).then(null, (reason) => {
    throw new ClipboardError(reason);
  });
}

/**
 * Indicates an error while attempting to put data into the clipboard.
 */
class ClipboardError extends Error {
  constructor(source: unknown) {
    super(`clipboard copy rejected: '${source}'`);
  }
}

/**
 * Indicates an error while trying to select one or more elements on the page.
 */
class QueryError extends Error {
  constructor(msg: string) {
    super(msg);
  }
}
