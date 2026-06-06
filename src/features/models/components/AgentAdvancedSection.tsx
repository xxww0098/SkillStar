import { Terminal } from "lucide-react";
import { memo, useMemo, useState } from "react";
import { cn } from "../../../lib/utils";
import { useToolActivations } from "../hooks/useToolActivations";
import { ToolActivationPanel } from "./ToolActivationPanel";
import { ToolJsonConfigPanel } from "./ToolJsonConfigPanel";
import { AgentModelConfig } from "./providerForm/AgentModelConfig";
import { ConfigCollapseSection } from "./providerForm/ProviderConfigPrimitives";
import type { ProviderFormState } from "./providerForm/useProviderFormState";

const TOOL_DISPLAY_NAMES: Record<string, string> = {
  "claude-code": "Claude",
  codex: "Codex",
  opencode: "OpenCode",
  gemini: "Gemini CLI",
};

type AgentTab = "sync" | "models" | "disk";

export interface AgentAdvancedSectionProps {
  providerId: string;
  providerModels: string[];
  defaultModel: string;
  baseUrlOpenai: string;
  baseUrlAnthropic: string;
  isToolActive: (toolId: string) => boolean;
  form: ProviderFormState;
  /** Start expanded (e.g. tool-configs route) */
  defaultExpanded?: boolean;
  defaultTab?: AgentTab;
}

function AgentAdvancedSectionInner({
  providerId,
  providerModels,
  defaultModel,
  baseUrlOpenai,
  baseUrlAnthropic,
  isToolActive,
  form,
  defaultExpanded = false,
  defaultTab = "sync",
}: AgentAdvancedSectionProps) {
  const [expanded, setExpanded] = useState(defaultExpanded);
  const [tab, setTab] = useState<AgentTab>(defaultTab);
  const { activations } = useToolActivations(providerId);

  // Build summary showing which agents are activated for this provider
  const activationSummary = useMemo(() => {
    const activeTools = Object.entries(activations)
      .filter(([, a]) => a?.provider_id === providerId)
      .map(([toolId]) => toolId);
    if (activeTools.length === 0) return "未激活任何 Agent";
    return activeTools.map((t) => TOOL_DISPLAY_NAMES[t] || t).join(" · ");
  }, [activations, providerId]);

  const tabs: { id: AgentTab; label: string }[] = [
    { id: "sync", label: "工具同步" },
    { id: "models", label: "模型映射" },
    { id: "disk", label: "磁盘配置" },
  ];

  return (
    <ConfigCollapseSection
      id="provider-agent-advanced"
      icon={Terminal}
      title="Agent 高级配置"
      summary={activationSummary}
      expanded={expanded}
      onToggle={() => setExpanded((p) => !p)}
    >
      <div className="flex flex-wrap gap-1 rounded-lg border border-border/50 bg-background/30 p-1">
        {tabs.map((t) => (
          <button
            key={t.id}
            type="button"
            onClick={() => setTab(t.id)}
            className={cn(
              "rounded-md px-2.5 py-1 text-[11px] font-medium transition-colors",
              tab === t.id ? "bg-primary/15 text-primary" : "text-muted-foreground hover:text-foreground",
            )}
          >
            {t.label}
          </button>
        ))}
      </div>

      {tab === "sync" && (
        <ToolActivationPanel
          providerId={providerId}
          providerModels={providerModels}
          defaultModel={defaultModel}
          baseUrlOpenai={baseUrlOpenai}
          baseUrlAnthropic={baseUrlAnthropic}
          showHeader={false}
          variant="compact"
        />
      )}

      {tab === "models" && <AgentModelConfig form={form} />}

      {tab === "disk" && <ToolJsonConfigPanel providerId={providerId} isToolActive={isToolActive} embedded />}
    </ConfigCollapseSection>
  );
}

export const AgentAdvancedSection = memo(AgentAdvancedSectionInner);
