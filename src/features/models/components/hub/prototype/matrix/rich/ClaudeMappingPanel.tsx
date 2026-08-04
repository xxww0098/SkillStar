import { Download, Loader2, Sparkles, Unplug, X } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "../../../../../../../components/ui/button";
import { cn } from "../../../../../../../lib/utils";
import type { ProviderEntryFlat } from "../../../../../../../types";
import { useModelFetch } from "../../../../../api/modelCatalog";
import { useProvidersFlat } from "../../../../../hooks/useProvidersFlat";
import { MODEL_CATALOG_META_KEY, buildModelCatalog } from "../../../../../lib/providerPatch";
import { providerModels } from "./RichMatrixShell";

/** Screenshot-aligned Claude Code roles. */
export const CLAUDE_MAP_ROLES = [
  { id: "sonnet", label: "Sonnet", showInMenu: true },
  { id: "opus", label: "Opus", showInMenu: true },
  { id: "fable", label: "Fable", showInMenu: true },
  { id: "haiku", label: "Haiku", showInMenu: true },
  { id: "subagent", label: "Subagent", showInMenu: false },
] as const;

export type ClaudeMapRoleId = (typeof CLAUDE_MAP_ROLES)[number]["id"];

export type ClaudeMapRow = {
  displayName: string;
  requestModel: string;
  support1M: boolean;
};

export type ClaudeMapState = Record<ClaudeMapRoleId, ClaudeMapRow>;

export function emptyClaudeMap(): ClaudeMapState {
  return {
    sonnet: { displayName: "", requestModel: "", support1M: false },
    opus: { displayName: "", requestModel: "", support1M: false },
    fable: { displayName: "", requestModel: "", support1M: false },
    haiku: { displayName: "", requestModel: "", support1M: false },
    subagent: { displayName: "", requestModel: "", support1M: false },
  };
}

/** Seed a plausible mapping from the bound provider's catalog (initial bind). */
export function seedClaudeMap(provider: ProviderEntryFlat, catalog?: string[]): ClaudeMapState {
  const models = catalog?.length ? catalog : providerModels(provider);
  const pick = (i: number) => models[i] ?? models[0] ?? provider.default_model ?? "";
  return {
    sonnet: { displayName: "", requestModel: pick(0), support1M: false },
    opus: { displayName: "", requestModel: pick(1) || pick(0), support1M: false },
    fable: { displayName: "", requestModel: pick(2) || pick(0), support1M: false },
    haiku: { displayName: "", requestModel: pick(Math.min(3, Math.max(models.length - 1, 0))) || pick(0), support1M: false },
    subagent: { displayName: "", requestModel: pick(0), support1M: false },
  };
}

/**
 * 「一键设置」— broadcast one model onto every role (cc-switch semantics).
 * Prefers an already-filled role model, then default_model, then catalog[0].
 */
export function oneClickClaudeMap(
  value: ClaudeMapState,
  catalog: string[],
  defaultModel: string,
): ClaudeMapState | null {
  const source =
    CLAUDE_MAP_ROLES.map((r) => value[r.id].requestModel.trim()).find(Boolean) ||
    defaultModel.trim() ||
    catalog[0]?.trim() ||
    "";
  if (!source) return null;

  const sourceRow = CLAUDE_MAP_ROLES.map((r) => value[r.id]).find((r) => r.requestModel.trim() === source);
  const support1M = sourceRow?.support1M ?? false;
  const next = emptyClaudeMap();
  for (const role of CLAUDE_MAP_ROLES) {
    next[role.id] = {
      displayName: role.showInMenu ? source : "",
      requestModel: source,
      support1M,
    };
  }
  return next;
}

export function mappingFillCount(map: ClaudeMapState): { filled: number; total: number } {
  const total = CLAUDE_MAP_ROLES.length;
  const filled = CLAUDE_MAP_ROLES.filter((r) => map[r.id].requestModel.trim()).length;
  return { filled, total };
}

type ClaudeMappingPanelProps = {
  provider: ProviderEntryFlat;
  value: ClaudeMapState;
  onChange: (next: ClaudeMapState) => void;
  onClose?: () => void;
  onUnbind?: () => void;
  onStub: (action: string, detail?: Record<string, unknown>) => void;
  /** "popover" = compact floating; "page" = full main-pane section. */
  chrome?: "popover" | "page";
  /** Footer note about where this surface persists. */
  diskHint?: string;
};

/**
 * Claude role → display / request / 1M mapping.
 * 「一键设置」广播同一模型到全部角色；「获取模型列表」拉取并写回 Provider.models。
 */
