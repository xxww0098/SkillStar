import { Cable, Unplug } from "lucide-react";
import { useEffect, useState } from "react";
import { ProviderBrandIcon } from "../../../../../../components/shared/ProviderBrandIcon";
import { Button } from "../../../../../../components/ui/button";
import { cn } from "../../../../../../lib/utils";
import { AgentToolIcon } from "../../../shared/AgentToolIcon";
import type { AgentDescriptor } from "../../../../lib/agentRegistry";
import { activeEntry } from "../../../../lib/toolBinding";
import { EditorPage } from "../EditorPage";
import { DeleteConfirmModal } from "../PrototypeOverlays";
import { StateDump } from "../StateDump";
import type { PrototypeHubData } from "../types";
import {
  ClaudeMappingPanel,
  emptyClaudeMap,
  seedClaudeMap,
  type ClaudeMapState,
} from "../matrix/rich/ClaudeMappingPanel";
import { isCompatible, providerModels } from "../matrix/rich/RichMatrixShell";

/**
 * D2 — App-vertical (cc-switch style)
 * Agent strip is the primary nav. Content = that agent's provider list +
 * bind controls + agent-specific additive slot (Claude mapping inline).
 * No cross-agent matrix.
 */
export function VariantD2({ data }: { data: PrototypeHubData }) {
  const [maps, setMaps] = useState<Record<string, ClaudeMapState>>({});
  const agent = data.agents.find((a) => a.toolId === data.selectedAgentId) ?? data.agents[0];

  if (["create", "edit", "agent-settings", "app-ai", "delete"].includes(data.overlay.type)) {
    return (
      <>
        <EditorPage data={data} detailStyle="sections" />
        <DeleteConfirmModal data={data} />
      </>
    );
  }

  if (!agent) return null;

  return (
    <>
      <div className="flex h-full min-h-0 flex-col overflow-hidden">
        <div data-tauri-drag-region className="h-4 shrink-0" aria-hidden />
        <main className="ss-page-scroll">
          <div className="mx-auto w-full max-w-3xl space-y-5 px-5 py-6">
            <header>
              <h1 className="text-2xl font-bold tracking-tight">Agent 工作台</h1>
              <p className="mt-1 text-sm text-muted-foreground">
                D2 · App 竖切 — 先选 Agent，再管它的 Provider 与专属加法（对齐 cc-switch）。
              </p>
            </header>

            {/* Primary: Agent switcher */}
            <div className="flex gap-1.5 rounded-2xl border border-border/55 bg-muted/25 p-1.5">
              {data.agents.map((a) => {
                const entry = activeEntry(data.toolActivations[a.toolId]);
                const active = a.toolId === agent.toolId;
                return (
                  <button
                    key={a.toolId}
                    type="button"
                    onClick={() => data.setSelectedAgentId(a.toolId)}
                    className={cn(
                      "flex flex-1 items-center justify-center gap-2 rounded-xl px-3 py-2.5 text-xs font-medium transition-colors",
                      active
                        ? "bg-background text-foreground shadow-sm"
                        : "text-muted-foreground hover:text-foreground",
                    )}
                  >
                    <AgentToolIcon toolId={a.iconId} size="sm" />
                    <span className="hidden sm:inline">{a.displayName}</span>
                    <span
                      className={cn(
                        "h-1.5 w-1.5 rounded-full",
                        entry ? "bg-emerald-400" : "bg-zinc-500",
                      )}
                    />
                  </button>
                );
              })}
            </div>

            <AgentWorkbench
              data={data}
              agent={agent}
              maps={maps}
              setMap={(providerId, next) => setMaps((prev) => ({ ...prev, [providerId]: next }))}
            />

            <StateDump state={{ ...data.stateDump, ia: "D2-app-vertical", focusAgent: agent.toolId }} />
          </div>
        </main>
      </div>
    </>
  );
}

