/**
 * This code adapted from char's "rainbow" project.
 *
 * https://github.com/char/rainbow/blob/47721ad574c61d59921435a2bff41b9fd582540d/src/util/elem.ts
 *
 * It's licensed under the terms of the WTFPL.
 *
 * https://bsky.app/profile/pet.bun.how/post/3l7vv6ddjn426
 */

// deno-lint-ignore-file ban-types

export type ElemProps<E extends Element> = {
  [K in keyof E as E[K] extends Function ? never : K]?: E[K];
};

// Just a shorthand for a long type name.
type HTMLElemTagNameMap = HTMLElementTagNameMap;

// Children can be built from an existing element, string, or text.
type IntoChild = Element | string | Text;

// We permit adding classes and data attributes.
type Extras = {
  classList?: string[];
  dataset?: Partial<Record<string, string>>;
};

// Attributes to add.
type Attrs<K extends keyof HTMLElemTagNameMap> =
  | ElemProps<HTMLElemTagNameMap[K]>
  | ElemProps<HTMLElemTagNameMap[K]>[];

// A little way to reduce an object down to only defined entries, since
// entries can have a key but an undefined value.
function removeUndefinedValues(x: object): object {
  const entries = Object.entries(x).filter(([_k, v]) => v !== undefined);
  return Object.fromEntries(entries);
}

/**
 * Construct a new HTMLElement
 */
export function elem<K extends keyof HTMLElemTagNameMap>(
  tag: K,
  attrs: Attrs<K> = {},
  extras: Extras = {},
  children: IntoChild[] = [],
): HTMLElemTagNameMap[K] {
  // Create the new element.
  const element = document.createElement(tag);

  // Assign any defined values from `attrs`.
  Object.assign(element, removeUndefinedValues(attrs));

  // Fill in any provided classes.
  if (extras.classList) {
    extras.classList.forEach((c) => element.classList.add(c));
  }

  // Fill in any assigned data attributes.
  if (extras.dataset) {
    Object.entries(extras.dataset)
      .filter(([_k, v]) => v !== undefined)
      .forEach(([k, v]) => (element.dataset[k] = v));
  }

  // Populate any children.
  const nodes = children.map(
    (e) => (typeof e === "string" ? document.createTextNode(e) : e)
  );
  element.append(...nodes);

  return element;
}
