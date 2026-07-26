import { Cable, ExternalLink, FileCode2, RefreshCw, SlidersHorizontal, Terminal, Unplug } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "../../../../components/ui/button";
import { ExternalAnchor } from "../../../../components/ui/ExternalAnchor";
import { ModalHeader, ModalShell } from "../../../../components/ui/ModalShell";
import { useProviderMetaPatch } from "../../api/providers";
import { useAgentActivation } from "../../hooks/useAgentActivation";
import { useAutosave } from "../../hooks/useAutosave";
import type { ProviderToolId } from "../../lib/agentRegistry";
import { computeAgentStatus } from "../../lib/agentStatus";
import { buildModelCatalog, getModelCatalogFromMeta } from "../../lib/providerPatch";
import {
  CLAUDE_MODEL_META_KEYS,
  CODEX_AUTH_MODE_META_KEY,
  CODEX_WIRE_API_META_KEY,
  type CodexAuthMode,
  type CodexWireApi,
  getMetaString,
  LATEST_CLAUDE_MODELS,
  providerCodexAuthMode,
  providerCodexWireApi,
} from "../../lib/providerPatch";
import { formatSyncTime } from "../../lib/modelFormat";
import type { ProviderEditorTab, SaveAttemptResult } from "../../types";
import { ConflictWarnings } from "../diagnostics/ConflictWarnings";
import { ModelFormField, ModelFormSection } from "../providerForm/ProviderConfigPrimitives";
import { AgentToolIcon } from "../shared/AgentToolIcon";
import { ModelSelectPopover } from "../shared/ModelSelectPopover";
import { ProviderSelectPopover } from "../shared/ProviderSelectPopover";
import { SaveBadge } from "../shared/SaveBadge";
import { AgentConfigFiles } from "./AgentConfigFiles";
import { AgentLaunchCommand } from "./AgentLaunchCommand";
import { AgentStatusPill } from "./AgentStatusPill";
import { ClaudeModelMapping, type ClaudeModelMappingValues } from "./ClaudeModelMapping";
import { CodexSettingsForm } from "./CodexSettingsForm";

export interface AgentSettingsDialogProps {
  toolId: ProviderToolId;
  open: boolean;
  onClose: () => void;
  onAddProvider: () => void;
  /** Open the provider editor drawer (e.g. to manage the model list). */
  onOpenProviderDrawer: (providerId: string, initialTab?: ProviderEditorTab) => void;
}

interface AgentParamValues extends ClaudeModelMappingValues {
  codexWireApi: CodexWireApi;
  codexAuthMode: CodexAuthMode;
}

interface AgentParamDraft {
  providerId: string;
  values: AgentParamValues;
}

/**
 * Per-agent deep configuration dialog: binding, agent-conditional model
 * params (persisted on the provider record via useProviderMetaPatch with the
 * standard autosave debounce), launch command, disk config and deactivation.
 */
