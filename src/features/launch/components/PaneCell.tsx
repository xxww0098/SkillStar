import { Columns2, Rows2, X } from "lucide-react";
import { memo } from "react";
import type { ModelAppId } from "../../models/components/AppCapsuleSwitcher";
import { AgentIcon } from "../../models/components/shared/ProviderIcon";
import { useModelProviders, useOpenCodeNativeProviders } from "../../models/hooks/useModelProviders";
import type { AgentCliInfo } from "../hooks/useAgentClis";
import type { PaneNode, SplitDirection } from "../hooks/useLaunchConfig";
import { OpenCodeModelSelect } from "./OpenCodeModelSelect";

const AGENT_COLORS: Record<string, string> = {
  claude: "border-orange-500/20 bg-orange-500/5 shadow-orange-500/5",
  codex: "border-green-500/20 bg-green-500/5 shadow-green-500/5",
  opencode: "border-purple-500/20 bg-purple-500/5 shadow-purple-500/5",
  gemini: "border-blue-500/20 bg-blue-500/5 shadow-blue-500/5",
};

const AGENT_HEX_COLORS: Record<string, string> = {
  claude: "#F97316",
  codex: "#10B981",
  opencode: "#A855F7",
  gemini: "#3B82F6",
};

// SVG component removed in favor of shared AgentIcon
interface PaneCellProps {
  pane: PaneNode;
  agents: AgentCliInfo[];
  isMulti: boolean;
  canRemove: boolean;
  onSplit: (paneId: string, direction: SplitDirection) => void;
  onAssign: (paneId: string, agentId: string, providerId?: string, providerName?: string, modelId?: string) => void;
  onRemove: (paneId: string) => void;
}

const ProviderSelect = memo(function ProviderSelect({
  pane,
  onAssign,
}: {
  pane: PaneNode;
  onAssign: (paneId: string, agentId: string, providerId?: string, providerName?: string, modelId?: string) => void;
}) {
  const DEFAULT_PROVIDER_VALUE = "__default__";
  const isOpencode = pane.agentId === "opencode";
  const genericProviders = useModelProviders(pane.agentId as ModelAppId);
  const opencodeProviders = useOpenCodeNativeProviders();
  const providers = isOpencode ? opencodeProviders : genericProviders;

  const hasAssignedProvider =
    !!pane.providerId && providers.sortedProviders.some((provider) => provider.id === pane.providerId);
  const selectedProviderId = hasAssignedProvider ? pane.providerId : DEFAULT_PROVIDER_VALUE;

  return (
    <select
      className="mt-1 text-[10px] bg-background/80 border border-border/50 rounded-md px-1.5 py-0.5 text-muted-foreground outline-none cursor-pointer hover:border-primary/40 focus:border-primary/60 transition-colors shadow-sm backdrop-blur-md appearance-none text-center min-w-[90px]"
      value={selectedProviderId}
      onChange={(e) => {
        const pId = e.target.value;
        if (pId === DEFAULT_PROVIDER_VALUE) {
          onAssign(pane.id, pane.agentId, undefined, undefined, pane.modelId);
          return;
        }
        const pName = providers.sortedProviders.find((p) => p.id === pId)?.name;
        onAssign(pane.id, pane.agentId, pId, pName, pane.modelId);
      }}
    >
      <option value={DEFAULT_PROVIDER_VALUE}>Default</option>
      {providers.sortedProviders.map((p) => {
        const displayName = p.name.replace(/^Google \((.+)\)$/, "$1");
        return (
          <option key={p.id} value={p.id}>
            {displayName}
          </option>
        );
      })}
    </select>
  );
});

