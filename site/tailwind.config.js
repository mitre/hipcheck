/** @type {import('tailwindcss').Config} */

const defaultTheme = require("tailwindcss/defaultTheme");

module.exports = {
  // Track the template files for the purpose of selecting
  // what CSS is actually used.
  content: ["./templates/**/*.html"],
  theme: {
    extend: {
      fontFamily: {
        // Use Inter as the default font, but otherwise use
        // the default sans-serif font.
        sans: ['"Inter"', ...defaultTheme.fontFamily.sans],
      },
      backgroundImage: {
        body: "url('/img/bg.png')",
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
