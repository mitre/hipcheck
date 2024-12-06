import { debounce } from "../util/event.ts";
import { elem } from "../util/html.ts";
import { querySelector } from "../util/web.ts";

/**
 * Sets up functionality for opening and closing the search modal.
 */
export function setupSearchModal() {
  const $button = querySelector("#search-button");
  const $modal = querySelector("#search-modal");
  const $modalClose = querySelector("#search-modal-close");
  const $modalShroud = querySelector("#search-modal-shroud");
  const $modalBox = querySelector("#search-modal-box");
  const $searchInput = querySelector("#search-input");

  // Need all of these together to make sure clicking the *background* closes
  // the search modal, but clicking inside the box (anywhere other than the
  // close button) does *not* close the modal.
  $button.addEventListener(
    "click",
    (e) => toggleModal(e, $modal, $searchInput),
  );
  $modalShroud.addEventListener(
    "click",
    (e) => toggleModal(e, $modal, $searchInput),
  );
  $modalClose.addEventListener(
    "click",
    (e) => toggleModal(e, $modal, $searchInput),
  );
  $modalBox.addEventListener("click", (e) => e.stopPropagation());

  // Keyboard shortcuts.
  document.addEventListener("keydown", (e) => {
    // 'Meta+K' to open or close the modal.
    if (e.metaKey === true && e.shiftKey === false && e.key === "k") {
      e.preventDefault();
      $button.click();
      return;
    }

    if (modalIsOpen($modal)) {
      switch (e.key) {
        case "Escape":
          e.preventDefault();
          $modalShroud.click();
          return;
        case "ArrowDown":
          // TODO: Focus the next result in the search results.
          break;
        case "ArrowUp":
          // TODO: Focus the prior result in the search result.
          break;
        default:
          break;
      }
    }
  });
}

/**
 * Toggle whether the modal is open or not.
 */
function toggleModal(
  e: MouseEvent,
  $modal: HTMLElement,
  $searchInput: HTMLElement,
) {
  e.preventDefault();
  $modal.classList.toggle("hidden");
  if (modalIsOpen($modal)) $searchInput.focus();
}

/**
 * Check if the modal is open.
 */
function modalIsOpen($modal: HTMLElement): boolean {
  return !$modal.classList.contains("hidden");
}

/**
 * Elasticlunr is the library Zola uses for search integration, and it doesn't
 * provide TypeScript type definitions. So the definitions here are just enough
 * to get Deno's linter to stop complaining, but they do mean we don't really
 * get type-checking protection for interacting with the Elasticluner API.
 *
 * In the future if we wanted type-checking for this API we could replace the
 * 'any' with an actual description of the relevant types.
 */

// Define the Index type.
type Index = {
  // deno-lint-ignore no-explicit-any
  load: (data: Promise<any>) => Promise<Index>;
  // deno-lint-ignore no-explicit-any
  search: (query: string, options?: any) => SearchResult;
};

/**
 * Data returned from the Elasticlunr search function
 */
type SearchResult = {
  ref: string;
  score: number;
  doc: {
    body: string;
    id: string;
    title: string;
  };
};

// Define the "elasticlunr" global.
declare global {
  let elasticlunr: {
    Index: Index;
    // deno-lint-ignore no-explicit-any
    stemmer: any;
  };
}

/**
 * The path to the search index created by Zola.
 */
const SEARCH_INDEX: string = "/search_index.en.json";

/**
 * The maximum number of results to show in searches.
 */
const MAX_ITEMS: number = 6;

/**
 * Setup the search operation within the search modal.
 */
export function setupSearch() {
  const $searchInput = querySelector("#search-input") as HTMLInputElement;
  const $searchResults = querySelector("#search-results");
  const $searchResultsItems = querySelector("#search-results-items");

  // The search index, representing the content of the site.
  let index: Promise<Index>;

  // The current term being searched by the user.
  let currentTerm = "";

  const initIndex = async function () {
    // If no index, then asynchronously load it from the index file.
    if (index === undefined) {
      index = fetch(SEARCH_INDEX)
        .then(
          async function (response) {
            return await elasticlunr.Index.load(await response.json());
          },
        );
    }

    return await index;
  };

  $searchInput.addEventListener(
    "keyup",
    debounce(150, async function () {
      const term = $searchInput.value.trim();
      if (term === currentTerm) return;

      $searchResults.style.display = term === "" ? "none" : "block";
      $searchResultsItems.innerHTML = "";

      currentTerm = term;
      if (currentTerm === "") return;

      const results: SearchResult[] = (await initIndex())
        .search(term, {
          bool: "AND",
          fields: {
            title: { boost: 2 },
            body: { boost: 1 },
          },
        });

      if (results.length === 0) {
        $searchResults.style.display = "none";
        return;
      }

      for (let i = 0; i < Math.min(results.length, MAX_ITEMS); ++i) {
        const entry = buildListEntry(results[i], currentTerm.split(" "));
        $searchResultsItems.appendChild(entry);
      }
    }),
  );
}

