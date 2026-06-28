/** @type {import('tailwindcss').Config} */
export default {
  darkMode: ["class"],
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        bds: {
          bg: "rgb(var(--bds-bg) / <alpha-value>)",
          surface: "rgb(var(--bds-surface) / <alpha-value>)",
          surface2: "rgb(var(--bds-surface2) / <alpha-value>)",
          border: "rgb(var(--bds-border) / <alpha-value>)",
          fg: "rgb(var(--bds-fg) / <alpha-value>)",
          muted: "rgb(var(--bds-muted) / <alpha-value>)",
          accent: "rgb(var(--bds-accent) / <alpha-value>)",
          accent2: "rgb(var(--bds-accent2) / <alpha-value>)",
          good: "rgb(var(--bds-good) / <alpha-value>)",
          warn: "rgb(var(--bds-warn) / <alpha-value>)",
          bad: "rgb(var(--bds-bad) / <alpha-value>)",
          info: "rgb(var(--bds-info) / <alpha-value>)",
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
