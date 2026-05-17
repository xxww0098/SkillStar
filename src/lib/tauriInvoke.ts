/**
 * Re-export shim. The typed Tauri IPC layer lives in `./ipc/`. Prefer importing
 * directly from `../lib/ipc` in new code.
 */
export {
  tauriInvoke,
  tauriInvokeDynamic,
  useTauriMutation,
  useTauriQuery,
  useTauriQueryWithArgs,
} from "./ipc";

export type { PatrolStatus, RepoCacheInfo, TauriCommands } from "./ipc";
