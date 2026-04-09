import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

export interface AgentCliInfo {
  id: string;
  name: string;
  binary: string;
  installed: boolean;
  path: string | null;
}

/** Fetches available agent CLIs once on mount. */
export function useAgentClis() {
  const [agents, setAgents] = useState<AgentCliInfo[]>([]);

  useEffect(() => {
    invoke<AgentCliInfo[]>("list_agent_clis")
      .then(setAgents)
      .catch(() => setAgents([]));
  }, []);

  return agents;
}
