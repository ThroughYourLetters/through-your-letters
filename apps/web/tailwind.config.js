/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        paper: '#f8f5f0',
        ink: '#1a1a1a',
        terracotta: '#cc543a',
        forest: '#2d5a27',
        ochre: '#d4a017',
        sky: '#7fb3d5',
      },
    },
  },
  plugins: [],
}