export const PaneCell = memo(function PaneCell({
  pane,
  agents,
  isMulti,
  canRemove,
  onSplit,
  onAssign,
  onRemove,
}: PaneCellProps) {
  const agentInfo = agents.find((a) => a.id === pane.agentId);
  const hasAgent = pane.agentId && agentInfo;
  const isOpencode = pane.agentId === "opencode";

  const colorClass = hasAgent
    ? (AGENT_COLORS[pane.agentId] ?? "border-border bg-card shadow-sm")
    : "border-border/30 bg-muted/20 border-dashed hover:border-border/60";

  return (
    <div
      className={`relative flex flex-col items-center justify-center w-full h-full rounded-xl border transition-all duration-200 overflow-hidden shadow-inner group ${colorClass}`}
      style={{ minHeight: "140px", minWidth: "120px" }}
    >
      {/* Background generic grid effect for premium feel */}
      <div className="absolute inset-0 opacity-[0.03] pointer-events-none bg-[url('data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iMjAiIGhlaWdodD0iMjAiIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyI+PGNpcmNsZSBjeD0iMiIgY3k9IjIiIHI9IjEiIGZpbGw9IiMwMDAiLz48L3N2Zz4=')] [mask-image:linear-gradient(to_bottom,white,transparent)]" />

      {/* Agent assignment */}
      {hasAgent ? (
        <div className="relative z-10 flex flex-col items-center gap-3 px-3 py-6">
          <div className="flex h-8 w-8 items-center justify-center rounded-lg shadow-sm border border-border/40 bg-background/50">
            <AgentIcon appId={pane.agentId} color={AGENT_HEX_COLORS[pane.agentId]} size="w-5 h-5" />
          </div>

          <div className="flex flex-col items-center gap-0.5">
            <select
              className="text-xs font-semibold tracking-wide text-foreground shadow-sm bg-transparent border-none outline-none cursor-pointer hover:text-primary transition-colors appearance-none text-center"
              value={agentInfo.id}
              onChange={(e) => onAssign(pane.id, e.target.value)}
              title="切换智能体"
            >
              {agents
                .filter((a) => a.installed)
                .map((a) => (
                  <option key={a.id} value={a.id} className="text-foreground bg-background font-normal">
                    {a.name}
                  </option>
                ))}
            </select>

            {!isOpencode && <ProviderSelect pane={pane} onAssign={onAssign} />}

            {/* OpenCode model selector */}
            {isOpencode && <OpenCodeModelSelect pane={pane} onAssign={onAssign} />}
          </div>
        </div>
      ) : (
        <div className="relative z-10 flex flex-col items-center gap-3 py-6">
          <select
            className="text-xs bg-background/50 border border-border/50 shadow-sm rounded-lg px-3 py-1.5 text-muted-foreground hover:text-foreground appearance-none cursor-pointer hover:border-primary/50 hover:bg-background focus:border-primary/60 focus:outline-none transition-all min-w-[120px] text-center backdrop-blur-md"
            value=""
            onChange={(e) => onAssign(pane.id, e.target.value)}
          >
            <option value="" disabled>
              + 关联智能体
            </option>
            {agents
              .filter((a) => a.installed)
              .map((a) => (
                <option key={a.id} value={a.id}>
                  {a.name}
                </option>
              ))}
          </select>
        </div>
      )}

      {/* Action buttons */}
      <div className="absolute top-2 right-2 flex items-center gap-1 z-20 rounded-md border border-border/40 bg-background/85 backdrop-blur-sm p-0.5 shadow-sm opacity-95 hover:opacity-100 transition-opacity">
        {isMulti && (
          <>
            <button
              type="button"
              onClick={() => onSplit(pane.id, "h")}
              className="p-1 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors cursor-pointer"
              title="水平分割"
            >
              <Columns2 className="w-3.5 h-3.5" />
            </button>
            <button
              type="button"
              onClick={() => onSplit(pane.id, "v")}
              className="p-1 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors cursor-pointer"
              title="垂直分割"
            >
              <Rows2 className="w-3.5 h-3.5" />
            </button>
            {canRemove && <div className="w-[1px] h-3 bg-border mx-0.5" />}
          </>
        )}
        {canRemove && (
          <button
            type="button"
            onClick={() => onRemove(pane.id)}
            className="p-1 rounded hover:bg-destructive/10 text-muted-foreground hover:text-destructive transition-colors cursor-pointer"
            title="删除面板"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        )}
      </div>
    </div>
  );
});