export function AgentSettingsDialog({
  toolId,
  open,
  onClose,
  onAddProvider,
  onOpenProviderDrawer,
}: AgentSettingsDialogProps) {
  const { t } = useTranslation();
  const act = useAgentActivation(toolId);
  const metaPatch = useProviderMetaPatch();
  const provider = act.boundProvider;
  const [configDirty, setConfigDirty] = useState(false);

  // Bind the draft to a provider id so switching providers can never write the
  // previous provider's Claude/Codex parameters into the new one.
  const [draft, setDraft] = useState<AgentParamDraft | null>(null);
  const persisted: AgentParamValues | null = useMemo(() => {
    if (!provider) return null;
    return {
      claudeMainModel: getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.main),
      claudeHaikuModel: getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.haiku),
      claudeSonnetModel: getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.sonnet),
      claudeOpusModel: getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.opus),
      codexWireApi: providerCodexWireApi(provider),
      codexAuthMode: providerCodexAuthMode(provider),
    };
  }, [provider]);

  const params = draft && draft.providerId === provider?.id ? draft.values : null;
  const values = params ?? persisted;
  const dirty = useMemo(() => {
    if (!params || !persisted) return false;
    return (Object.keys(params) as (keyof AgentParamValues)[]).some((k) => params[k] !== persisted[k]);
  }, [params, persisted]);

  const saveParams = useCallback(async (): Promise<SaveAttemptResult> => {
    if (!provider || !params) return "saved";
    try {
      await metaPatch(
        provider.id,
        {
          [CLAUDE_MODEL_META_KEYS.main]: params.claudeMainModel.trim(),
          [CLAUDE_MODEL_META_KEYS.haiku]: params.claudeHaikuModel.trim(),
          [CLAUDE_MODEL_META_KEYS.sonnet]: params.claudeSonnetModel.trim(),
          [CLAUDE_MODEL_META_KEYS.opus]: params.claudeOpusModel.trim(),
          [CODEX_WIRE_API_META_KEY]: params.codexWireApi,
          [CODEX_AUTH_MODE_META_KEY]: params.codexAuthMode,
        },
        { codex_wire_api: params.codexWireApi, codex_auth_mode: params.codexAuthMode },
      );
      // Re-write the on-disk config so codex picks the new params up immediately.
      if (toolId === "codex" && act.activeEntry) {
        await act.updateSettings({ wire_api: params.codexWireApi, auth_mode: params.codexAuthMode });
      }
      return "saved";
    } catch {
      return "error";
    }
  }, [provider, params, metaPatch, toolId, act]);

  const { state: saveState, flush } = useAutosave({ dirty, save: saveParams, changeToken: params });

  const setParam = useCallback(
    <K extends keyof AgentParamValues>(key: K, value: AgentParamValues[K]) => {
      if (!persisted || !provider) return;
      setDraft((previous) => ({
        providerId: provider.id,
        values: {
          ...(previous?.providerId === provider.id ? previous.values : persisted),
          [key]: value,
        },
      }));
    },
    [persisted, provider],
  );

  const status = computeAgentStatus({
    agent: act.agent,
    activation: act.activeEntry,
    boundProvider: provider,
    installed: act.install.installed,
    installLoading: act.install.loading,
    isSyncing: act.busy,
  });

  const availableModels = useMemo(() => {
    if (!provider) return [];
    return buildModelCatalog([provider.default_model, ...(provider.models ?? [])]);
  }, [provider]);

  const modelCatalog = useMemo(() => getModelCatalogFromMeta(provider?.meta), [provider]);
  const lastSync = act.activeEntry?.last_sync_at
    ? formatSyncTime(new Date(act.activeEntry.last_sync_at * 1000).toISOString(), t)
    : null;

  const flushThen = useCallback(
    async (action: () => void | Promise<void>) => {
      if (configDirty) {
        toast.warning(t("models.configFiles.saveBeforeClose"));
        return false;
      }
      const result = await flush();
      if (result === "validation" || result === "error") return false;
      await action();
      return true;
    },
    [configDirty, flush, t],
  );

  const requestClose = useCallback(async () => {
    await flushThen(onClose);
  }, [flushThen, onClose]);

  const handleProviderPick = useCallback(
    async (providerId: string) => {
      await flushThen(async () => {
        setDraft(null);
        await act.activate(providerId);
      });
    },
    [act, flushThen],
  );

  const claudeMappingOptions = useMemo(
    () =>
      buildModelCatalog([
        LATEST_CLAUDE_MODELS.main,
        LATEST_CLAUDE_MODELS.haiku,
        LATEST_CLAUDE_MODELS.sonnet,
        LATEST_CLAUDE_MODELS.opus,
        ...availableModels,
      ]),
    [availableModels],
  );

  return (
    <ModalShell
      open={open}
      onClose={() => void requestClose()}
      ariaLabel={t("models.dialog.title", { name: act.agent.displayName })}
      panelClassName="max-w-[640px]"
      surfaceClassName="flex max-h-[85vh] flex-col"
      dismissable={!configDirty}
    >
      <ModalHeader
        icon={<AgentToolIcon toolId={act.agent.iconId} size="sm" />}
        title={
          <span className="flex items-center gap-2">
            {t("models.dialog.title", { name: act.agent.displayName })}
            <AgentStatusPill status={status} />
            {configDirty || dirty || saveState !== "idle" ? (
              <SaveBadge state={configDirty ? "dirty" : saveState} />
            ) : null}
          </span>
        }
        onClose={() => void requestClose()}
      />

      <div className="ss-page-scroll min-h-0 flex-1 space-y-3.5 overflow-y-auto px-6 py-4">
        {provider ? <ConflictWarnings providerId={provider.id} toolId={toolId} /> : null}

        {!act.install.installed && !act.install.loading ? (
          <div className="rounded-xl border border-amber-500/20 bg-amber-500/[0.06] px-3 py-2.5 text-[11px] text-amber-400">
            <p>{t("models.card.notInstalled")}</p>
            <ExternalAnchor
              href={act.agent.installDocsUrl}
              className="mt-1 inline-flex items-center gap-1 font-medium text-primary hover:underline"
            >
              {t("models.card.installDocs")} <ExternalLink className="h-3 w-3" />
            </ExternalAnchor>
          </div>
        ) : null}

        {/* ── Binding ──────────────────────────────────────────── */}
        <ModelFormSection
          title={t("models.dialog.connectSection")}
          description={t("models.dialog.connectSectionHint")}
          icon={<Cable className="h-4 w-4" />}
        >
          <ModelFormField id="agent-provider" label={t("models.card.providerLabel")}>
            <ProviderSelectPopover
              id="agent-provider"
              ariaLabel={t("models.card.providerLabel")}
              density="standard"
              providers={act.compatibleProviders}
              currentId={act.activeEntry?.provider_id}
              onPick={(id) => void handleProviderPick(id)}
              onAddProvider={() => void flushThen(onAddProvider)}
              busy={act.busy}
              disabled={!act.install.installed || configDirty}
            />
          </ModelFormField>
          {provider ? (
            <ModelFormField id="agent-model" label={t("models.card.modelLabel")}>
              <ModelSelectPopover
                id="agent-model"
                ariaLabel={t("models.card.modelLabel")}
                density="standard"
                models={availableModels}
                catalog={modelCatalog}
                current={act.currentModel}
                onPick={(m) => void act.pickModel(m)}
                disabled={act.busy || configDirty}
                footerAction={{
                  label: t("models.picker.manageModels"),
                  onClick: () => void flushThen(() => onOpenProviderDrawer(provider.id, "models")),
                }}
              />
            </ModelFormField>
          ) : null}
        </ModelFormSection>

        {/* ── Model params (rendered per agent) ───────────────── */}
        {provider && values && toolId === "claude-code" ? (
          <ModelFormSection
            title={t("models.dialog.modelParams")}
            description={t("models.dialog.modelParamsHint")}
            icon={<SlidersHorizontal className="h-4 w-4" />}
          >
            <ClaudeModelMapping
              values={values}
              options={claudeMappingOptions}
              onChange={(key, value) => setParam(key, value)}
            />
          </ModelFormSection>
        ) : null}
        {provider && values && toolId === "codex" ? (
          <ModelFormSection
            title={t("models.dialog.modelParams")}
            description={t("models.dialog.modelParamsHint")}
            icon={<SlidersHorizontal className="h-4 w-4" />}
          >
            <CodexSettingsForm
              wireApi={values.codexWireApi}
              authMode={values.codexAuthMode}
              onChangeWireApi={(v) => setParam("codexWireApi", v)}
              onChangeAuthMode={(v) => setParam("codexAuthMode", v)}
              provider={provider}
            />
          </ModelFormSection>
        ) : null}

        {/* ── Launch command (Claude only) ─────────────────────── */}
        {toolId === "claude-code" && act.currentModel ? (
          <ModelFormSection title={t("models.dialog.launchCommand")} icon={<Terminal className="h-4 w-4" />}>
            <AgentLaunchCommand model={act.currentModel} />
          </ModelFormSection>
        ) : null}

        {/* ── Disk config ──────────────────────────────────────── */}
        <ModelFormSection
          title={t("models.dialog.diskConfig")}
          description={act.agent.configPathDisplay}
          icon={<FileCode2 className="h-4 w-4" />}
        >
          <AgentConfigFiles
            toolId={toolId}
            activeProviderId={act.activeEntry?.provider_id ?? null}
            onDirtyChange={setConfigDirty}
          />
          <div className="flex items-center justify-between text-[11px] text-muted-foreground">
            <span>{t("models.dialog.lastSync", { time: lastSync ?? t("models.dialog.neverSynced") })}</span>
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-7 gap-1.5 text-[11px]"
              onClick={() => void flushThen(() => act.resync())}
              disabled={!act.activeEntry || act.busy || configDirty}
            >
              <RefreshCw className={act.busy ? "h-3 w-3 animate-spin" : "h-3 w-3"} />
              {t("models.dialog.rewrite")}
            </Button>
          </div>
        </ModelFormSection>
      </div>

      <footer className="flex shrink-0 items-center justify-between border-t border-border/50 px-6 py-3">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="gap-1.5 text-destructive hover:bg-destructive/10 hover:text-destructive"
          onClick={() => {
            void flushThen(async () => {
              setDraft(null);
              await act.deactivate();
            });
          }}
          disabled={!act.activeEntry || act.busy || configDirty}
        >
          <Unplug className="h-3.5 w-3.5" />
          {t("models.card.disconnect")}
        </Button>
        <Button
          variant="outline"
          size="sm"
          onClick={() => void requestClose()}
          disabled={configDirty}
          title={configDirty ? t("models.configFiles.saveBeforeClose") : undefined}
        >
          {t("models.save.done")}
        </Button>
      </footer>
    </ModalShell>
  );
}
