import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      fontFamily: {
        sans: ["Aptos", "Segoe UI Variable", "Segoe UI", "Noto Sans", "sans-serif"],
        mono: ["Cascadia Code", "JetBrains Mono", "SFMono-Regular", "Consolas", "monospace"]
      },
      borderRadius: {
        app: "8px"
      },
      boxShadow: {
        quiet: "0 18px 60px rgba(12, 18, 31, 0.12)"
      }
    }
  },
  plugins: []
} satisfies Config;
