/**
 * DEV-ONLY browser fallback for the Tauri IPC layer.
 *
 * When the frontend runs OUTSIDE the Tauri shell (e.g. plain `vite` opened in a
 * browser for UI iteration), production rejects every `invoke()`. That makes the
 * whole app unusable in the browser. In DEV builds we instead serve realistic
 * sample data here so every screen renders populated — enabling fast visual
 * design work without a full `tauri dev` rebuild.
 *
 * This module is imported dynamically and ONLY from the `import.meta.env.DEV`
 * branch in `../core.ts`, so it is dead-code-eliminated from production bundles
 * and is NEVER reachable inside the real Tauri shell. It must not be used in
 * tests (tests mock at their own layer).
 *
 * The handler table is sharded by domain: each `./<domain>.ts` module exports a
 * `*_HANDLERS` fragment (with its sample data inline or in a sibling
 * `<domain>Data.ts`), and this entry merges them. `mergeHandlerFragments`
 * throws on duplicate command keys, so two fragments can never silently mock
 * the same command (see ./shared.ts).
 */

import { APP_SHELL_HANDLERS } from "./appShell";
import { GITHUB_HANDLERS } from "./github";
import { LEARNING_HANDLERS } from "./learning";
import { MARKETPLACE_HANDLERS } from "./marketplace";
import { MCP_HANDLERS } from "./mcp";
import { MODELS_HANDLERS } from "./models";
import { SHARED_CHANNEL_HANDLERS } from "./sharedChannels";
import { SETTINGS_HANDLERS } from "./settings";
import { mergeHandlerFragments } from "./shared";
import { SKILLS_HANDLERS } from "./skills";
import { SSH_HANDLERS } from "./ssh";
import { USAGE_HANDLERS } from "./usage";

const HANDLERS = mergeHandlerFragments([
  APP_SHELL_HANDLERS,
  SKILLS_HANDLERS,
  LEARNING_HANDLERS,
  MARKETPLACE_HANDLERS,
  MCP_HANDLERS,
  MODELS_HANDLERS,
  SETTINGS_HANDLERS,
  GITHUB_HANDLERS,
  USAGE_HANDLERS,
  SSH_HANDLERS,
  SHARED_CHANNEL_HANDLERS,
]);

/**
 * Resolve a mocked command. Known commands return realistic sample data; unknown
 * commands resolve `undefined` (rather than rejecting) so unmocked reads degrade
 * to empty state and void mutations no-op, without flooding the console.
 */
export async function devInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  // Small delay so loading skeletons are exercised during UI iteration.
  await new Promise((r) => setTimeout(r, 90));
  const handler = HANDLERS[command];
  if (handler) {
    return handler(args ?? {}) as T;
  }
  // Unmocked commands resolve to an empty array — safe for the dominant
  // "list" read pattern (`.length` / `.map` / `for..of`) so unmocked screens
  // degrade to empty state instead of crashing. Object-returning commands that
  // need a richer shape are mocked explicitly in a domain fragment. Logged at
  // `warn` (not `debug`) so gaps are visible by default instead of requiring
  // verbose console filters; see ../devMockCoverage.test.ts for the automated
  // coverage check against src/lib/ipc/commands/*.ts (KNOWN_MISSING_MOCKS
  // there tracks today's known gaps, including this one falling through to
  // `[]`).
  if (import.meta.env.DEV) {
    // eslint-disable-next-line no-console
    console.warn(`[devMock] unmocked command "${command}" → []`, args ?? {});
  }
  return [] as unknown as T;
}
