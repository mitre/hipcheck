import { updateIcon } from "../util/icon.ts";
import { querySelectorAll } from "../util/web.ts";

/**
 * Sets up the collapse/expand functionality for docs navigation.
 */
export function setupDocsNav() {
  let $toggles;

  try {
    $toggles = querySelectorAll(".docs-nav-toggle");
  } catch (_e) {
    return;
  }

  $toggles.forEach(($toggle) => {
    const $toggleIcon = $toggle.querySelector(
      ".toggle-icon use",
    ) as HTMLElement;
    if ($toggleIcon === null) {
      console.log(`no icon found for toggle: '${$toggle}'`);
      return;
    }

    const section = sectionForToggle($toggle);
    if (section === null) {
      return;
    }

    if (getSectionHidden(section)) {
      section.$sectionList.classList.add("hidden");
      updateIcon($toggleIcon, "chevron-right", "chevron-down");
    } else {
      section.$sectionList.classList.remove("hidden");
      updateIcon($toggleIcon, "chevron-down", "chevron-right");
    }

    $toggle.addEventListener("click", (e) => {
      e.preventDefault();

      section.$sectionList.classList.toggle("hidden");

      if (sectionIsHidden(section)) {
        updateIcon($toggleIcon, "chevron-right", "chevron-down");
      } else {
        updateIcon($toggleIcon, "chevron-down", "chevron-right");
      }

      setSectionHidden(section);
    });
  });
}

type Section = {
  // The outer section containing the toggle.
  $section: HTMLElement;
  // The toggle used to trigger showing or hiding the section.
  $toggle: HTMLElement;
  // The actual list being hidden or shown.
  $sectionList: HTMLElement;
};

function sectionForToggle($toggle: HTMLElement): Section | null {
  const $section = $toggle.parentElement?.parentElement;
  if ($section === null || $section === undefined) {
    console.log(`No section found for toggle '${$toggle}'`);
    return null;
  }

  const $sectionList = $section.querySelector(
    ".docs-nav-section",
  ) as HTMLElement;

  if ($sectionList === null) {
    console.log(`No section list found for toggle '${$toggle}'`);
    return null;
  }

  return {
    $section,
    $toggle,
    $sectionList,
  };
}

function sectionIsHidden(section: Section): boolean {
  return section.$sectionList.classList.contains("hidden");
}

function getSectionHidden(section: Section): boolean {
  const sectionId = getSectionId(section);
  return localStorage.getItem(`docs-nav-${sectionId}`) === "true";
}

function setSectionHidden(section: Section) {
  const sectionId = getSectionId(section);
  localStorage.setItem(
    `docs-nav-${sectionId}`,
    sectionIsHidden(section).toString(),
  );
}

function getSectionId(section: Section): string | undefined {
  return section.$toggle.dataset.section?.replace(/\s/, "-").toLowerCase();
}
