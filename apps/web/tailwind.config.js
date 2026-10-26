/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        paper: "#f7f3ec",
        surface: "#fffdf9",
        ink: "#1d1a17",
        muted: "#6f675d",
        line: "#e6dfd3",
        terracotta: {
          DEFAULT: "#cc543a",
          dark: "#a9432d",
          soft: "#f6e3dc",
        },
        rust: "#cc543a",
        forest: "#2d5a27",
        ochre: "#d4a017",
        sky: "#7fb3d5",
      },
      fontFamily: {
        sans: ['"Instrument Sans"', "system-ui", "sans-serif"],
        display: ['"Fraunces"', "Georgia", "serif"],
      },
      boxShadow: {
        soft: "0 1px 2px rgba(29,26,23,0.04), 0 2px 8px rgba(29,26,23,0.04)",
        card: "0 1px 2px rgba(29,26,23,0.05), 0 8px 24px rgba(29,26,23,0.06)",
        lift: "0 2px 4px rgba(29,26,23,0.06), 0 18px 48px rgba(29,26,23,0.12)",
      },
      maxWidth: {
        page: "80rem",
      },
    },
  },
  plugins: [],
};
