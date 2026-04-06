import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import path from "path";
import { fileURLToPath } from "url";
import { defineConfig } from "vite";

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
        manualChunks: (id) => {
          if (
            id.includes("node_modules/react/") ||
            id.includes("node_modules/react-dom/") ||
            id.includes("node_modules/scheduler/")
          )
            return "react-vendor";
          if (id.includes("node_modules/framer-motion/")) return "motion-vendor";
          if (id.includes("node_modules/i18next/") || id.includes("node_modules/react-i18next/")) return "i18n-vendor";
          if (id.includes("node_modules/@tauri-apps/")) return "tauri-vendor";
          if (id.includes("node_modules/@radix-ui/")) return "radix-vendor";
          if (
            id.includes("node_modules/class-variance-authority/") ||
            id.includes("node_modules/clsx/") ||
            id.includes("node_modules/lucide-react/") ||
            id.includes("node_modules/sonner/") ||
            id.includes("node_modules/tailwind-merge/")
          )
            return "ui-vendor";
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
