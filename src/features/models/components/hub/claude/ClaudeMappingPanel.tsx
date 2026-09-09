import { AlertTriangle, Download, Loader2, Sparkles } from "lucide-react";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "../../../../../components/ui/button";
import { cn } from "../../../../../lib/utils";
import type { DroppedRole, ProviderEntryFlat, RoleTarget, ToolBinding } from "../../../../../types";
import type { RoleDefDto } from "../../../../../types/generated/RoleDefDto";
import { useAgentDescriptor, useRoleDrops } from "../../../api/agents";
import { useModelFetch } from "../../../api/modelCatalog";
import { useProvidersFlat } from "../../../hooks/useProvidersFlat";
import { isNativeOfficialProvider } from "../../../lib/officialProviders";
import { MODEL_CATALOG_META_KEY, buildModelCatalog } from "../../../lib/providerPatch";
import { activeEntry, bindingRoles } from "../../../lib/toolBinding";
import { modelInputClass } from "../../providerForm/ProviderConfigPrimitives";

function providerModels(provider: ProviderEntryFlat): string[] {
  return buildModelCatalog(provider.models.length ? provider.models : [provider.default_model]);
}

/** Claude's tier names are also their display labels. Unknown registry ids stay visible. */
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

export function claudeFillCount(
  roles: Record<string, RoleTarget>,
  defs: RoleDefDto[],
): { filled: number; total: number } {
  return { filled: defs.filter((def) => roles[def.id]?.model.trim()).length, total: defs.length };
}

/** A proposed assignment, never a rendering fallback or an implicit write. */
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

/** Broadcast a filled role, default model, or catalog head to declared roles only. */
export function oneClickClaudeRoles(
  roles: Record<string, RoleTarget>,
  defs: RoleDefDto[],
  providerId: string,
  catalog: string[],
  defaultModel: string,
): Record<string, RoleTarget> | null {
  const source =
    defs.map((def) => roles[def.id]?.model.trim()).find(Boolean) || defaultModel.trim() || catalog[0]?.trim() || "";
  if (!source || defs.length === 0) return null;
  return Object.fromEntries(defs.map((def) => [def.id, { provider_id: providerId, model: source }]));
}

type ClaudeMappingPanelProps = {
  provider: ProviderEntryFlat;
  /** Only claude-code can write; other ids render a read-only explanation. */
  toolId: string;
  binding: ToolBinding | null;
  /** Lock parent navigation while writing or retaining a local draft. */
  onBusyChange?: (busy: boolean) => void;
};
type Feedback = { kind: "success" | "error" | "warning"; message: string };

/** Inline editor for an applied API binding. Drafts never cross provider/agent boundaries. */
export function ClaudeMappingPanel(props: ClaudeMappingPanelProps) {
  return <ClaudeMappingEditor key={props.toolId + ":" + props.provider.id} {...props} />;
}

