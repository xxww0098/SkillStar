/// <reference types="vitest/config" />

import path from "node:path";
import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.{test,spec}.{ts,tsx}"],
    css: false,
    // Vitest otherwise forks one jsdom environment per core (18 here). That
    // over-subscribes the machine: the run is not CPU-bound (34% during the
    // capped run) but every worker still holds a full jsdom + module graph, so
    // timing-sensitive assertions start missing their windows. Measured on this
    // repo: uncapped, `DetailPanel.test.tsx` failed 5 full runs in a row and
    // the suite took 7m22s; capped at 4 it is 134/134 files / 917/917 tests
    // green in 6m30s. Lower is not automatically slower here — capping removed
    // more contention overhead than it added queueing.
    //
    // Treating this as the fix rather than retrying the flake: `--maxWorkers=2`
    // turns the same failure green, and raising `asyncUtilTimeout` (5000 ->
    // 10000) did not, so the cause is worker count, not wait length.
    maxWorkers: 4,
    // The heaviest component tests take ~2.5s solo; under a full-suite run on a
    // loaded machine they have been measured at 16-17s per file, so the 5s
    // default flakes on contention rather than on real failures. Pairs with the
    // raised `asyncUtilTimeout` in `src/test/setup.ts`.
    testTimeout: 15_000,
  },
});
