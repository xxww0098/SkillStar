import { listen } from "@tauri-apps/api/event";

import { tauriInvoke } from "./ipc";
import type { GitOperationProgress, InstallStage, Skill } from "../types";

export interface InstallSkillParams {
  url: string;
  name?: string;
  agentId?: string;
}

/**
 * Install a skill while surfacing the backend's stage events for its session.
 *
 * The subscribe-then-invoke shape mirrors `ImportModal`'s progress wiring:
 * a fresh `sessionId` is generated here, the backend binds it to the install
 * command, and `skillstar://git-progress` events for that session drive the
 * `onStage` callback. In browser development there is no native event bus, so
 * a failed subscription degrades to a silent install — the invoke still runs.
 */
export async function installSkillWithProgress(
  params: InstallSkillParams,
  onStage?: (stage: InstallStage, skill: string | undefined) => void,
): Promise<Skill> {
  const sessionId = crypto.randomUUID();
  let stopListening: (() => void) | undefined;
  try {
    stopListening = await listen<GitOperationProgress>("skillstar://git-progress", ({ payload }) => {
      if (payload.session_id !== sessionId || !payload.stage) return;
      onStage?.(payload.stage, payload.skill);
    });
  } catch {
    // Browser development mode has no native event bus.
  }
  try {
    return await tauriInvoke("install_skill", { ...params, sessionId });
  } finally {
    stopListening?.();
  }
}
