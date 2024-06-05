import { themes, themeKey } from "theme";

(function () {
  document.getElementById("toggle-darkmode").addEventListener("click", (e) => {
    let containsDark = document.documentElement.classList.toggle("dark");

    if (containsDark) {
      localStorage.setItem(themeKey, themes.DARK);
    } else {
      localStorage.setItem(themeKey, themes.LIGHT);
    }

    e.preventDefault();
  });
})();
