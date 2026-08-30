import { Check, ChevronDown, Copy, Eraser, Route, Sparkles, X } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../../../../components/ui/button";
import { cn, copyToClipboard } from "../../../../../../lib/utils";
import type {
  OmpRoleTarget,
  OmpSettings,
  OmpThinkingLevel,
  ProviderEntryFlat,
  ToolBinding,
} from "../../../../../../types";
import type { Reasoning } from "../../../../../../types/generated/Reasoning";
import { useAgentDescriptor, useRoleDrops } from "../../../../api/agents";
import { useProvidersFlat } from "../../../../hooks/useProvidersFlat";
import {
  buildOmpLaunchCommand,
  isDefaultThinking,
  modelReasoning,
  OMP_DEFAULT_CYCLE_ORDER,
  OMP_PRIMARY_ROLES,
  OMP_ROLE_DEFS,
  OMP_ROLES_INHERITING_DEFAULT,
  OMP_SECONDARY_ROLES,
  OMP_TOOL_ID,
  previewRoleValue,
} from "../../../../lib/ompRoles";
import { getModelCatalogFromMeta } from "../../../../lib/providerPatch";
import { activeEntry, bindingRoles } from "../../../../lib/toolBinding";
import { OmpRoleRow } from "./OmpRoleRow";
import { providerModels } from "./RichMatrixShell";

/**
 * Providers a role may target: bound to OMP *and* carrying an OpenAI base URL.
 * Anything else is silently skipped by the writer, so it must not be offerable.
 */
export function ompAssignableProviders(
  binding: ToolBinding | null,
  providers: ProviderEntryFlat[],
): ProviderEntryFlat[] {
  const ids = new Set((binding?.entries ?? []).map((e) => e.provider_id));
  return providers.filter((p) => ids.has(p.id) && Boolean(p.base_url_openai));
}

/** Roles carrying a writable assignment (a role without a model is never written). */
export function ompAssignedCount(roles: Record<string, OmpRoleTarget>, ids: string[]): number {
  return ids.filter((id) => roles[id]?.provider_id && roles[id]?.model.trim()).length;
}

type OmpRolePanelProps = {
  binding: ToolBinding | null;
  providers: ProviderEntryFlat[];
  onClose?: () => void;
  /** "popover" = compact floating; "page" = full main-pane section. */
  chrome?: "popover" | "page";
};

/**
 * OMP `modelRoles` editor. Every change persists through
 * `update_tool_binding_settings` (optimistic + toast handled in the api layer),
 * so the query cache — not local state — is the value shown here.
 */