function ClaudeMappingEditor({ provider, toolId, binding, onBusyChange }: ClaudeMappingPanelProps) {
  const { t } = useTranslation();
  const titleId = useId();
  const { updateProvider, updateToolBindingSettings } = useProvidersFlat();
  const { fetchModelCatalog, isLoading: isFetchingModels } = useModelFetch();
  const descriptor = useAgentDescriptor(toolId);
  const drops = useRoleDrops(toolId);
  const [draftModels, setDraftModels] = useState<Record<string, string>>({});
  const [extraModels, setExtraModels] = useState<string[]>([]);
  const [busy, setBusy] = useState<"roles" | "catalog" | null>(null);
  const [roleFeedback, setRoleFeedback] = useState<Feedback | null>(null);
  const [catalogFeedback, setCatalogFeedback] = useState<Feedback | null>(null);
  const hasDrafts = Object.keys(draftModels).length > 0;
  useEffect(() => {
    onBusyChange?.(busy !== null || isFetchingModels || hasDrafts);
    return () => onBusyChange?.(false);
  }, [busy, isFetchingModels, hasDrafts, onBusyChange]);
  const inFlight = useRef(false);
  const mounted = useRef(true);
  const latestProvider = useRef(provider);
  latestProvider.current = provider;
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  // An absent or empty descriptor grants no capability. Desktop's marker is not native sync.
  const defs = toolId === "claude-code" && descriptor?.id === toolId ? descriptor.roles : [];
  const applied = activeEntry(binding)?.provider_id === provider.id && !isNativeOfficialProvider(provider);
  const canEdit = toolId === "claude-code" && applied && defs.length > 0;
  const savedRoles = bindingRoles(binding);
  const roles = { ...savedRoles };
  for (const def of defs) {
    const model = draftModels[def.id];
    if (model === undefined) continue;
    if (!model.trim()) delete roles[def.id];
    else roles[def.id] = { ...roles[def.id], provider_id: provider.id, model: model.trim() };
  }
  const models = useMemo(
    () => buildModelCatalog([...providerModels(provider), ...extraModels]),
    [provider, extraModels],
  );
  const canOneClick = canEdit && Boolean(oneClickClaudeRoles(roles, defs, provider.id, models, provider.default_model));
  const canFetch = canEdit && Boolean(provider.models_url?.trim() && provider.api_key?.trim());
  const disabled = !canEdit || busy !== null || isFetchingModels;
  const { filled, total } = claudeFillCount(roles, defs);

  // Retain drafts through optimistic cache changes and rollback. Only a confirmed
  // write followed by its cache echo releases the local overlay.
  useEffect(() => {
    if (roleFeedback?.kind !== "success" || busy !== null || Object.keys(draftModels).length === 0) return;
    const persisted = bindingRoles(binding);
    if (
      Object.entries(draftModels).every(([id, model]) =>
        model ? persisted[id]?.model === model && persisted[id]?.provider_id === provider.id : !persisted[id],
      )
    ) {
      setDraftModels({});
    }
  }, [binding, busy, draftModels, roleFeedback, provider.id]);

  const persist = async (next: Record<string, RoleTarget>, editedIds = Object.keys(draftModels)): Promise<boolean> => {
    if (!canEdit || inFlight.current) return false;
    inFlight.current = true;
    setBusy("roles");
    setRoleFeedback(null);
    setDraftModels(Object.fromEntries(editedIds.map((id) => [id, next[id]?.model ?? ""])));
    try {
      const result = await updateToolBindingSettings(toolId, { ...binding?.settings, roles: next });
      if (!mounted.current) return false;
      if (result?.success !== true) throw new Error(result?.error || t("models.claudeMapping.writeUnconfirmed"));
      const skipped = (result.dropped_roles?.length ?? 0) > 0;
      setRoleFeedback({
        kind: skipped ? "warning" : "success",
        message: t(skipped ? "models.claudeMapping.savedWithDrops" : "models.claudeMapping.saved"),
      });
      return !skipped;
    } catch (error) {
      if (mounted.current) {
        const message = t("models.claudeMapping.saveFailed", {
          message: error instanceof Error ? error.message : String(error),
        });
        setRoleFeedback({ kind: "error", message });
        toast.error(message);
      }
      return false;
    } finally {
      inFlight.current = false;
      if (mounted.current) setBusy(null);
    }
  };

  const handleOneClick = async () => {
    if (!canEdit || inFlight.current) return;
    const assignments = oneClickClaudeRoles(roles, defs, provider.id, models, provider.default_model);
    if (!assignments) {
      const message = t("models.claudeMapping.quickSetEmpty");
      setRoleFeedback({ kind: "error", message });
      toast.error(message);
      return;
    }
    const next = { ...roles };
    for (const [id, target] of Object.entries(assignments)) next[id] = { ...next[id], ...target };
    if (
      await persist(
        next,
        defs.map((def) => def.id),
      )
    )
      toast.success(t("models.claudeMapping.quickSetSuccess"));
  };

  const handleFetchModels = async () => {
    if (!canFetch || inFlight.current) return;
    inFlight.current = true;
    setBusy("catalog");
    setCatalogFeedback(null);
    try {
      const result = await fetchModelCatalog(provider.id);
      if (!mounted.current) return;
      const current = latestProvider.current;
      await updateProvider(provider.id, {
        models: buildModelCatalog([...current.models, ...result.models]),
        meta: { ...(current.meta ?? {}), [MODEL_CATALOG_META_KEY]: result.catalog },
      });
      if (!mounted.current) return;
      setExtraModels(result.models);
      const message = t("models.claudeMapping.fetchedModels", { count: result.models.length });
      setCatalogFeedback({ kind: "success", message });
      toast.success(message);
      if (result.missing_cost_count > 0)
        toast.message(t("models.claudeMapping.missingCost", { count: result.missing_cost_count }));
    } catch (error) {
      if (mounted.current) {
        const message = t("models.claudeMapping.fetchFailed", {
          message: error instanceof Error ? error.message : String(error),
        });
        setCatalogFeedback({ kind: "error", message });
        toast.error(message);
      }
    } finally {
      inFlight.current = false;
      if (mounted.current) setBusy(null);
    }
  };

  let unavailable: string | null = null;
  if (toolId !== "claude-code") unavailable = t("models.claudeMapping.cliOnly");
  else if (!applied) unavailable = t("models.claudeMapping.appliedApiRequired");
  else if (!descriptor) unavailable = t("models.claudeMapping.loadingRoles");
  else if (defs.length === 0) unavailable = t("models.claudeMapping.noRoles");

  return (
    <section
      aria-labelledby={titleId}
      aria-busy={busy !== null}
      className="@container min-w-0 rounded-2xl border border-border/55 bg-card text-foreground"
    >
      <div className="space-y-4 border-b border-border/50 p-5">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0 flex-1 basis-64">
            <h3 id={titleId} className="text-base font-semibold">
              {t("models.claudeMapping.title")}
            </h3>
            <p className="mt-1 max-w-prose text-[13px] leading-6 text-muted-foreground">
              {t("models.claudeMapping.description")}
            </p>
          </div>
          {canEdit ? (
            <p className="text-[13px] leading-6 text-muted-foreground">
              {t("models.claudeMapping.assignedCount", { filled, total })}
            </p>
          ) : null}
        </div>
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            variant="outline"
            className="h-auto min-h-10 max-w-full shrink whitespace-normal py-2 text-[13px]"
            disabled={disabled || !canOneClick}
            onClick={() => void handleOneClick()}
          >
            <Sparkles className="size-4" aria-hidden="true" />
            {t("models.claudeMapping.quickSet")}
          </Button>
          <Button
            type="button"
            variant="outline"
            className="h-auto min-h-10 max-w-full shrink whitespace-normal py-2 text-[13px]"
            disabled={disabled || !canFetch}
            onClick={() => void handleFetchModels()}
          >
            {busy === "catalog" ? (
              <Loader2 className="size-4 animate-spin motion-reduce:animate-none" aria-hidden="true" />
            ) : (
              <Download className="size-4" aria-hidden="true" />
            )}
            {t("models.claudeMapping.fetchModels")}
          </Button>
        </div>
        {canEdit && !canFetch ? (
          <p className="text-[13px] leading-6 text-muted-foreground">{t("models.claudeMapping.fetchRequirement")}</p>
        ) : null}
      </div>
      {unavailable ? (
        <p role="status" className="p-5 text-[13px] leading-6 text-muted-foreground">
          {unavailable}
        </p>
      ) : (
        <div className="divide-y divide-border/50 px-5">
          <div className="hidden grid-cols-[7rem_minmax(0,1fr)_minmax(0,1.1fr)] gap-4 py-3 text-[13px] font-medium text-muted-foreground @2xl:grid">
            <span>{t("models.claudeMapping.colRole")}</span>
            <span>{t("models.claudeMapping.colRequest")}</span>
            <span>{t("models.claudeMapping.colWrites")}</span>
          </div>
          {defs.map((def) => (
            <ClaudeRoleRow
              key={def.id}
              def={def}
              value={draftModels[def.id] ?? roles[def.id]?.model ?? ""}
              options={models}
              disabled={disabled}
              drop={drops.find((drop) => drop.role === def.id) ?? null}
              onChange={(model) => {
                setDraftModels((prev) => ({ ...prev, [def.id]: model }));
                if (roleFeedback?.kind === "success") setRoleFeedback(null);
              }}
              onCommit={() => {
                if (Object.hasOwn(draftModels, def.id)) void persist(roles);
              }}
            />
          ))}
        </div>
      )}
      {canEdit ? (
        <div className="space-y-3 border-t border-border/50 p-5 text-[13px] leading-6">
          <p className="text-muted-foreground">{t("models.claudeMapping.diskHint")}</p>
          {busy ? (
            <p role="status" className="flex items-center gap-2">
              <Loader2 className="size-4 animate-spin motion-reduce:animate-none" aria-hidden="true" />
              {t(busy === "roles" ? "models.claudeMapping.saving" : "models.claudeMapping.fetching")}
            </p>
          ) : null}
          {roleFeedback ? <MappingFeedback feedback={roleFeedback} /> : null}
          {hasDrafts || roleFeedback?.kind === "error" ? (
            <div className="flex flex-wrap gap-2">
              {roleFeedback?.kind === "error" ? (
                <Button type="button" variant="outline" disabled={disabled} onClick={() => void persist(roles)}>
                  {t("models.claudeMapping.retrySave")}
                </Button>
              ) : null}
              {hasDrafts ? (
                <Button
                  type="button"
                  variant="ghost"
                  disabled={disabled}
                  // Keep focus in the input so discarding does not first trigger blur autosave.
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => {
                    if (inFlight.current) return;
                    setDraftModels({});
                    setRoleFeedback(null);
                  }}
                >
                  {t("models.claudeMapping.discardDraft")}
                </Button>
              ) : null}
            </div>
          ) : null}
          {catalogFeedback ? <MappingFeedback feedback={catalogFeedback} /> : null}
        </div>
      ) : null}
    </section>
  );
}

