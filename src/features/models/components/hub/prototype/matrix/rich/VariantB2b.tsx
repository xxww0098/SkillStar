import { Layers3, Unplug } from "lucide-react";
import { Popover } from "radix-ui";
import { useEffect, useMemo, useState } from "react";
import { Button } from "../../../../../../../components/ui/button";
import { cn } from "../../../../../../../lib/utils";
import type { ProviderEntryFlat } from "../../../../../../../types";
import type { AgentDescriptor } from "../../../../../lib/agentRegistry";
import { activeEntry } from "../../../../../lib/toolBinding";
import { EditorPage } from "../../EditorPage";
import type { PrototypeHubData } from "../../types";
import {
  ClaudeMappingPanel,
  emptyClaudeMap,
  mappingFillCount,
  seedClaudeMap,
  type ClaudeMapState,
} from "./ClaudeMappingPanel";
import { isCompatible, providerModels, RichMatrixShell } from "./RichMatrixShell";

/**
 * B2b — Inline select (+ Claude Code mapping)
 * Other agents: inline model <select>.
 * Claude Code: role→display/request/1M mapping (product screenshot), not a single model.
 */
function isClaudeColumn(toolId: string): boolean {
  return toolId === "claude-code" || toolId === "claude-desktop";
}

function mapKey(toolId: string, providerId: string): string {
  return `${toolId}::${providerId}`;
}

export function VariantB2b({ data }: { data: PrototypeHubData }) {
  // Mapping is per Claude surface × provider — CLI and Desktop never share rows.
  const [maps, setMaps] = useState<Record<string, ClaudeMapState>>({});

  const getMap = (toolId: string, provider: ProviderEntryFlat) =>
    maps[mapKey(toolId, provider.id)] ?? emptyClaudeMap();
  const setMap = (toolId: string, providerId: string, next: ClaudeMapState) => {
    setMaps((prev) => ({ ...prev, [mapKey(toolId, providerId)]: next }));
  };

  // Full-page Claude mapping from agent strip / deep settings.
  if (data.overlay.type === "agent-settings" && isClaudeColumn(data.overlay.toolId)) {
    const toolId = data.overlay.toolId;
    return (
      <ClaudeMappingPage
        data={data}
        toolId={toolId}
        maps={maps}
        setMap={(providerId, next) => setMap(toolId, providerId, next)}
      />
    );
  }

  // Create / App AI stay main-pane pages. Edit/delete are owned by ModelsHub
  // (ProviderEditorDrawer + DeleteProviderDialog) over the matrix.
  if (data.overlay.type === "create" || data.overlay.type === "app-ai" || data.overlay.type === "agent-settings") {
    return <EditorPage data={data} detailStyle="tabs" />;
  }

  return (
    <RichMatrixShell
      data={data}
      subtitle="Provider × Agent — CLI / Desktop 绑定独立；表头切换官方账号；行绑定第三方 Provider。"
      editorStyle="tabs"
      providerCol="compact"
      columnHint={(column) => (isClaudeColumn(column.bindToolId) ? "mapping" : null)}
      legend={null}
      renderCell={({ provider, column, agent }) =>
        isClaudeColumn(column.bindToolId) ? (
          <ClaudeCodeCell
            data={data}
            provider={provider}
            toolId={column.bindToolId}
            map={getMap(column.bindToolId, provider)}
            onMapChange={(next) => setMap(column.bindToolId, provider.id, next)}
          />
        ) : (
          <InlineSelectCell data={data} provider={provider} agent={agent} />
        )
      }
    />
  );
}

function ClaudeMappingPage({
  data,
  toolId,
  maps,
  setMap,
}: {
  data: PrototypeHubData;
  toolId: string;
  maps: Record<string, ClaudeMapState>;
  setMap: (providerId: string, next: ClaudeMapState) => void;
}) {
  const entry = activeEntry(data.toolActivations[toolId]);
  const provider = entry ? (data.providers.find((p) => p.id === entry.provider_id) ?? null) : null;
  const title = toolId === "claude-desktop" ? "Claude Desktop" : "Claude CLI";
  const key = provider ? mapKey(toolId, provider.id) : "";

  useEffect(() => {
    if (provider && !maps[key]) {
      setMap(provider.id, seedClaudeMap(provider));
    }
  }, [provider, maps, key, setMap]);

  if (!provider) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-8 text-sm text-muted-foreground">
        <p>{title} 尚未绑定 Provider。</p>
        <Button size="sm" onClick={data.closeOverlay}>
          返回矩阵
        </Button>
      </div>
    );
  }

  const value = maps[key] ?? seedClaudeMap(provider);

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden">
      <div data-tauri-drag-region className="h-4 shrink-0" aria-hidden />
      <header className="flex shrink-0 items-center gap-3 border-b border-border/50 px-5 py-3">
        <Button size="sm" variant="ghost" onClick={data.closeOverlay}>
          ← Back
        </Button>
        <div className="min-w-0 flex-1">
          <h1 className="text-base font-semibold">{title} · 模型映射</h1>
          <p className="text-[11px] text-muted-foreground">{provider.name}</p>
        </div>
      </header>
      <div className="ss-page-scroll">
        <div className="mx-auto max-w-3xl px-5 py-5">
          <ClaudeMappingPanel
            chrome="page"
            provider={provider}
            value={value}
            onChange={(next) => setMap(provider.id, next)}
            onUnbind={() => {
              void data.deactivateTool(toolId);
              data.closeOverlay();
            }}
            onStub={data.stub}
            diskHint={
              toolId === "claude-desktop"
                ? "绑定写入 ~/.claude-desktop/skillstar-binding.json（原生 Desktop 配置待接入）"
                : "写入 ~/.claude/settings.json（原型 stub）"
            }
          />
        </div>
      </div>
    </div>
  );
}

