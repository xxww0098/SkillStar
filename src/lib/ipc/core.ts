/**
 * Core IPC plumbing.
 *
 * All Tauri `invoke()` calls in the app must flow through `tauriInvoke` or the
 * React Query wrappers below. This keeps a single chokepoint for:
 * - compile-time validation of command names, args, and result types
 * - consistent cache + retry behavior via React Query
 * - easy mocking in tests (see `src/test/setup.ts`)
 *
 * If a new backend command is added, register it in `./commands/*.ts` — never
 * call `invoke()` directly from feature code.
 */
import { type UseQueryOptions, useMutation, useQuery } from "@tanstack/react-query";
import { invoke, isTauri } from "@tauri-apps/api/core";

import type { TauriCommands } from "./commands";

const NOT_IN_TAURI_SHELL =
  "当前页面不在 SkillStar 桌面窗口中，无法调用后端：请启动完整应用（例如 bun run tauri dev），不要单独在浏览器中打开 Vite 的本地开发地址。";

function invokeInTauriShell<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) {
    // Outside the Tauri shell, production cannot reach the backend. In DEV we
    // serve realistic sample data (see ./devMock) so the full UI renders in a
    // plain browser for design iteration. The dynamic import keeps devMock out
    // of production bundles entirely.
    if (import.meta.env.DEV) {
      return import("./devMock").then((m) => m.devInvoke<T>(command, args));
    }
    return Promise.reject(new Error(NOT_IN_TAURI_SHELL));
  }
  return args === undefined ? invoke<T>(command) : invoke<T>(command, args);
}

type CommandKey = keyof TauriCommands;
type CommandArgs<K extends CommandKey> = TauriCommands[K]["args"];
type CommandResult<K extends CommandKey> = TauriCommands[K]["result"];
type NoArgCommand<K extends CommandKey> = CommandArgs<K> extends Record<string, never> ? K : never;
type NoArgKey = { [K in CommandKey]: NoArgCommand<K> }[CommandKey];

/**
 * Type-safe `invoke()` wrapper. Checks command name, arg shape, and return type
 * at compile time.
 *
 * ```ts
 * const skills = await tauriInvoke("list_skills");
 * const skill = await tauriInvoke("install_skill", { url: "https://..." });
 * ```
 */
export function tauriInvoke<K extends CommandKey>(
  command: K,
  ...args: CommandArgs<K> extends Record<string, never> ? [] : [CommandArgs<K>]
): Promise<CommandResult<K>> {
  if (args.length === 0) {
    return invokeInTauriShell<CommandResult<K>>(command);
  }
  return invokeInTauriShell<CommandResult<K>>(command, args[0] as Record<string, unknown>);
}

/**
 * Escape hatch for the rare case when the command name is only known at
 * runtime (e.g. streaming hooks that route between multiple commands, or
 * Tauri plugin commands like `plugin:webview|internal_toggle_devtools`).
 *
 * Prefer `tauriInvoke` wherever possible — this bypasses type checking.
 */
export function tauriInvokeDynamic<T = unknown>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invokeInTauriShell<T>(command, args);
}

/**
 * React Query wrapper for commands that take no arguments.
 *
 * ```ts
 * const { data } = useTauriQuery("list_skills");
 * ```
 */
export function useTauriQuery<K extends NoArgKey>(
  command: K,
  options?: Omit<UseQueryOptions<CommandResult<K>, Error>, "queryKey" | "queryFn">,
) {
  return useQuery<CommandResult<K>, Error>({
    queryKey: [command],
    queryFn: () => invokeInTauriShell<CommandResult<K>>(command),
    ...options,
  });
}

/**
 * React Query wrapper for commands that take arguments.
 *
 * ```ts
 * const { data } = useTauriQueryWithArgs("read_skill_content", { name: "foo" });
 * ```
 */
export function useTauriQueryWithArgs<K extends CommandKey>(
  command: K,
  args: CommandArgs<K>,
  options?: Omit<UseQueryOptions<CommandResult<K>, Error>, "queryKey" | "queryFn">,
) {
  return useQuery<CommandResult<K>, Error>({
    queryKey: [command, args],
    queryFn: () => invokeInTauriShell<CommandResult<K>>(command, args as Record<string, unknown>),
    ...options,
  });
}

/**
 * React Query mutation wrapper.
 *
 * ```ts
 * const install = useTauriMutation("install_skill");
 * await install.mutateAsync({ url: "https://..." });
 * ```
 */
export function useTauriMutation<K extends CommandKey>(command: K) {
  return useMutation<CommandResult<K>, Error, CommandArgs<K>>({
    mutationFn: (args) => invokeInTauriShell<CommandResult<K>>(command, args as Record<string, unknown>),
  });
}
