/**
 * Hub-level connection-probe cache for the agent cards. Probes each bound
 * (toolId, providerId) pair ONCE on mount/rebind via test_provider_connection
 * (same behavior the old HealthBar strip had) and supports manual retest by
 * clicking a card's status pill. Instantiate once in ModelsHub and pass the
 * returned slice getters down so all surfaces read the same results.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { tauriInvoke } from "../../../lib/ipc";
import type { ConnectionTestResult, ProviderEntryFlat, ToolActivationsMap } from "../../../types";
import { PROVIDER_AGENTS, providerCompatibleWithAgent } from "../lib/agentRegistry";

export interface AgentHealth {
  results: Record<string, ConnectionTestResult | null>;
  testing: Record<string, boolean>;
  retest: (toolId: string) => void;
}

export function useAgentHealth(providers: ProviderEntryFlat[], toolActivations: ToolActivationsMap): AgentHealth {
  const [results, setResults] = useState<Record<string, ConnectionTestResult | null>>({});
  const [testing, setTesting] = useState<Record<string, boolean>>({});
  const probedKeys = useRef<Set<string>>(new Set());

  const targets = useMemo(
    () =>
      PROVIDER_AGENTS.map((agent) => {
        const activation = toolActivations[agent.toolId] ?? null;
        const provider = activation?.provider_id
          ? (providers.find((p) => p.id === activation.provider_id) ?? null)
          : null;
        return { agent, activation, provider };
      }),
    [providers, toolActivations],
  );

  const runTest = useCallback(
    async (toolId: string) => {
      const target = targets.find((t) => t.agent.toolId === toolId);
      if (!target?.provider) return;
      const { agent, activation, provider } = target;
      const url = agent.requiredUrlField === "anthropic" ? provider.base_url_anthropic : provider.base_url_openai;
      if (!url || !provider.api_key) return;
      setTesting((prev) => ({ ...prev, [toolId]: true }));
      try {
        const result = await tauriInvoke("test_provider_connection", {
          baseUrl: url,
          apiKey: provider.api_key,
          model: activation?.model ?? provider.default_model ?? "",
          format: agent.requiredUrlField === "anthropic" ? "anthropic" : "openai",
        });
        setResults((prev) => ({ ...prev, [toolId]: result }));
      } catch (err) {
        setResults((prev) => ({
          ...prev,
          [toolId]: { status: "network_error", error: err instanceof Error ? err.message : String(err) },
        }));
      } finally {
        setTesting((prev) => ({ ...prev, [toolId]: false }));
      }
    },
    [targets],
  );

  // Auto-probe once per (toolId, providerId) binding.
  useEffect(() => {
    for (const { agent, provider } of targets) {
      if (!provider || !providerCompatibleWithAgent(agent, provider)) continue;
      const key = `${agent.toolId}:${provider.id}`;
      if (probedKeys.current.has(key)) continue;
      probedKeys.current.add(key);
      void runTest(agent.toolId);
    }
  }, [targets, runTest]);

  const retest = useCallback((toolId: string) => void runTest(toolId), [runTest]);

  return { results, testing, retest };
}