function ClaudeCodeCell({
  data,
  provider,
  toolId,
  map,
  onMapChange,
}: {
  data: PrototypeHubData;
  provider: ProviderEntryFlat;
  toolId: string;
  map: ClaudeMapState;
  onMapChange: (next: ClaudeMapState) => void;
}) {
  const [open, setOpen] = useState(false);
  const entry = activeEntry(data.toolActivations[toolId]);
  const isActive = entry?.provider_id === provider.id;
  const { filled, total } = mappingFillCount(map);
  const primary = useMemo(() => {
    const sonnet = map.sonnet.requestModel.trim();
    return sonnet || provider.default_model || "未映射";
  }, [map, provider.default_model]);

  const compatible = Boolean(provider.base_url_anthropic);
  if (!compatible) {
    return (
      <div className="mx-auto flex h-14 w-full max-w-[168px] items-center justify-center text-[10px] text-muted-foreground/30">
        需 Anthropic URL
      </div>
    );
  }

  if (!isActive) {
    return (
      <button
        type="button"
        onClick={() => {
          void data
            .activateTool(provider.id, toolId, provider.default_model || undefined)
            .then(() => {
              onMapChange(seedClaudeMap(provider));
              setOpen(true);
            })
            .catch(() => {});
        }}
        className="mx-auto flex h-14 w-full max-w-[168px] flex-col items-center justify-center gap-0.5 rounded-xl border border-dashed border-border/55 text-[11px] text-muted-foreground hover:border-primary/40 hover:bg-primary/[0.04] hover:text-foreground"
      >
        <Layers3 className="h-3.5 w-3.5" />
        绑定并映射
      </button>
    );
  }

  const panelValue = filled === 0 ? seedClaudeMap(provider) : map;

  return (
    <Popover.Root
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        if (next && filled === 0) onMapChange(seedClaudeMap(provider));
      }}
    >
      <Popover.Trigger asChild>
        <button
          type="button"
          className={cn(
            "mx-auto flex h-14 w-full max-w-[168px] flex-col items-start justify-center gap-0.5 rounded-xl border px-2.5 text-left",
            "border-emerald-500/35 bg-emerald-500/[0.08] hover:bg-emerald-500/[0.12]",
          )}
        >
          <span className="flex w-full items-center gap-1 text-[10px] font-semibold text-emerald-400">
            <Layers3 className="h-3 w-3 shrink-0" />
            映射 {filled}/{total}
          </span>
          <span className="w-full truncate font-mono text-[10px] text-foreground">Sonnet · {primary}</span>
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content align="center" sideOffset={8} className="z-[120] p-0">
          <ClaudeMappingPanel
            chrome="popover"
            provider={provider}
            value={panelValue}
            onChange={onMapChange}
            onClose={() => setOpen(false)}
            onUnbind={() => {
              void data.deactivateTool(toolId);
              setOpen(false);
            }}
            onStub={data.stub}
            diskHint={
              toolId === "claude-desktop"
                ? "绑定写入 ~/.claude-desktop/skillstar-binding.json（原生 Desktop 配置待接入）"
                : "写入 ~/.claude/settings.json（原型 stub）"
            }
          />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

function InlineSelectCell({
  data,
  provider,
  agent,
}: {
  data: PrototypeHubData;
  provider: ProviderEntryFlat;
  agent: AgentDescriptor;
}) {
  const entry = activeEntry(data.toolActivations[agent.toolId]);
  const isActive = entry?.provider_id === provider.id;
  const models = providerModels(provider);
  const current = entry?.model || provider.default_model || "";

  if (!isCompatible(provider, agent)) {
    return (
      <div className="mx-auto flex h-14 w-full max-w-[160px] items-center justify-center text-[10px] text-muted-foreground/30">
        n/a
      </div>
    );
  }

  if (!isActive) {
    return (
      <button
        type="button"
        onClick={() =>
          void data.activateTool(provider.id, agent.toolId, provider.default_model || undefined).catch(() => {})
        }
        className="mx-auto flex h-14 w-full max-w-[160px] items-center justify-center rounded-xl border border-dashed border-border/55 text-[11px] text-muted-foreground hover:border-primary/40 hover:bg-primary/[0.04]"
      >
        Bind
      </button>
    );
  }

  return (
    <div className="mx-auto flex h-14 w-full max-w-[160px] items-center gap-1 rounded-xl border border-emerald-500/35 bg-emerald-500/[0.08] px-1.5">
      <select
        className="h-9 min-w-0 flex-1 cursor-pointer rounded-lg border-0 bg-transparent px-1.5 font-mono text-[10px] text-foreground outline-none focus:ring-1 focus:ring-primary/40"
        value={current}
        onChange={(e) => void data.activateTool(provider.id, agent.toolId, e.target.value).catch(() => {})}
        aria-label={`${agent.displayName} model`}
      >
        {models.map((m) => (
          <option key={m} value={m}>
            {m}
          </option>
        ))}
        {!models.includes(current) && current ? <option value={current}>{current}</option> : null}
      </select>
      <Button
        size="icon-xs"
        variant="ghost"
        title="Unbind"
        className="shrink-0 text-muted-foreground hover:text-destructive"
        onClick={() => void data.deactivateTool(agent.toolId).catch(() => {})}
      >
        <Unplug className="h-3 w-3" />
      </Button>
    </div>
  );
}
