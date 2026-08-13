import { Cable, Unplug } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { ProviderBrandIcon } from "../../../../../../components/shared/ProviderBrandIcon";
import { Button } from "../../../../../../components/ui/button";
import { cn } from "../../../../../../lib/utils";
import type { ProviderEntryFlat } from "../../../../../../types";
import { AgentToolIcon } from "../../../shared/AgentToolIcon";
import type { AgentDescriptor } from "../../../../lib/agentRegistry";
import { activeEntry, bindsProvider } from "../../../../lib/toolBinding";
import { EditorPage } from "../EditorPage";
import { DeleteConfirmModal } from "../PrototypeOverlays";
import { StateDump } from "../StateDump";
import type { PrototypeHubData } from "../types";
import {
  ClaudeMappingPanel,
  emptyClaudeMap,
  mappingFillCount,
  seedClaudeMap,
  type ClaudeMapState,
} from "../matrix/rich/ClaudeMappingPanel";
import { isCompatible, providerModels } from "../matrix/rich/RichMatrixShell";

/**
 * D3 — Dual-rail
 * Left: Provider gallery (identity / edit). Right: one Agent's binding inspector
 * + additive panel. Cross-agent overview is a compact status strip, not a matrix.
 */
export function VariantD3({ data }: { data: PrototypeHubData }) {
  const [maps, setMaps] = useState<Record<string, ClaudeMapState>>({});
  const [focusProviderId, setFocusProviderId] = useState<string | null>(null);
  const agent = data.agents.find((a) => a.toolId === data.selectedAgentId) ?? data.agents[0];

  const focusProvider = useMemo(() => {
    if (focusProviderId) {
      return data.providers.find((p) => p.id === focusProviderId) ?? null;
    }
    return data.providers[0] ?? null;
  }, [data.providers, focusProviderId]);

  if (["create", "edit", "agent-settings", "app-ai", "delete"].includes(data.overlay.type)) {
    return (
      <>
        <EditorPage data={data} detailStyle="split" />
        <DeleteConfirmModal data={data} />
      </>
    );
  }

  if (!agent) return null;

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden">
      <div data-tauri-drag-region className="h-4 shrink-0" aria-hidden />
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
        <header className="shrink-0 border-b border-border/50 px-5 py-4">
          <h1 className="text-xl font-bold tracking-tight">Providers · 绑定</h1>
          <p className="mt-0.5 text-xs text-muted-foreground">
            D3 · 双栏 — 左管 Provider 身份，右管当前 Agent 绑定与加法。
          </p>
        </header>

        {/* Compact cross-agent status — not a matrix */}
        <div className="flex shrink-0 gap-2 overflow-x-auto border-b border-border/40 px-5 py-2">
          {data.agents.map((a) => {
            const entry = activeEntry(data.toolActivations[a.toolId]);
            const provider = entry ? (data.providers.find((p) => p.id === entry.provider_id) ?? null) : null;
            const selected = a.toolId === agent.toolId;
            return (
              <button
                key={a.toolId}
                type="button"
                onClick={() => data.setSelectedAgentId(a.toolId)}
                className={cn(
                  "flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[11px]",
                  selected
                    ? "border-foreground/30 bg-foreground/5 font-medium"
                    : "border-border/50 text-muted-foreground",
                )}
              >
                <AgentToolIcon toolId={a.iconId} size="sm" />
                {a.displayName}
                <span className={cn("h-1.5 w-1.5 rounded-full", entry ? "bg-emerald-400" : "bg-zinc-500")} />
                {provider ? (
                  <span className="max-w-[72px] truncate font-mono text-[9px] opacity-70">{provider.name}</span>
                ) : null}
              </button>
            );
          })}
        </div>

        <div className="grid min-h-0 flex-1 grid-cols-1 lg:grid-cols-[minmax(0,1fr)_minmax(320px,380px)]">
          {/* Left rail — provider gallery */}
          <div className="ss-page-scroll border-r border-border/40 px-4 py-4">
            <h2 className="mb-3 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
              Provider 库（{data.providers.length}）
            </h2>
            <div className="space-y-2">
              {data.providers.map((provider) => {
                const focused = focusProvider?.id === provider.id;
                const boundHere = bindsProvider(data.toolActivations[agent.toolId], provider.id);
                const agentHits = data.agents.filter((a) => bindsProvider(data.toolActivations[a.toolId], provider.id));
                return (
                  <button
                    key={provider.id}
                    type="button"
                    onClick={() => setFocusProviderId(provider.id)}
                    className={cn(
                      "flex w-full items-start gap-3 rounded-xl border px-3 py-3 text-left transition-colors",
                      focused
                        ? "border-foreground/25 bg-card shadow-sm"
                        : "border-border/45 bg-card/30 hover:border-border/70",
                    )}
                  >
                    <ProviderBrandIcon providerName={provider.name} size="lg" />
                    <span className="min-w-0 flex-1">
                      <span className="flex items-center gap-2">
                        <span className="truncate text-sm font-medium">{provider.name}</span>
                        {boundHere ? (
                          <span className="rounded bg-emerald-500/15 px-1.5 py-0.5 text-[9px] text-emerald-400">
                            → {agent.displayName}
                          </span>
                        ) : null}
                      </span>
                      <span className="mt-0.5 block truncate font-mono text-[10px] text-muted-foreground">
                        {provider.default_model || "—"}
                      </span>
                      <span className="mt-1 flex flex-wrap gap-1">
                        {agentHits.map((a) => (
                          <span
                            key={a.toolId}
                            className="rounded bg-muted/60 px-1 py-0.5 text-[9px] text-muted-foreground"
                          >
                            {a.displayName}
                          </span>
                        ))}
                        {agentHits.length === 0 ? (
                          <span className="text-[9px] text-muted-foreground/60">未绑定任何 Agent</span>
                        ) : null}
                      </span>
                    </span>
                    <Button
                      size="sm"
                      variant="ghost"
                      className="shrink-0"
                      onClick={(e) => {
                        e.stopPropagation();
                        data.setOverlay({ type: "edit", providerId: provider.id });
                      }}
                    >
                      编辑
                    </Button>
                  </button>
                );
              })}
              {data.providers.length === 0 ? (
                <p className="rounded-xl border border-dashed border-border/60 px-4 py-8 text-center text-xs text-muted-foreground">
                  还没有 Provider。点右上角添加。
                </p>
              ) : null}
            </div>
          </div>

          {/* Right rail — binding inspector */}
          <div className="ss-page-scroll px-4 py-4">
            <BindingInspector
              data={data}
              agent={agent}
              focusProvider={focusProvider}
              maps={maps}
              setMap={(providerId, next) => setMaps((prev) => ({ ...prev, [providerId]: next }))}
            />
          </div>
        </div>

        <div className="shrink-0 border-t border-border/40 px-5 py-2">
          <StateDump
            state={{
              ...data.stateDump,
              ia: "D3-dual-rail",
              focusAgent: agent.toolId,
              focusProviderId: focusProvider?.id ?? null,
            }}
          />
        </div>
      </div>
    </div>
  );
}

