/**
 * Lazily fetches how a skill is physically deployed to each enabled agent
 * (symlink vs directory copy vs missing) via `get_skill_deploy_status`.
 *
 * Fetches once whenever `skillName` becomes non-null (i.e. the detail panel
 * opens for an installed skill) — no polling. Pass `null` to skip fetching.
 */
import { useEffect, useState } from "react";
import { type AgentDeployStatus, tauriInvoke } from "../../../lib/ipc";

export function useDeployStatus(skillName: string | null): AgentDeployStatus[] | null {
  const [status, setStatus] = useState<AgentDeployStatus[] | null>(null);

  useEffect(() => {
    setStatus(null);
    if (!skillName) return;
    let cancelled = false;
    tauriInvoke("get_skill_deploy_status", { skillName })
      .then((rows) => {
        if (!cancelled) setStatus(rows);
      })
      .catch((e) => {
        if (import.meta.env.DEV) console.warn("[useDeployStatus] Failed to fetch deploy status:", e);
        if (!cancelled) setStatus(null);
      });
    return () => {
      cancelled = true;
    };
  }, [skillName]);

  return status;
}

/** Rows worth surfacing: copy fallbacks and dangling links. Healthy links stay silent. */
export function degradedDeploys(rows: AgentDeployStatus[] | null): AgentDeployStatus[] {
  return (rows ?? []).filter((row) => row.kind === "copy" || (row.kind === "link" && !row.link_alive));
}
