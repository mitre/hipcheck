import { querySelector, querySelectorAll } from "../util/web.ts";

export function setupSmoothScrolling() {
  /*
   * This code from: https://stackoverflow.com/a/7717572
   * Used under the CC BY-SA 3.0 license with modifications.
   */
  querySelectorAll('a[href^="#"]').forEach((anchor) => {
    anchor.addEventListener("click", (e) => {
      e.preventDefault();
      if (e.currentTarget === null) return;

      const targetHeader = (e.currentTarget as HTMLElement).getAttribute("href");
      if (targetHeader === null) return;

      const $header = querySelector(targetHeader);
      const headerPosition = $header.getBoundingClientRect().top;
      const scrollAmount = globalThis.window.scrollY;
      const offsetPosition = headerPosition + scrollAmount;

      globalThis.window.scrollTo({
        top: offsetPosition,
        behavior: "smooth",
      });
    });
  });
}
