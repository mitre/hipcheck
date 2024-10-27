/**
 * Debounce an event handler by waiting `waitFor` number of milliseconds before
 * permitting the event to be triggered again.
 */
export function debounce<F extends (...args: Parameters<F>) => ReturnType<F>>(
  waitFor: number,
  func: F,
): (...args: Parameters<F>) => void {
  let timeout: number;
  return (...args: Parameters<F>): void => {
    clearTimeout(timeout);
    timeout = setTimeout(() => func(...args), waitFor);
  };
}
