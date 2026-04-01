import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "path";
import { fileURLToPath } from "url";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;
const __dirname = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  clearScreen: false,
  build: {
    rollupOptions: {
      output: {
        manualChunks: {
          "react-vendor": ["react", "react-dom", "scheduler"],
          "motion-vendor": ["framer-motion"],
          "i18n-vendor": ["i18next", "react-i18next"],
          "tauri-vendor": [
            "@tauri-apps/api",
            "@tauri-apps/plugin-dialog",
            "@tauri-apps/plugin-process",
            "@tauri-apps/plugin-shell",
            "@tauri-apps/plugin-updater",
          ],
          "radix-vendor": [
            "@radix-ui/react-slot",
            "@radix-ui/react-switch",
          ],
          "ui-vendor": [
            "class-variance-authority",
            "clsx",
            "lucide-react",
            "sonner",
            "tailwind-merge",
          ],
        },
      },
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: [
        "**/src-tauri/**",
        "**/.agents/**",
        "**/.claude/**",
        "**/.cursor/**",
        "**/.gemini/**",
        "**/.opencode/**",
        "**/.qoder/**",
        "**/.trae/**",
      ],
    },
  },
}));
