/**
 * The agent registry, served by the backend.
 *
 * `agentRegistry.ts` still owns the presentational extras (icon id, tagline,
 * install docs URL) that have no backend counterpart. Everything the writers
 * actually act on — binding kind, required wire protocol, config files, and the
 * role list — comes from here, so a role added to `tool_sync/agents.rs` shows up
 * in the panel without a second edit on this side.
 */
import { useQuery } from "@tanstack/react-query";
import { tauriInvoke } from "../../../lib/ipc";
import type { DroppedRole } from "../../../types";
import type { AgentDescriptorDto } from "../../../types/generated/AgentDescriptorDto";
import type { RoleDefDto } from "../../../types/generated/RoleDefDto";
import { modelsKeys } from "./keys";

export type { AgentDescriptorDto, RoleDefDto };

/** Every agent the backend can write, in registry order. */
export function useAgentDescriptors() {
  return useQuery({
    queryKey: modelsKeys.agentDescriptors(),
    queryFn: () => tauriInvoke("list_agent_descriptors") as Promise<AgentDescriptorDto[]>,
    // The registry is compiled in: it cannot change while the app is running.
    staleTime: Infinity,
  });
}

/** One agent's descriptor, or `null` while the registry is still loading. */
export function useAgentDescriptor(toolId: string): AgentDescriptorDto | null {
  const { data } = useAgentDescriptors();
  return data?.find((agent) => agent.id === toolId) ?? null;
}

/**
 * Park the roles a write skipped so the panel can mark the rows.
 *
 * Recomputing the skip conditions on this side would mean keeping a copy of the
 * writer's rules in the renderer, and the copy would be wrong the first time a
 * writer changed. The backend already decided; this only remembers.
 */
export function useRoleDrops(toolId: string): DroppedRole[] {
  const { data } = useQuery({
    queryKey: modelsKeys.roleDrops(toolId),
    // Nothing to fetch: the value only ever arrives from a sync result.
    queryFn: () => [] as DroppedRole[],
    staleTime: Infinity,
    enabled: Boolean(toolId),
  });
  return data ?? [];
}
