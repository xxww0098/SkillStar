import { Layers3, Route, Unplug } from "lucide-react";
import { Popover } from "radix-ui";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../../../../components/ui/button";
import { cn } from "../../../../../../lib/utils";
import type { ProviderEntryFlat } from "../../../../../../types";
import type { AgentDescriptor } from "../../../../lib/agentRegistry";
import { useAgentDescriptor } from "../../../../api/agents";
import { OMP_PRIMARY_ROLES, OMP_TOOL_ID } from "../../../../lib/ompRoles";
import { activeEntry, bindingRoles, bindsProvider } from "../../../../lib/toolBinding";
import { EditorPage } from "../EditorPage";
import type { ModelsHubData } from "../types";
import { ClaudeMappingPanel, claudeFillCount } from "./ClaudeMappingPanel";
import { ompAssignedCount, OmpRolePanel } from "./OmpRolePanel";
import { isCompatible, providerModels, RichMatrixShell } from "./RichMatrixShell";

/**
 * B2b — Inline select (+ Claude Code mapping)
 * Other agents: inline model <select>.
 * Claude Code: role→display/request/1M mapping (product screenshot), not a single model.
 */
function isClaudeColumn(toolId: string): boolean {
  return toolId === "claude-code" || toolId === "claude-desktop";
}

/** Where a Claude binding lands on disk — differs for CLI vs Desktop. */
function claudeDiskHintKey(toolId: string): string {
  return toolId === "claude-desktop" ? "models.matrix.claudeDesktopDiskHint" : "models.matrix.claudeCliDiskHint";
}

export function VariantB2b({ data }: { data: ModelsHubData }) {
  const { t } = useTranslation();

  // Full-page Claude mapping from agent strip / deep settings.
  if (data.overlay.type === "agent-settings" && isClaudeColumn(data.overlay.toolId)) {
    return <ClaudeMappingPage data={data} toolId={data.overlay.toolId} />;
  }

  // Create / App AI stay main-pane pages. Edit/delete are owned by ModelsHub
  // (ProviderEditorDrawer + DeleteProviderDialog) over the matrix.
  if (data.overlay.type === "create" || data.overlay.type === "app-ai" || data.overlay.type === "agent-settings") {
    return <EditorPage data={data} detailStyle="tabs" />;
  }

  return (
    <RichMatrixShell
      data={data}
      subtitle={t("models.matrix.b2bSubtitle")}
      editorStyle="tabs"
      providerCol="compact"
      columnHint={(column) =>
        isClaudeColumn(column.bindToolId) ? "mapping" : column.bindToolId === OMP_TOOL_ID ? "roles" : null
      }
      legend={null}
      renderCell={({ provider, column, agent }) =>
        isClaudeColumn(column.bindToolId) ? (
          <ClaudeCodeCell data={data} provider={provider} toolId={column.bindToolId} />
        ) : column.bindToolId === OMP_TOOL_ID ? (
          <OmpRoleCell data={data} provider={provider} agent={agent} />
        ) : (
          <InlineSelectCell data={data} provider={provider} agent={agent} />
        )
      }
    />
  );
}

function ClaudeMappingPage({ data, toolId }: { data: ModelsHubData; toolId: string }) {
  const { t } = useTranslation();
  const binding = data.toolActivations[toolId] ?? null;
  const entry = activeEntry(binding);
  const provider = entry ? (data.providers.find((p) => p.id === entry.provider_id) ?? null) : null;
  const title = toolId === "claude-desktop" ? "Claude Desktop" : "Claude CLI";

  if (!provider) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-8 text-sm text-muted-foreground">
        <p>{t("models.matrix.notBoundYet", { name: title })}</p>
        <Button size="sm" onClick={data.closeOverlay}>
          {t("models.matrix.backToMatrix")}
        </Button>
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden">
      <div data-tauri-drag-region className="h-4 shrink-0" aria-hidden />
      <header className="flex shrink-0 items-center gap-3 border-b border-border/50 px-5 py-3">
        <Button size="sm" variant="ghost" onClick={data.closeOverlay}>
          ← {t("models.common.back")}
        </Button>
        <div className="min-w-0 flex-1">
          <h1 className="text-base font-semibold">
            {title} · {t("models.claudeMapping.title")}
          </h1>
          <p className="text-[11px] text-muted-foreground">{provider.name}</p>
        </div>
      </header>
      <div className="ss-page-scroll">
        <div className="mx-auto max-w-3xl px-5 py-5">
          <ClaudeMappingPanel
            chrome="page"
            provider={provider}
            toolId={toolId}
            binding={binding}
            onUnbind={() => {
              void data.deactivateTool(toolId);
              data.closeOverlay();
            }}
            diskHint={t(claudeDiskHintKey(toolId))}
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
}: {
  data: ModelsHubData;
  provider: ProviderEntryFlat;
  toolId: string;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const binding = data.toolActivations[toolId] ?? null;
  const entry = activeEntry(binding);
  const isActive = entry?.provider_id === provider.id;
  const descriptor = useAgentDescriptor(toolId);
  const roles = bindingRoles(binding);
  const defs = useMemo(() => descriptor?.roles ?? [], [descriptor]);
  const { filled, total } = claudeFillCount(roles, defs);
  const unmappedLabel = t("models.matrix.unmapped");
  const primary = roles.sonnet?.model.trim() || entry?.model || provider.default_model || unmappedLabel;

  const compatible = Boolean(provider.base_url_anthropic);
  if (!compatible) {
    return (
      <div className="mx-auto flex h-14 w-full max-w-[168px] items-center justify-center text-[10px] text-muted-foreground/30">
        {t("models.matrix.needsAnthropicUrl")}
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
            .then(() => setOpen(true))
            .catch(() => {});
        }}
        className="mx-auto flex h-14 w-full max-w-[168px] flex-col items-center justify-center gap-0.5 rounded-xl border border-dashed border-border/55 text-[11px] text-muted-foreground hover:border-primary/40 hover:bg-primary/[0.04] hover:text-foreground"
      >
        <Layers3 className="h-3.5 w-3.5" />
        {t("models.matrix.bindAndMap")}
      </button>
    );
  }

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
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
            {t("models.matrix.mappedCount", { filled, total })}
          </span>
          <span className="w-full truncate font-mono text-[10px] text-foreground">Sonnet · {primary}</span>
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content align="center" sideOffset={8} className="z-[120] p-0">
          <ClaudeMappingPanel
            chrome="popover"
            provider={provider}
            toolId={toolId}
            binding={binding}
            onClose={() => setOpen(false)}
            onUnbind={() => {
              void data.deactivateTool(toolId);
              setOpen(false);
            }}
            diskHint={t(claudeDiskHintKey(toolId))}
          />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

