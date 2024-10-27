/** @type {import('tailwindcss').Config} */

const defaultTheme = require("tailwindcss/defaultTheme");

module.exports = {
  // Track the template files for the purpose of selecting
  // what CSS is actually used.
  content: ["./templates/**/*.html", "./public/**/*.html", "./content/**/*.md"],
  theme: {
    extend: {
      fontFamily: {
        // Use Inter as the default font, but otherwise use
        // the default sans-serif font.
        sans: [
          "'Inter', ui-sans-serif, system-ui, sans-serif, 'Apple Color Emoji', 'Segoe UI Emoji', 'Segoe UI Symbol', 'Noto Color Emoji'",
          {
            fontFeatureSettings:
              '"calt", "dlig", "case", "ss03", "cv01", "cv10"',
          },
        ],
        mono: ["IBM Plex Mono", ...defaultTheme.fontFamily.mono],
      },
    },
  },
  plugins: [
    // Use the standard typography plugin
    require("@tailwindcss/typography"),
  ],
  // Use a selector to set dark mode.
  darkMode: "selector",
};
