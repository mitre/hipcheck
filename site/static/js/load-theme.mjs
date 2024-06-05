import { themes, themeSet, themeKey } from "theme";

/**
 * Get any theme setting from localStorage.
 *
 * @return the theme string from localStorage.
 */
const getStoredTheme = function () {
  return localStorage.getItem(themeKey);
};

/**
 * Get the user's theme preference with a media query.
 *
 * @return the theme preference.
 */
const getUserPreferredTheme = function () {
  let prefersDark = window.matchMedia("(prefers-color-scheme: dark)");

  if (prefersDark) {
    return themes.DARK;
  }

  return themes.LIGHT;
};

/**
 * Set the theme by updating the site styles.
 *
 * @param theme The theme enum indicating what theme to use.
 * @return if setting the theme succeeded.
 */
const setTheme = function (theme) {
  if (theme === themes.DARK) {
    document.documentElement.classList.add("dark");
    return themeSet.YES;
  } else if (theme === themes.LIGHT) {
    document.documentElement.classList.remove("dark");
    return themeSet.YES;
  } else {
    console.error(`unexpected theme ${theme}, should be "light" or "dark"`);
    return themeSet.NO;
  }
};

(function () {
  let storedTheme = getStoredTheme();

  if (storedTheme) {
    console.debug(`Found stored theme '${storedTheme}'`);
    let result = setTheme(storedTheme);
    if (result === themeSet.YES) return;
  }

  let userPreferredTheme = getUserPreferredTheme();
  console.debug(`Found preferred theme '${userPreferredTheme}'`);
  let result = setTheme(userPreferredTheme);
  if (result === themeSet.YES) return;

  console.error("unable to set the theme, defaulting to 'light'");
  setTheme(themes.LIGHT);
})();
