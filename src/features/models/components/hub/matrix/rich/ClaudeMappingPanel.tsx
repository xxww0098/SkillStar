import { AlertTriangle, Download, Loader2, Sparkles, Unplug, X } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "../../../../../../components/ui/button";
import { cn } from "../../../../../../lib/utils";
import type { DroppedRole, ProviderEntryFlat, RoleTarget, ToolBinding } from "../../../../../../types";
import type { RoleDefDto } from "../../../../../../types/generated/RoleDefDto";
import { useAgentDescriptor, useRoleDrops } from "../../../../api/agents";
import { useModelFetch } from "../../../../api/modelCatalog";
import { useProvidersFlat } from "../../../../hooks/useProvidersFlat";
import { MODEL_CATALOG_META_KEY, buildModelCatalog } from "../../../../lib/providerPatch";
import { bindingRoles } from "../../../../lib/toolBinding";
import { providerModels } from "./RichMatrixShell";

/**
 * Claude's roles, while the registry query is in flight.
 *
 * The real list comes from the backend: which roles exist and which env key
 * each writes are facts the writer owns, and a second copy here is how the two
 * drift. `fable` used to be in this list and is not in the backend's, because
 * Claude Code has no `ANTHROPIC_DEFAULT_FABLE_MODEL` — a row that could never
 * be written.
 */
const CLAUDE_FALLBACK_ROLES: RoleDefDto[] = [
  { id: "default", agent_key: "ANTHROPIC_MODEL", primary: true, inherits: null, requires: "any" },
  { id: "fast", agent_key: "ANTHROPIC_DEFAULT_HAIKU_MODEL", primary: true, inherits: null, requires: "any" },
  { id: "sonnet", agent_key: "ANTHROPIC_DEFAULT_SONNET_MODEL", primary: false, inherits: null, requires: "any" },
  { id: "opus", agent_key: "ANTHROPIC_DEFAULT_OPUS_MODEL", primary: false, inherits: null, requires: "any" },
  {
    id: "subagent",
    agent_key: "CLAUDE_CODE_SUBAGENT_MODEL",
    primary: false,
    inherits: "default",
    requires: "any",
  },
];

/** Human label for a role id. Claude's tier names are their own labels. */
function roleLabel(id: string): string {
  switch (id) {
    case "default":
      return "Default";
    case "fast":
      return "Haiku";
    case "sonnet":
      return "Sonnet";
    case "opus":
      return "Opus";
    case "subagent":
      return "Subagent";
    default:
      return id;
  }
}

/** Roles carrying a writable assignment. */
export function claudeFillCount(
  roles: Record<string, RoleTarget>,
  defs: RoleDefDto[],
): { filled: number; total: number } {
  return {
    filled: defs.filter((def) => roles[def.id]?.model.trim()).length,
    total: defs.length,
  };
}

/**
 * A plausible mapping seeded from the provider's catalogue, for the first bind.
 *
 * Exported for the "one-click" affordance and its test; it produces a value to
 * be *saved*, not a rendering default — a seed the user never confirmed must
 * not appear filled in.
 */
export function seedClaudeRoles(
  provider: ProviderEntryFlat,
  defs: RoleDefDto[],
  catalog?: string[],
): Record<string, RoleTarget> {
  const models = catalog?.length ? catalog : providerModels(provider);
  const pick = (i: number) => models[i] ?? models[0] ?? provider.default_model ?? "";
  const next: Record<string, RoleTarget> = {};
  defs.forEach((def, i) => {
    const model = pick(Math.min(i, Math.max(models.length - 1, 0))) || pick(0);
    if (model) next[def.id] = { provider_id: provider.id, model };
  });
  return next;
}

/**
 * Broadcast one model onto every role (cc-switch semantics). Prefers an
 * already-filled role, then `default_model`, then the catalogue head.
 */
export function oneClickClaudeRoles(
  roles: Record<string, RoleTarget>,
  defs: RoleDefDto[],
  providerId: string,
  catalog: string[],
  defaultModel: string,
): Record<string, RoleTarget> | null {
  const source =
    defs.map((def) => roles[def.id]?.model.trim()).find(Boolean) || defaultModel.trim() || catalog[0]?.trim() || "";
  if (!source) return null;
  return Object.fromEntries(defs.map((def) => [def.id, { provider_id: providerId, model: source }]));
}

type ClaudeMappingPanelProps = {
  provider: ProviderEntryFlat;
  /** The agent this panel edits — CLI and Desktop are separate bindings. */
  toolId: string;
  binding: ToolBinding | null;
  onClose?: () => void;
  onUnbind?: () => void;
  /** "popover" = compact floating; "page" = full main-pane section. */
  chrome?: "popover" | "page";
  /** Footer note about where this surface persists. */
  diskHint?: string;
};