export function ClaudeMappingPanel({
  provider,
  value,
  onChange,
  onClose,
  onUnbind,
  onStub,
  chrome = "popover",
  diskHint = "写入 ~/.claude/settings.json",
}: ClaudeMappingPanelProps) {
  const { t } = useTranslation();
  const { updateProvider } = useProvidersFlat();
  const { fetchModelCatalog, isLoading: isFetchingModels } = useModelFetch();
  const [extraModels, setExtraModels] = useState<string[]>([]);

  const models = useMemo(
    () => buildModelCatalog([...providerModels(provider), ...extraModels]),
    [provider, extraModels],
  );
  const canOneClick = Boolean(
    CLAUDE_MAP_ROLES.some((r) => value[r.id].requestModel.trim()) ||
      provider.default_model.trim() ||
      models.length > 0,
  );
  const canFetch = Boolean(provider.models_url?.trim() && provider.api_key?.trim());
  const { filled, total } = mappingFillCount(value);

  const setRow = (id: ClaudeMapRoleId, patch: Partial<ClaudeMapRow>) => {
    onChange({ ...value, [id]: { ...value[id], ...patch } });
  };

  const handleOneClick = () => {
    const next = oneClickClaudeMap(value, models, provider.default_model);
    if (!next) {
      toast.error(t("models.claudeMapping.quickSetEmpty"));
      return;
    }
    onChange(next);
    onStub("claudeOneClickMap", { providerId: provider.id, next });
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
      const result = await fetchModelCatalog(url, apiKey);
      const merged = buildModelCatalog([...provider.models, ...result.models]);
      setExtraModels(result.models);
      await updateProvider(provider.id, {
        models: merged,
        meta: {
          ...(provider.meta ?? {}),
          [MODEL_CATALOG_META_KEY]: result.catalog,
        },
      });
      onStub("fetchModels", { providerId: provider.id, count: result.models.length });
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
            {t("models.claudeMapping.subtitle")}
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
              aria-label="Close"
            >
              <X className="h-4 w-4" />
            </button>
          ) : null}
        </div>
      </div>

      <div className="space-y-2 px-4 py-3">
        <div className="grid grid-cols-[72px_1fr_1.2fr_56px] gap-2 px-0.5 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
          <span>{t("models.claudeMapping.colRole")}</span>
          <span>{t("models.claudeMapping.colDisplay")}</span>
          <span>{t("models.claudeMapping.colRequest")}</span>
          <span className="text-center">1M</span>
        </div>

        {CLAUDE_MAP_ROLES.map((role) => {
          const row = value[role.id];
          return (
            <div key={role.id} className="grid grid-cols-[72px_1fr_1.2fr_56px] items-center gap-2">
              <span className="inline-flex h-8 items-center justify-center rounded-lg border border-border/50 bg-muted/30 px-2 text-[11px] font-semibold">
                {role.label}
              </span>
              <input
                className={cn(
                  "h-9 rounded-lg border border-border/55 bg-background px-2.5 text-xs outline-none focus:ring-1 focus:ring-primary/35",
                  !role.showInMenu && "bg-muted/40 text-muted-foreground",
                )}
                placeholder={
                  role.showInMenu
                    ? t("models.claudeMapping.displayPlaceholder")
                    : t("models.claudeMapping.displayHidden")
                }
                value={row.displayName}
                disabled={!role.showInMenu}
                onChange={(e) => setRow(role.id, { displayName: e.target.value })}
                onBlur={() =>
                  onStub("claudeMapDisplayName", {
                    role: role.id,
                    displayName: row.displayName,
                  })
                }
              />
              <ModelRequestInput
                value={row.requestModel}
                options={models}
                onChange={(requestModel) => setRow(role.id, { requestModel })}
                onCommit={() =>
                  onStub("claudeMapRequestModel", {
                    role: role.id,
                    requestModel: row.requestModel,
                  })
                }
              />
              <button
                type="button"
                onClick={() => {
                  const support1M = !row.support1M;
                  setRow(role.id, { support1M });
                  onStub("claudeMap1M", { role: role.id, support1M });
                }}
                className="inline-flex items-center justify-center gap-1 text-[11px] text-muted-foreground"
                title={t("models.claudeMapping.oneMTitle")}
              >
                <span
                  className={cn(
                    "flex h-4 w-4 items-center justify-center rounded-full border",
                    row.support1M ? "border-primary bg-primary text-[9px] text-primary-foreground" : "border-border/70",
                  )}
                >
                  {row.support1M ? "✓" : null}
                </span>
                1M
              </button>
            </div>
          );
        })}
      </div>

      {(onUnbind || chrome === "page") && (
        <div className="flex items-center justify-between gap-2 border-t border-border/40 px-4 py-2.5">
          <p className="text-[10px] text-muted-foreground">{diskHint}</p>
          <div className="flex gap-1.5">
            {onUnbind ? (
              <Button size="xs" variant="ghost" className="text-destructive" onClick={onUnbind}>
                <Unplug className="mr-1 h-3 w-3" />
                {t("models.claudeMapping.unbind")}
              </Button>
            ) : null}
            <Button
              size="xs"
              onClick={() => {
                onStub("claudeMapSave", { providerId: provider.id, value });
                onClose?.();
              }}
            >
              {t("models.claudeMapping.done")}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}

function ModelRequestInput({
  value,
  options,
  onChange,
  onCommit,
}: {
  value: string;
  options: string[];
  onChange: (v: string) => void;
  onCommit: () => void;
}) {
  const [listId] = useState(() => `claude-models-${Math.random().toString(36).slice(2, 8)}`);
  return (
    <div className="min-w-0">
      <input
        list={listId}
        className="h-9 w-full rounded-lg border border-border/55 bg-background px-2.5 font-mono text-[11px] outline-none focus:ring-1 focus:ring-primary/35"
        placeholder="model id"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onBlur={onCommit}
      />
      <datalist id={listId}>
        {options.map((m) => (
          <option key={m} value={m} />
        ))}
      </datalist>
    </div>
  );
}