export function OmpRolePanel({ binding, providers, onClose, chrome = "popover" }: OmpRolePanelProps) {
  const { t } = useTranslation();
  const { updateToolBindingSettings } = useProvidersFlat();
  const [showSecondary, setShowSecondary] = useState(false);
  const [copied, setCopied] = useState(false);
  // The registry knows which roles fall back to which, and which the writer
  // just skipped. Both were previously invisible: the fallback table lived here
  // as a literal and the skips were reported nowhere at all.
  const descriptor = useAgentDescriptor(OMP_TOOL_ID);
  const drops = useRoleDrops(OMP_TOOL_ID);

  const roles = bindingRoles(binding);
  const assignable = useMemo(() => ompAssignableProviders(binding, providers), [binding, providers]);

  /**
   * What an empty row will fall back to. The backend registry answers for every
   * agent; the local list is only the value used before the query resolves, so
   * the row never briefly claims a role has no fallback when it does.
   */
  const inheritsOf = useCallback(
    (roleId: string): string | null => {
      const declared = descriptor?.roles.find((def) => def.agent_key === roleId || def.id === roleId);
      if (descriptor) return declared?.inherits ?? null;
      return OMP_ROLES_INHERITING_DEFAULT.includes(roleId) ? "default" : null;
    },
    [descriptor],
  );

  /** The reasoning capability of the model a role points at, when known. */
  const reasoningOf = useCallback(
    (target: OmpRoleTarget | undefined): Reasoning | null => {
      if (!target) return null;
      const provider = providers.find((p) => p.id === target.provider_id);
      return modelReasoning(getModelCatalogFromMeta(provider?.meta), target.model);
    },
    [providers],
  );

  const dropOf = useCallback((roleId: string) => drops.find((drop) => drop.role === roleId) ?? null, [drops]);
  const primaryCount = ompAssignedCount(
    roles,
    OMP_PRIMARY_ROLES.map((r) => r.id),
  );
  const totalCount = ompAssignedCount(
    roles,
    OMP_ROLE_DEFS.map((r) => r.id),
  );

  const persist = useCallback(
    (next: Record<string, OmpRoleTarget>) => {
      const settings: OmpSettings = { roles: next };
      void updateToolBindingSettings(OMP_TOOL_ID, settings).catch(() => {});
    },
    [updateToolBindingSettings],
  );

  const patchRole = (roleId: string, patch: Partial<OmpRoleTarget> | null) => {
    const next = { ...roles };
    if (patch === null) delete next[roleId];
    else next[roleId] = { ...(next[roleId] ?? { provider_id: "", model: "" }), ...patch };
    persist(next);
  };

  // Picking a provider also seeds a model, so one click yields a role the
  // backend can actually write.
  const assignProvider = (roleId: string, providerId: string) => {
    const provider = assignable.find((p) => p.id === providerId);
    const entryModel = binding?.entries.find((e) => e.provider_id === providerId)?.model ?? "";
    const seeded = roles[roleId]?.provider_id === providerId ? roles[roleId].model : "";
    const model =
      seeded || entryModel || provider?.default_model || (provider ? providerModels(provider)[0] : "") || "";
    patchRole(roleId, { provider_id: providerId, model });
  };

  const setThinking = (roleId: string, level: OmpThinkingLevel) => {
    patchRole(roleId, { thinking: isDefaultThinking(level) ? null : level });
  };

  /** Broadcast the active binding entry onto every unset primary role. */
  const fillPrimary = () => {
    const entry = activeEntry(binding);
    const provider = entry ? assignable.find((p) => p.id === entry.provider_id) : null;
    if (!entry || !provider) return;
    const model = entry.model || provider.default_model || providerModels(provider)[0] || "";
    if (!model) return;
    const next = { ...roles };
    for (const role of OMP_PRIMARY_ROLES) {
      if (next[role.id]?.model.trim()) continue;
      next[role.id] = { provider_id: provider.id, model };
    }
    persist(next);
  };

  const command = useMemo(() => {
    const models: Record<string, string | undefined> = {};
    for (const role of OMP_PRIMARY_ROLES) {
      const target = roles[role.id];
      models[role.id] = target
        ? (previewRoleValue(target.provider_id, target.model, target.thinking) ?? undefined)
        : undefined;
    }
    return buildOmpLaunchCommand(models);
  }, [roles]);

  const handleCopy = useCallback(async () => {
    if (await copyToClipboard(command)) {
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    }
  }, [command]);

  const canFill = Boolean(activeEntry(binding) && assignable.length > 0);

  return (
    <div
      className={cn(
        "bg-card/95 text-foreground backdrop-blur-xl",
        chrome === "popover" && "w-[min(660px,94vw)] rounded-xl border border-border/60 shadow-xl",
        chrome === "page" && "rounded-2xl border border-border/55",
      )}
    >
      <div className="flex items-start justify-between gap-3 border-b border-border/40 px-4 py-3">
        <div className="min-w-0">
          <h3 className="flex items-center gap-1.5 text-sm font-semibold">
            <Route className="h-3.5 w-3.5 text-primary" />
            {t("models.ompRoles.panel.title")}
          </h3>
          <p className="mt-0.5 text-[11px] leading-snug text-muted-foreground">{t("models.ompRoles.panel.subtitle")}</p>
          <p className="mt-1 font-mono text-[10px] text-muted-foreground">
            {t("models.ompRoles.panel.assignedCount", {
              primary: primaryCount,
              primaryTotal: OMP_PRIMARY_ROLES.length,
              total: totalCount,
              allTotal: OMP_ROLE_DEFS.length,
            })}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          <Button size="xs" variant="outline" disabled={!canFill} onClick={fillPrimary}>
            <Sparkles className="mr-1 h-3 w-3" />
            {t("models.ompRoles.panel.fillPrimary")}
          </Button>
          <Button size="xs" variant="ghost" disabled={totalCount === 0} onClick={() => persist({})}>
            <Eraser className="mr-1 h-3 w-3" />
            {t("models.ompRoles.panel.clearAll")}
          </Button>
          {onClose ? (
            <button
              type="button"
              onClick={onClose}
              className="rounded-lg p-1 text-muted-foreground hover:bg-muted/50 hover:text-foreground"
              aria-label={t("models.ompRoles.panel.close")}
            >
              <X className="h-4 w-4" />
            </button>
          ) : null}
        </div>
      </div>

      {assignable.length === 0 ? (
        <p className="px-4 py-6 text-center text-[11px] leading-5 text-muted-foreground">
          {t("models.ompRoles.panel.noAssignable")}
        </p>
      ) : (
        <div className="max-h-[54vh] space-y-1.5 overflow-y-auto px-4 py-3">
          {OMP_PRIMARY_ROLES.map((role) => (
            <OmpRoleRow
              key={role.id}
              role={role}
              target={roles[role.id]}
              providers={assignable}
              onAssignProvider={(providerId) => assignProvider(role.id, providerId)}
              onModelChange={(model) => patchRole(role.id, { model })}
              onThinkingChange={(level) => setThinking(role.id, level)}
              onClear={() => patchRole(role.id, null)}
              inheritsRole={inheritsOf(role.id)}
              reasoning={reasoningOf(roles[role.id])}
              drop={dropOf(role.id)}
            />
          ))}

          <button
            type="button"
            onClick={() => setShowSecondary((v) => !v)}
            className="flex w-full items-center gap-1.5 rounded-lg px-1 py-1.5 text-[11px] font-medium text-muted-foreground transition hover:text-foreground"
          >
            <ChevronDown className={cn("h-3.5 w-3.5 transition-transform", showSecondary && "rotate-180")} />
            {t("models.ompRoles.panel.moreRoles", { n: OMP_SECONDARY_ROLES.length })}
          </button>

          {showSecondary
            ? OMP_SECONDARY_ROLES.map((role) => (
                <OmpRoleRow
                  key={role.id}
                  role={role}
                  target={roles[role.id]}
                  providers={assignable}
                  onAssignProvider={(providerId) => assignProvider(role.id, providerId)}
                  onModelChange={(model) => patchRole(role.id, { model })}
                  onThinkingChange={(level) => setThinking(role.id, level)}
                  onClear={() => patchRole(role.id, null)}
                  inheritsRole={inheritsOf(role.id)}
                  reasoning={reasoningOf(roles[role.id])}
                  drop={dropOf(role.id)}
                />
              ))
            : null}

          <p className="pt-1 text-[10px] leading-4 text-muted-foreground/85">
            {t("models.ompRoles.panel.cycleHint", { order: OMP_DEFAULT_CYCLE_ORDER.join(" · ") })}
          </p>
        </div>
      )}

      <div className="space-y-1.5 border-t border-border/40 px-4 py-2.5">
        <div className="flex items-center justify-between gap-2">
          <span className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            {t("models.ompRoles.panel.launchTitle")}
          </span>
          <Button size="xs" variant="outline" onClick={() => void handleCopy()}>
            {copied ? <Check className="mr-1 h-3 w-3" /> : <Copy className="mr-1 h-3 w-3" />}
            {copied ? t("models.ompRoles.panel.copied") : t("models.ompRoles.panel.copyCommand")}
          </Button>
        </div>
        <pre className="overflow-x-auto rounded-[9px] border border-border/45 bg-muted/35 px-2.5 py-2 font-mono text-[10px] leading-4 text-muted-foreground">
          {command}
        </pre>
        <p className="text-[10px] text-muted-foreground/80">{t("models.ompRoles.panel.diskHint")}</p>
      </div>
    </div>
  );
}