/**
 * Claude role → model mapping.
 *
 * Every change persists through `update_agent_settings`, which lands in
 * `AgentBinding.roles` and from there in `~/.claude/settings.json`'s env block.
 * That chain is the point of this component: the panel used to hold its value in
 * a `useState` that nothing ever read, so the form accepted input, showed it
 * back, and wrote nothing — the backend had been reading the tier models from
 * their old home for three versions and never received any.
 */
export function ClaudeMappingPanel({
  provider,
  toolId,
  binding,
  onClose,
  onUnbind,
  chrome = "popover",
  diskHint,
}: ClaudeMappingPanelProps) {
  const { t } = useTranslation();
  // Defaulted here rather than in the parameter list: `t` only exists once the
  // hook has run, and the fallback copy has to be translatable too.
  const diskHintText = diskHint ?? t("models.claudeMapping.diskHint");
  const { updateProvider, updateToolBindingSettings } = useProvidersFlat();
  const { fetchModelCatalog, isLoading: isFetchingModels } = useModelFetch();
  const [extraModels, setExtraModels] = useState<string[]>([]);
  const descriptor = useAgentDescriptor(toolId);
  const drops = useRoleDrops(toolId);

  const defs = descriptor?.roles.length ? descriptor.roles : CLAUDE_FALLBACK_ROLES;
  const roles = bindingRoles(binding);

  const models = useMemo(
    () => buildModelCatalog([...providerModels(provider), ...extraModels]),
    [provider, extraModels],
  );
  const canOneClick = Boolean(
    defs.some((def) => roles[def.id]?.model.trim()) || provider.default_model.trim() || models.length > 0,
  );
  const canFetch = Boolean(provider.models_url?.trim() && provider.api_key?.trim());
  const { filled, total } = claudeFillCount(roles, defs);

  const persist = (next: Record<string, RoleTarget>) => {
    void updateToolBindingSettings(toolId, { roles: next }).catch(() => {});
  };

  const setRoleModel = (roleId: string, model: string) => {
    const next = { ...roles };
    if (!model.trim()) delete next[roleId];
    else next[roleId] = { provider_id: provider.id, model };
    persist(next);
  };

  const handleOneClick = () => {
    const next = oneClickClaudeRoles(roles, defs, provider.id, models, provider.default_model);
    if (!next) {
      toast.error(t("models.claudeMapping.quickSetEmpty"));
      return;
    }
    persist(next);
    toast.success(t("models.claudeMapping.quickSetSuccess"));
  };

  const handleFetchModels = async () => {
    const url = provider.models_url?.trim() ?? "";
    const apiKey = provider.api_key?.trim() ?? "";
    if (!url || !apiKey) {
      toast.error(t("models.modelsTab.fetchRequirement"));
      return;
    }
    try {
      const result = await fetchModelCatalog(provider.id);
      const merged = buildModelCatalog([...provider.models, ...result.models]);
      setExtraModels(result.models);
      await updateProvider(provider.id, {
        models: merged,
        meta: {
          ...(provider.meta ?? {}),
          [MODEL_CATALOG_META_KEY]: result.catalog,
        },
      });
      toast.success(t("models.toasts.fetchedModels", { count: result.models.length }));
      if (result.missing_cost_count > 0) {
        toast.message(t("models.toasts.missingCost", { count: result.missing_cost_count }));
      }
    } catch (error) {
      toast.error(
        t("models.toasts.fetchModelsFailed", {
          message: error instanceof Error ? error.message : String(error),
        }),
      );
    }
  };

  return (
    <div
      className={cn(
        "bg-card text-foreground",
        chrome === "popover" && "w-[min(560px,92vw)] rounded-xl border border-border/60 shadow-xl",
        chrome === "page" && "rounded-2xl border border-border/55",
      )}
    >
      <div className="flex items-start justify-between gap-3 border-b border-border/40 px-4 py-3">
        <div className="min-w-0">
          <h3 className="text-sm font-semibold">{t("models.claudeMapping.title")}</h3>
          <p className="mt-0.5 text-[11px] leading-snug text-muted-foreground">
            {t("models.claudeMapping.fallbackHint")}
          </p>
          <p className="mt-1 font-mono text-[10px] text-muted-foreground">
            {provider.name} · {filled}/{total} {t("models.claudeMapping.filledSuffix")}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          <Button size="xs" variant="outline" disabled={!canOneClick} onClick={handleOneClick}>
            <Sparkles className="mr-1 h-3 w-3" />
            {t("models.claudeMapping.quickSet")}
          </Button>
          <span title={canFetch ? undefined : t("models.modelsTab.fetchRequirement")}>
            <Button
              size="xs"
              variant="outline"
              disabled={!canFetch || isFetchingModels}
              onClick={() => void handleFetchModels()}
            >
              {isFetchingModels ? (
                <Loader2 className="mr-1 h-3 w-3 animate-spin" />
              ) : (
                <Download className="mr-1 h-3 w-3" />
              )}
              {t("models.claudeMapping.fetchModels")}
            </Button>
          </span>
          {onClose ? (
            <button
              type="button"
              onClick={onClose}
              className="rounded-lg p-1 text-muted-foreground hover:bg-muted/50 hover:text-foreground"
              aria-label={t("models.common.close")}
            >
              <X className="h-4 w-4" />
            </button>
          ) : null}
        </div>
      </div>

      <div className="space-y-2 px-4 py-3">
        <div className="grid grid-cols-[86px_1fr_1.2fr] gap-2 px-0.5 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
          <span>{t("models.claudeMapping.colRole")}</span>
          <span>{t("models.claudeMapping.colRequest")}</span>
          <span>{t("models.claudeMapping.colWrites")}</span>
        </div>

        {defs.map((def) => (
          <ClaudeRoleRow
            key={def.id}
            def={def}
            target={roles[def.id]}
            options={models}
            drop={drops.find((drop) => drop.role === def.id) ?? null}
            onModelChange={(model) => setRoleModel(def.id, model)}
          />
        ))}
      </div>

      {(onUnbind || chrome === "page") && (
        <div className="flex items-center justify-between gap-2 border-t border-border/40 px-4 py-2.5">
          <p className="text-[10px] text-muted-foreground">{diskHintText}</p>
          <div className="flex gap-1.5">
            {onUnbind ? (
              <Button size="xs" variant="ghost" className="text-destructive" onClick={onUnbind}>
                <Unplug className="mr-1 h-3 w-3" />
                {t("models.claudeMapping.unbind")}
              </Button>
            ) : null}
            {onClose ? (
              <Button size="xs" onClick={onClose}>
                {t("models.claudeMapping.done")}
              </Button>
            ) : null}
          </div>
        </div>
      )}
    </div>
  );
}