/**
 * OMP cell — the binding is only half the story, so the cell opens the role
 * panel instead of a single model <select>. Unbound behaviour stays the plain
 * "Bind" button the other multi-provider columns use.
 */
function OmpRoleCell({
  data,
  provider,
  agent,
}: {
  data: ModelsHubData;
  provider: ProviderEntryFlat;
  agent: AgentDescriptor;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const binding = data.toolActivations[agent.toolId] ?? null;
  const bound = bindsProvider(binding, provider.id);
  const entryModel = binding?.entries.find((e) => e.provider_id === provider.id)?.model ?? "";
  const primaryIds = useMemo(() => OMP_PRIMARY_ROLES.map((r) => r.id), []);
  const assigned = ompAssignedCount(bindingRoles(binding), primaryIds);

  if (!isCompatible(provider, agent)) {
    return (
      <div className="mx-auto flex h-14 w-full max-w-[160px] items-center justify-center text-[10px] text-muted-foreground/30">
        n/a
      </div>
    );
  }

  if (!bound) {
    return (
      <button
        type="button"
        onClick={() =>
          void data.activateTool(provider.id, agent.toolId, provider.default_model || undefined).catch(() => {})
        }
        className="mx-auto flex h-14 w-full max-w-[160px] items-center justify-center rounded-xl border border-dashed border-border/55 text-[11px] text-muted-foreground hover:border-primary/40 hover:bg-primary/[0.04]"
      >
        {t("models.common.bind")}
      </button>
    );
  }

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <div className="mx-auto flex h-14 w-full max-w-[160px] items-center gap-1 rounded-xl border border-emerald-500/35 bg-emerald-500/[0.08] px-1.5">
        <Popover.Trigger asChild>
          <button
            type="button"
            className="flex min-w-0 flex-1 flex-col items-start justify-center gap-0.5 rounded-lg px-1 py-1 text-left hover:bg-emerald-500/10"
          >
            <span className="flex w-full items-center gap-1 text-[10px] font-semibold text-emerald-400">
              <Route className="h-3 w-3 shrink-0" />
              {t("models.ompRoles.panel.cellRoles", { assigned, total: primaryIds.length })}
            </span>
            <span className="w-full truncate font-mono text-[10px] text-foreground">{entryModel || "—"}</span>
          </button>
        </Popover.Trigger>
        <Button
          size="icon-xs"
          variant="ghost"
          title={t("models.ompRoles.panel.unbindProvider")}
          className="shrink-0 text-muted-foreground hover:text-destructive"
          // Per-provider unbind, not deactivateTool: role routing is the whole
          // point of binding several providers to OMP, so pulling one must not
          // take the others down with it. The backend prunes any role that
          // targeted this provider.
          onClick={() => void data.removeBindingEntry(agent.toolId, provider.id).catch(() => {})}
        >
          <Unplug className="h-3 w-3" />
        </Button>
      </div>
      <Popover.Portal>
        <Popover.Content align="center" sideOffset={8} className="z-[120] p-0">
          <OmpRolePanel binding={binding} providers={data.providers} onClose={() => setOpen(false)} />
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
  data: ModelsHubData;
  provider: ProviderEntryFlat;
  agent: AgentDescriptor;
}) {
  const { t } = useTranslation();
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
        {t("models.common.bind")}
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
        title={t("models.common.unbind")}
        className="shrink-0 text-muted-foreground hover:text-destructive"
        onClick={() => void data.deactivateTool(agent.toolId).catch(() => {})}
      >
        <Unplug className="h-3 w-3" />
      </Button>
    </div>
  );
}