function AgentWorkbench({
  data,
  agent,
  maps,
  setMap,
}: {
  data: PrototypeHubData;
  agent: AgentDescriptor;
  maps: Record<string, ClaudeMapState>;
  setMap: (providerId: string, next: ClaudeMapState) => void;
}) {
  const binding = data.toolActivations[agent.toolId];
  const entry = activeEntry(binding);
  const activeProvider = entry ? (data.providers.find((p) => p.id === entry.provider_id) ?? null) : null;
  const compatible = data.providers.filter((p) => isCompatible(p, agent));

  useEffect(() => {
    if (activeProvider && agent.toolId === "claude-code" && !maps[activeProvider.id]) {
      setMap(activeProvider.id, seedClaudeMap(activeProvider));
    }
  }, [activeProvider, agent.toolId, maps, setMap]);

  return (
    <div className="space-y-4">
      <section className="rounded-2xl border border-border/55 bg-card/50 p-4">
        <div className="flex items-start gap-3">
          <AgentToolIcon toolId={agent.iconId} size="md" />
          <div className="min-w-0 flex-1">
            <h2 className="text-base font-semibold">{agent.displayName}</h2>
            <p className="text-[11px] text-muted-foreground">{agent.configPathDisplay}</p>
            <p className="mt-1 text-xs text-muted-foreground">
              {agent.kind === "multi" ? "可并存多个 Provider，指针选中激活项。" : "全局单一绑定；切换即覆盖。"}
              {agent.toolId === "claude-code" ? " · 专属加法：角色模型映射。" : null}
            </p>
          </div>
          {activeProvider ? (
            <Button
              size="sm"
              variant="ghost"
              className="text-muted-foreground"
              onClick={() => data.stub("deactivate", { toolId: agent.toolId })}
            >
              <Unplug className="mr-1 h-3.5 w-3.5" />
              停用
            </Button>
          ) : null}
        </div>

        {activeProvider ? (
          <div className="mt-4 rounded-xl border border-emerald-500/25 bg-emerald-500/[0.06] px-3 py-2.5 text-xs">
            <div className="flex items-center gap-2 font-medium text-emerald-300">
              <Cable className="h-3.5 w-3.5" />
              已绑定 · {activeProvider.name}
              {entry?.model ? <span className="font-mono text-[10px] text-emerald-200/70">· {entry.model}</span> : null}
            </div>
          </div>
        ) : (
          <p className="mt-4 text-xs text-amber-300/90">尚未绑定 Provider — 从下方列表启用一条。</p>
        )}
      </section>

      <section className="space-y-2">
        <h3 className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
          可用 Provider（{compatible.length}）
        </h3>
        {compatible.length === 0 ? (
          <p className="rounded-xl border border-dashed border-border/60 px-4 py-6 text-center text-xs text-muted-foreground">
            没有兼容端点。先添加带 {agent.requiredUrlField === "anthropic" ? "Anthropic" : "OpenAI"} URL 的
            Provider。
          </p>
        ) : (
          compatible.map((provider) => {
            const bound = entry?.provider_id === provider.id;
            const models = providerModels(provider);
            return (
              <article
                key={provider.id}
                className={cn(
                  "rounded-xl border px-3 py-3 transition-colors",
                  bound ? "border-emerald-500/40 bg-emerald-500/[0.04]" : "border-border/50 bg-card/40",
                )}
              >
                <div className="flex items-center gap-3">
                  <ProviderBrandIcon name={provider.name} size={28} />
                  <div className="min-w-0 flex-1">
                    <button
                      type="button"
                      className="truncate text-sm font-medium hover:underline"
                      onClick={() => data.setOverlay({ type: "edit", providerId: provider.id })}
                    >
                      {provider.name}
                    </button>
                    <p className="truncate font-mono text-[10px] text-muted-foreground">
                      {provider.default_model || models[0] || "—"}
                    </p>
                  </div>
                  {agent.kind === "multi" && !bound ? (
                    <select
                      className="h-8 max-w-[140px] rounded-md border border-border/60 bg-background px-2 text-[11px]"
                      defaultValue={provider.default_model || models[0] || ""}
                      onChange={(e) =>
                        data.stub("activate", {
                          toolId: agent.toolId,
                          providerId: provider.id,
                          model: e.target.value,
                        })
                      }
                    >
                      {models.map((m) => (
                        <option key={m} value={m}>
                          {m}
                        </option>
                      ))}
                    </select>
                  ) : null}
                  <Button
                    size="sm"
                    variant={bound ? "outline" : "default"}
                    onClick={() =>
                      bound
                        ? data.stub("deactivate", { toolId: agent.toolId })
                        : data.stub("activate", {
                            toolId: agent.toolId,
                            providerId: provider.id,
                            model: provider.default_model || models[0],
                          })
                    }
                  >
                    {bound ? "已启用" : "启用"}
                  </Button>
                </div>
              </article>
            );
          })
        )}
      </section>

      {/* Agent-specific additive slot — only when bound + Claude */}
      {agent.toolId === "claude-code" && activeProvider ? (
        <section className="rounded-2xl border border-border/55 bg-card/50 p-4">
          <h3 className="mb-3 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
            专属加法 · 模型映射
          </h3>
          <ClaudeMappingPanel
            chrome="page"
            provider={activeProvider}
            value={maps[activeProvider.id] ?? emptyClaudeMap()}
            onChange={(next) => setMap(activeProvider.id, next)}
            onUnbind={() => data.stub("deactivate", { toolId: "claude-code" })}
            onStub={data.stub}
          />
        </section>
      ) : null}

      {agent.toolId !== "claude-code" ? (
        <p className="text-[11px] text-muted-foreground">
          此 Agent 暂无额外表单槽；加法只在有专属磁盘语义时出现（如 Claude 映射）。
        </p>
      ) : null}
    </div>
  );
}
