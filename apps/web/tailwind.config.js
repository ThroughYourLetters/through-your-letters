/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        paper: "#faf8f4",
        surface: "#ffffff",
        ink: "#1d1a17",
        muted: "#6b655d",
        line: "#ebe5dc",
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
        sans: ['"Inter"', "system-ui", "sans-serif"],
        display: ['"Bricolage Grotesque"', '"Inter"', "system-ui", "sans-serif"],
        serif: ['"Fraunces"', "Georgia", "serif"],
      },
      boxShadow: {
        soft: "0 1px 2px rgba(29,26,23,0.04), 0 2px 8px rgba(29,26,23,0.04)",
        card: "0 1px 2px rgba(29,26,23,0.05), 0 8px 24px rgba(29,26,23,0.06)",
        lift: "0 2px 4px rgba(29,26,23,0.06), 0 18px 48px rgba(29,26,23,0.12)",
      },
      keyframes: {
        pop: {
          "0%": { transform: "scale(0.6) rotate(-12deg)", opacity: "0" },
          "100%": { transform: "scale(1) rotate(0)", opacity: "1" },
        },
        float: {
          "0%, 100%": { transform: "translateY(0)" },
          "50%": { transform: "translateY(-6px)" },
        },
      },
      animation: {
        float: "float 4s ease-in-out infinite",
      },
      maxWidth: {
        page: "80rem",
      },
    },
  },
  plugins: [],
};
