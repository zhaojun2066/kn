import type { Config } from "tailwindcss";

export default {
  content: ["./src/**/*.{ts,tsx}", "./index.html"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        app: {
          // All colors driven by CSS variables — theme switching is pure CSS
          bg:           "var(--app-bg)",
          panel:        "var(--app-panel)",
          sidebar:      "var(--app-sidebar)",
          input:        "var(--app-input)",
          hover:        "var(--app-hover)",
          active:       "var(--app-active)",
          selected:     "var(--app-selected)",
          focus:        "var(--app-focus)",
          border:       "var(--app-border)",
          "border-light": "var(--app-border-light)",
          text:         "var(--app-text)",
          "text-dim":   "var(--app-text-dim)",
          "text-muted": "var(--app-text-muted)",
          accent:       "var(--app-accent)",
          "accent-dim": "var(--app-accent-dim)",
          amber:        "var(--app-amber)",
          "amber-glow": "var(--app-amber-glow)",
          green:        "var(--app-green)",
          "green-bg":   "var(--app-green-bg)",
          red:          "var(--app-red)",
          "red-bg":     "var(--app-red-bg)",
          orange:       "var(--app-orange)",
          "orange-bg":  "var(--app-orange-bg)",
          blue:         "var(--app-blue)",
          purple:       "var(--app-purple)",
          teal:         "var(--app-teal)",
          toolbar:      "var(--app-toolbar)",
          statusbar:    "var(--app-statusbar)",
          subtle:       "var(--app-subtle)",
          "cmd-bg":     "var(--app-cmd-bg)",
          "cmd-header": "var(--app-cmd-header)",
          glow:         "var(--app-glow)",
          "glow-amber": "var(--app-glow-amber)",
          "glow-red":   "var(--app-glow-red)",
        },
      },
      fontFamily: {
        sans: ['"JetBrains Mono"', '"Fira Code"', '"Cascadia Code"', "Consolas", "monospace"],
        mono: ['"JetBrains Mono"', '"Fira Code"', '"Cascadia Code"', "Consolas", "monospace"],
      },
      // 使用 rem 单位（基准 13px），配合 useFontScale 实现全局字体缩放
      fontSize: {
        "3xs":  ["0.692rem", "1rem"],       // 9px / 13px @13px base
        "2xs":  ["0.769rem", "1.077rem"],   // 10px / 14px @13px base
        xs:     ["0.846rem", "1.231rem"],   // 11px / 16px
        sm:     ["0.923rem", "1.385rem"],   // 12px / 18px
        base:   ["1rem",    "1.538rem"],    // 13px / 20px
        lg:     ["1.077rem", "1.538rem"],   // 14px / 20px
        xl:     ["1.231rem", "1.846rem"],   // 16px / 24px
        "2xl":  ["1.385rem", "2rem"],       // 18px / 26px
      },
      spacing: {
        "0.5": "2px",
        "1":   "4px",
        "1.5": "6px",
        "2":   "8px",
        "2.5": "10px",
        "3":   "12px",
        "3.5": "14px",
        "4":   "16px",
        "5":   "20px",
        "6":   "24px",
        "8":   "32px",
      },
      borderRadius: {
        DEFAULT: "2px",
        none: "0px",
        sm: "0px",
        md: "0px",
        lg: "0px",
      },
      boxShadow: {
        panel: "var(--shadow-panel)",
        dialog: "var(--shadow-dialog)",
        dropdown: "0 4px 16px rgba(0,0,0,0.6)",
        tooltip: "0 2px 8px rgba(0,0,0,0.6)",
        glow: "var(--shadow-glow)",
        "glow-amber": "var(--shadow-glow-amber)",
      },
      transitionDuration: {
        fast: "100ms",
        normal: "150ms",
      },
      keyframes: {
        "cursor-blink": {
          "0%, 100%": { opacity: "1" },
          "50%": { opacity: "0" },
        },
        "scanline": {
          "0%": { transform: "translateY(0)" },
          "100%": { transform: "translateY(4px)" },
        },
        "glow-pulse": {
          "0%, 100%": { boxShadow: "0 0 8px rgba(61,252,61,0.08)" },
          "50%": { boxShadow: "0 0 16px rgba(61,252,61,0.18)" },
        },
        "scan-blink": {
          "0%, 100%": { boxShadow: "none" },
          "50%": { boxShadow: "0 0 0 2px var(--app-accent), 0 0 18px var(--app-glow)" },
        },
      },
      animation: {
        "cursor-blink": "cursor-blink 1s step-end infinite",
        "scanline": "scanline 0.1s linear infinite",
        "glow-pulse": "glow-pulse 2s ease-in-out infinite",
        "scan-blink": "scan-blink 2.5s ease-in-out infinite",
      },
    },
  },
  plugins: [],
} satisfies Config;
