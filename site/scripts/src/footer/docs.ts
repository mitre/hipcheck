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
    // Swallow these errors.
    return;
  }
  

  $toggles.forEach(($toggle) => {  //loop though toggle elements
    // Get the icon inside the toggle.
    const $toggleIcon = $toggle.querySelector(
      ".toggle-icon use",
    ) as HTMLElement;
    if ($toggleIcon === null) {
      console.log(`no icon found for toggle: '${$toggle}'`);
      return;
    }
    //Get ID 
    const sectionId = $toggle.dataset.target;
    if (!sectionId) { //check if empty
      console.log(`No section ID found for toggle: '${$toggle}'`);
      return;
    }
    const section = document.getElementById(sectionId);
    if (!section) {
      console.log(`No section found for ID: '${sectionId}'`);
      return;
    }

    //Restore state from local storaage
    const isHidden = localStorage.getItem(`docs-nav-${sectionId}`) === "true";
    if (isHidden) { //if hidden hen collapse 
      section.classList.add("hidden"); //adds to class 
      updateIcon($toggleIcon, "chevron-right", "chevron-down");  //reflects toggle state
    } else {
      section.classList.remove("hidden");
      updateIcon($toggleIcon, "chevron-down", "chevron-right");
    }

    //Toggle event listener for clicks
    $toggle.addEventListener("click", (e) => { //listens for clicks on the toggle button
      e.preventDefault(); //cancels event - default action will not occur

      // Find the subsection list associated with the current toggle.
      const $parent = $toggle.parentElement?.parentElement;
      if ($parent === null || $parent === undefined) return; //if not found exit
      const $subsection = $parent.querySelector(  //find subsection from parent
        ".docs-nav-section",
      ) as HTMLElement;
      if ($subsection === null) return;

      // Hide or show it.
      $subsection.classList.toggle("hidden");
      //will update based on visibility
      if (sectionIsHidden($subsection)) {
        updateIcon($toggleIcon, "chevron-right", "chevron-down");
      } else {
        updateIcon($toggleIcon, "chevron-down", "chevron-right");

      }  
      //save state to local storage 
      localStorage.setItem(`docs-nav-${sectionId}`, sectionIsHidden($subsection).toString());


      
    });
  });
}


//check if hidden using ture or false
function sectionIsHidden($subsection: HTMLElement): boolean {
  return $subsection.classList.contains("hidden");
}