function BindingInspector({
  data,
  agent,
  focusProvider,
  maps,
  setMap,
}: {
  data: PrototypeHubData;
  agent: AgentDescriptor;
  focusProvider: ProviderEntryFlat | null;
  maps: Record<string, ClaudeMapState>;
  setMap: (providerId: string, next: ClaudeMapState) => void;
}) {
  const entry = activeEntry(data.toolActivations[agent.toolId]);
  const boundProvider = entry ? (data.providers.find((p) => p.id === entry.provider_id) ?? null) : null;
  const compatible = focusProvider ? isCompatible(focusProvider, agent) : false;
  const models = focusProvider ? providerModels(focusProvider) : [];

  useEffect(() => {
    if (boundProvider && agent.toolId === "claude-code" && !maps[boundProvider.id]) {
      setMap(boundProvider.id, seedClaudeMap(boundProvider));
    }
  }, [boundProvider, agent.toolId, maps, setMap]);

  const map = boundProvider ? (maps[boundProvider.id] ?? emptyClaudeMap()) : emptyClaudeMap();
  const fill = mappingFillCount(map);

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2">
        <AgentToolIcon toolId={agent.iconId} size="md" />
        <div>
          <h2 className="text-sm font-semibold">{agent.displayName} · 绑定</h2>
          <p className="text-[10px] text-muted-foreground">{agent.configPathDisplay}</p>
        </div>
      </div>

      {boundProvider ? (
        <div className="rounded-xl border border-emerald-500/30 bg-emerald-500/[0.06] p-3 text-xs">
          <div className="flex items-center justify-between gap-2">
            <span className="flex items-center gap-1.5 font-medium text-emerald-300">
              <Cable className="h-3.5 w-3.5" />
              {boundProvider.name}
              {entry?.model ? <span className="font-mono text-[10px] opacity-70">· {entry.model}</span> : null}
            </span>
            <Button size="sm" variant="ghost" onClick={() => data.stub("deactivate", { toolId: agent.toolId })}>
              <Unplug className="h-3.5 w-3.5" />
            </Button>
          </div>
          {agent.toolId === "claude-code" ? (
            <p className="mt-1 text-[10px] text-emerald-200/70">
              映射 {fill.filled}/{fill.total} 角色已填
            </p>
          ) : null}
        </div>
      ) : (
        <p className="rounded-xl border border-dashed border-amber-500/30 bg-amber-500/[0.05] px-3 py-2 text-xs text-amber-200/90">
          未绑定。在左侧选中 Provider，再点下方「绑定到 {agent.displayName}」。
        </p>
      )}

      {focusProvider ? (
        <section className="rounded-xl border border-border/50 bg-card/40 p-3">
          <div className="flex items-center gap-2">
            <ProviderBrandIcon providerName={focusProvider.name} size="md" />
            <div className="min-w-0 flex-1">
              <p className="truncate text-xs font-medium">{focusProvider.name}</p>
              <p className="font-mono text-[10px] text-muted-foreground">选中中</p>
            </div>
          </div>
          {!compatible ? (
            <p className="mt-2 text-[11px] text-rose-300/90">
              缺少 {agent.requiredUrlField === "anthropic" ? "Anthropic" : "OpenAI"} 端点，无法绑定。
            </p>
          ) : (
            <div className="mt-3 flex items-center gap-2">
              {agent.kind === "multi" || agent.toolId !== "claude-code" ? (
                <select
                  className="h-8 min-w-0 flex-1 rounded-md border border-border/60 bg-background px-2 text-[11px]"
                  defaultValue={
                    entry?.provider_id === focusProvider.id ? entry.model : focusProvider.default_model || models[0]
                  }
                  id={`d3-model-${focusProvider.id}`}
                >
                  {models.map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </select>
              ) : (
                <span className="flex-1 text-[11px] text-muted-foreground">Claude：绑定后配置角色映射</span>
              )}
              <Button
                size="sm"
                disabled={!compatible}
                onClick={() => {
                  const select = document.getElementById(`d3-model-${focusProvider.id}`) as HTMLSelectElement | null;
                  data.stub("activate", {
                    toolId: agent.toolId,
                    providerId: focusProvider.id,
                    model: select?.value || focusProvider.default_model || models[0],
                  });
                }}
              >
                绑定
              </Button>
            </div>
          )}
        </section>
      ) : null}

      {agent.toolId === "claude-code" && boundProvider ? (
        <section className="rounded-xl border border-border/50 bg-card/40 p-3">
          <h3 className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
            加法 · Claude 映射
          </h3>
          <ClaudeMappingPanel
            chrome="page"
            provider={boundProvider}
            value={map}
            onChange={(next) => setMap(boundProvider.id, next)}
            onStub={data.stub}
          />
        </section>
      ) : null}
    </div>
  );
}