/**
 * Build an HTML element for each item in the search results.
 */
function buildListEntry(data: SearchResult, terms: string[]): HTMLElement {
  return elem("li", {}, {
    classList: ["border-t", "border-neutral-300", "dark:border-neutral-500"],
  }, [
    elem("div", {}, {}, [
      elem("a", { href: data.ref }, {
        classList: [
          "block",
          "px-5",
          "py-2",
          "hover:bg-blue-50",
          "dark:hover:bg-blue-500",
          "hover:text-blue-500",
          "dark:hover:text-white",
          "group",
        ],
      }, [
        elem("span", {}, {
          classList: ["block", "text-base", "mb-1", "font-medium"],
        }, [
          data.doc.title,
        ]),
        elem("span", {}, {
          classList: [
            "block",
            "text-neutral-500",
            "text-sm",
            "group-hover:text-blue-500",
          ],
        }, [
          makeTeaser(data.doc.body, terms),
        ]),
      ]),
    ]),
  ]);
}

/**
 * Construct a usable preview of the body that matched the search term.
 *
 * This code adapted from Zola's sample search code, itself adapted from mdbook.
 * Licensed under the terms of the MIT license.
 *
 * https://github.com/getzola/zola/blob/master/LICENSE
 */
function makeTeaser(body: string, terms: string[]): HTMLElement {
  const TERM_WEIGHT = 40;
  const NORMAL_WORD_WEIGHT = 2;
  const FIRST_WORD_WEIGHT = 8;
  const TEASER_MAX_WORDS = 15;

  const stemmedTerms = terms.map(function (w) {
    return elasticlunr.stemmer(w.toLowerCase());
  });

  let termFound = false;
  let index = 0;
  // contains elements of ["word", weight, index_in_document]
  const weighted: ([string, number, number])[] = [];

  // split in sentences, then words
  const sentences = body.toLowerCase().split(". ");

  for (const i in sentences) {
    const words = sentences[i].split(" ");
    let value = FIRST_WORD_WEIGHT;

    for (const j in words) {
      const word = words[j];

      if (word.length > 0) {
        for (const k in stemmedTerms) {
          if (elasticlunr.stemmer(word).startsWith(stemmedTerms[k])) {
            value = TERM_WEIGHT;
            termFound = true;
          }
        }

        weighted.push([word, value, index]);
        value = NORMAL_WORD_WEIGHT;
      }

      index += word.length;
      // ' ' or '.' if last word in sentence
      index += 1;
    }

    // because we split at a two-char boundary '. '
    index += 1;
  }

  if (weighted.length === 0) {
    const final = body;
    const span = elem("span", {}, {}, []);
    span.innerHTML = final;
    return span;
  }

  const windowWeights: number[] = [];
  const windowSize = Math.min(weighted.length, TEASER_MAX_WORDS);
  // We add a window with all the weights first
  let curSum = 0;
  for (let i = 0; i < windowSize; i++) {
    curSum += weighted[i][1];
  }
  windowWeights.push(curSum);

  for (let i = 0; i < weighted.length - windowSize; i++) {
    curSum -= weighted[i][1];
    curSum += weighted[i + windowSize][1];
    windowWeights.push(curSum);
  }

  // If we didn't find the term, just pick the first window
  let maxSumIndex = 0;
  if (termFound) {
    let maxFound = 0;
    // backwards
    for (let i = windowWeights.length - 1; i >= 0; i--) {
      if (windowWeights[i] > maxFound) {
        maxFound = windowWeights[i];
        maxSumIndex = i;
      }
    }
  }

  const teaser: string[] = [];
  let startIndex = weighted[maxSumIndex][2];
  for (let i = maxSumIndex; i < maxSumIndex + windowSize; i++) {
    const word = weighted[i];
    if (startIndex < word[2]) {
      // missing text from index to start of `word`
      teaser.push(body.substring(startIndex, word[2]));
      startIndex = word[2];
    }

    // add <em/> around search terms
    if (word[1] === TERM_WEIGHT) {
      teaser.push("<b>");
    }
    startIndex = word[2] + word[0].length;
    teaser.push(body.substring(word[2], startIndex));

    if (word[1] === TERM_WEIGHT) {
      teaser.push("</b>");
    }
  }
  teaser.push("…");
  const final = teaser.join("");
  const span = elem("span", {}, {}, []);
  span.innerHTML = final;
  return span;
}
