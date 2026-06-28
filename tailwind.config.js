/** @type {import('tailwindcss').Config} */
export default {
  darkMode: ["class"],
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        bds: {
          bg: "#0b0d10",
          surface: "#121519",
          surface2: "#171b21",
          border: "#232932",
          fg: "#e7ecf2",
          muted: "#8b97a6",
          accent: "#ff6a2b",
          accent2: "#ffb02e",
          good: "#39d98a",
          warn: "#ffb02e",
          bad: "#ff5d5d",
          info: "#4ea8ff",
        },
      },
      fontFamily: {
        sans: [
          "Inter",
          "-apple-system",
          "BlinkMacSystemFont",
          "Segoe UI",
          "system-ui",
          "sans-serif",
        ],
        mono: ["SFMono-Regular", "ui-monospace", "Menlo", "monospace"],
      },
      borderRadius: {
        lg: "12px",
        md: "9px",
        sm: "6px",
      },
      keyframes: {
        "fade-in": {
          from: { opacity: "0", transform: "translateY(6px)" },
          to: { opacity: "1", transform: "translateY(0)" },
        },
        shimmer: {
          "100%": { transform: "translateX(100%)" },
        },
        "pulse-glow": {
          "0%,100%": { opacity: "0.5" },
          "50%": { opacity: "1" },
        },
      },
      animation: {
        "fade-in": "fade-in 0.35s cubic-bezier(0.22,1,0.36,1)",
        shimmer: "shimmer 1.6s infinite",
        "pulse-glow": "pulse-glow 2s ease-in-out infinite",
      },
    },
  },
  plugins: [require("tailwindcss-animate")],
};