function MappingFeedback({ feedback }: { feedback: Feedback }) {
  return (
    <p
      role={feedback.kind === "success" ? "status" : "alert"}
      className={cn(
        "break-words text-[13px] leading-6",
        feedback.kind === "error"
          ? "text-destructive"
          : feedback.kind === "warning"
            ? "text-amber-700 dark:text-amber-300"
            : "text-muted-foreground",
      )}
    >
      {feedback.message}
    </p>
  );
}

function ClaudeRoleRow({
  def,
  value,
  options,
  disabled,
  drop,
  onChange,
  onCommit,
}: {
  def: RoleDefDto;
  value: string;
  options: string[];
  disabled: boolean;
  drop: DroppedRole | null;
  onChange: (model: string) => void;
  onCommit: () => void;
}) {
  const { t } = useTranslation();
  const inputId = useId();
  const listId = inputId + "-models";
  const hintId = inputId + "-hint";
  return (
    <div className="grid min-w-0 grid-cols-1 gap-2 py-4 @2xl:grid-cols-[7rem_minmax(0,1fr)_minmax(0,1.1fr)] @2xl:gap-4">
      <label htmlFor={inputId} className="text-[13px] font-semibold leading-6 @2xl:pt-2">
        {roleLabel(def.id)}
      </label>
      <div className="min-w-0">
        <input
          id={inputId}
          list={listId}
          className={cn(modelInputClass, "w-full min-w-0 font-mono")}
          placeholder={t("models.claudeMapping.modelPlaceholder")}
          aria-label={roleLabel(def.id) + " model"}
          aria-describedby={hintId}
          disabled={disabled}
          value={value}
          onChange={(event) => onChange(event.target.value)}
          onBlur={onCommit}
          onKeyDown={(event) => {
            if (event.key === "Enter") event.currentTarget.blur();
          }}
        />
        <datalist id={listId}>
          {options.map((model) => (
            <option key={model} value={model} />
          ))}
        </datalist>
      </div>
      <div id={hintId} className="min-w-0 space-y-1 text-[13px] leading-6">
        <p className="font-mono text-muted-foreground [overflow-wrap:anywhere]">{def.agent_key}</p>
        {!value.trim() ? (
          <p className="text-muted-foreground">
            {def.inherits
              ? t("models.claudeMapping.inheritsFrom", { role: def.inherits })
              : t(
                  def.id === "default"
                    ? "models.claudeMapping.activeDefaultFallback"
                    : "models.claudeMapping.agentDecides",
                )}
          </p>
        ) : null}
        {drop ? (
          <p className="flex items-start gap-2 text-amber-700 dark:text-amber-300">
            <AlertTriangle className="mt-1 size-4 shrink-0" aria-hidden="true" />
            <span className="min-w-0 break-words">{t("models.claudeMapping.dropReason." + drop.reason)}</span>
          </p>
        ) : null}
      </div>
    </div>
  );
}