/**
 * One role row: the model that serves it, plus the env key it writes and what
 * an empty row falls back to.
 *
 * The env key is shown rather than hidden because it is the answer to "what does
 * this row actually do" — the same question the panel could not answer at all
 * while its state went nowhere.
 */
function ClaudeRoleRow({
  def,
  target,
  options,
  drop,
  onModelChange,
}: {
  def: RoleDefDto;
  target: RoleTarget | undefined;
  options: string[];
  drop: DroppedRole | null;
  onModelChange: (model: string) => void;
}) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState<string | null>(null);
  const [listId] = useState(() => `claude-models-${def.id}-${Math.random().toString(36).slice(2, 8)}`);
  const value = draft ?? target?.model ?? "";

  const commit = () => {
    if (draft === null) return;
    setDraft(null);
    if (draft !== (target?.model ?? "")) onModelChange(draft);
  };

  return (
    <div className="grid grid-cols-[86px_1fr_1.2fr] items-start gap-2">
      <span className="inline-flex h-9 items-center justify-center rounded-lg border border-border/50 bg-muted/30 px-2 text-[11px] font-semibold">
        {roleLabel(def.id)}
      </span>
      <div className="min-w-0">
        <input
          list={listId}
          className="h-9 w-full rounded-lg border border-border/55 bg-background px-2.5 font-mono text-[11px] outline-none focus:ring-1 focus:ring-primary/35"
          placeholder="model id"
          aria-label={`${roleLabel(def.id)} model`}
          value={value}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === "Enter") e.currentTarget.blur();
          }}
        />
        <datalist id={listId}>
          {options.map((m) => (
            <option key={m} value={m} />
          ))}
        </datalist>
      </div>
      <div className="min-w-0 pt-0.5">
        <p className="truncate font-mono text-[10px] leading-4 text-muted-foreground/85">
          {t("models.roles.writesTo", { key: def.agent_key })}
        </p>
        {!target?.model.trim() ? (
          <p className="truncate text-[10px] leading-4 text-muted-foreground/70">
            {def.inherits ? t("models.roles.inheritsFrom", { role: def.inherits }) : t("models.roles.agentDecides")}
          </p>
        ) : null}
        {drop ? (
          <p className="flex items-start gap-1 text-[10px] leading-4 text-amber-500">
            <AlertTriangle className="mt-px h-3 w-3 shrink-0" />
            <span>{t(`models.roleDrops.reason.${drop.reason}`)}</span>
          </p>
        ) : null}
      </div>
    </div>
  );
}
