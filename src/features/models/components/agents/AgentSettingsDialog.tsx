import { ExternalLink, RefreshCw, Terminal, Unplug } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
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
import type { SaveAttemptResult } from "../../types";
import { ConflictWarnings } from "../diagnostics/ConflictWarnings";
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
  onOpenProviderDrawer: (providerId: string) => void;
}

interface AgentParamValues extends ClaudeModelMappingValues {
  codexWireApi: CodexWireApi;
  codexAuthMode: CodexAuthMode;
}

function SectionTitle({ children }: { children: React.ReactNode }) {
  return <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{children}</h3>;
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

  // Agent-conditional params, seeded from the bound provider.
  const [params, setParams] = useState<AgentParamValues | null>(null);
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

  const { state: saveState, flush } = useAutosave({ dirty, save: saveParams });

  const setParam = useCallback(
    <K extends keyof AgentParamValues>(key: K, value: AgentParamValues[K]) => {
      if (!persisted) return;
      setParams((prev) => ({ ...(prev ?? persisted), [key]: value }));
    },
    [persisted],
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
    ? formatSyncTime(new Date(act.activeEntry.last_sync_at * 1000).toISOString())
    : null;

  const requestClose = useCallback(() => {
    void flush();
    onClose();
  }, [flush, onClose]);

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
      onClose={requestClose}
      ariaLabel={t("models.dialog.title", { name: act.agent.displayName })}
      panelClassName="max-w-[640px]"
      surfaceClassName="flex max-h-[85vh] flex-col"
    >
      <ModalHeader
        icon={<AgentToolIcon toolId={act.agent.iconId} size="sm" />}
        title={
          <span className="flex items-center gap-2">
            {t("models.dialog.title", { name: act.agent.displayName })}
            <AgentStatusPill status={status} />
            {dirty || saveState !== "idle" ? <SaveBadge state={saveState} /> : null}
          </span>
        }
        onClose={requestClose}
      />

      <div className="ss-page-scroll min-h-0 flex-1 space-y-5 overflow-y-auto px-6 py-4">
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
        <section className="space-y-2.5">
          <SectionTitle>{t("models.dialog.connectSection")}</SectionTitle>
          <div className="space-y-1.5">
            <span className="text-[11px] font-medium text-muted-foreground">{t("models.card.providerLabel")}</span>
            <ProviderSelectPopover
              providers={act.compatibleProviders}
              currentId={act.activeEntry?.provider_id}
              onPick={(id) => void act.activate(id)}
              onAddProvider={onAddProvider}
              busy={act.busy}
              disabled={!act.install.installed}
            />
          </div>
          {provider ? (
            <div className="space-y-1.5">
              <span className="text-[11px] font-medium text-muted-foreground">{t("models.card.modelLabel")}</span>
              <ModelSelectPopover
                models={availableModels}
                catalog={modelCatalog}
                current={act.currentModel}
                onPick={(m) => void act.pickModel(m)}
                disabled={act.busy}
                footerAction={{
                  label: t("models.picker.manageModels"),
                  onClick: () => onOpenProviderDrawer(provider.id),
                }}
              />
            </div>
          ) : null}
        </section>

        {/* ── Model params (rendered per agent) ───────────────── */}
        {provider && values && toolId === "claude-code" ? (
          <section className="space-y-2.5 border-t border-border/40 pt-4">
            <SectionTitle>{t("models.dialog.modelParams")}</SectionTitle>
            <ClaudeModelMapping
              values={values}
              options={claudeMappingOptions}
              onChange={(key, value) => setParam(key, value)}
            />
          </section>
        ) : null}
        {provider && values && toolId === "codex" ? (
          <section className="space-y-2.5 border-t border-border/40 pt-4">
            <SectionTitle>{t("models.dialog.modelParams")}</SectionTitle>
            <CodexSettingsForm
              wireApi={values.codexWireApi}
              authMode={values.codexAuthMode}
              onChangeWireApi={(v) => setParam("codexWireApi", v)}
              onChangeAuthMode={(v) => setParam("codexAuthMode", v)}
              provider={provider}
            />
          </section>
        ) : null}

        {/* ── Launch command (Claude only) ─────────────────────── */}
        {toolId === "claude-code" && act.currentModel ? (
          <section className="space-y-2.5 border-t border-border/40 pt-4">
            <SectionTitle>
              <span className="inline-flex items-center gap-1.5">
                <Terminal className="h-3 w-3" />
                {t("models.dialog.launchCommand")}
              </span>
            </SectionTitle>
            <AgentLaunchCommand model={act.currentModel} />
          </section>
        ) : null}

        {/* ── Disk config ──────────────────────────────────────── */}
        <section className="space-y-2.5 border-t border-border/40 pt-4">
          <SectionTitle>{t("models.dialog.diskConfig")}</SectionTitle>
          <p className="rounded-lg border border-border/40 bg-background/35 px-2.5 py-2 font-mono text-[11px] text-muted-foreground">
            {act.agent.configPathDisplay}
          </p>
          <AgentConfigFiles toolId={toolId} activeProviderId={act.activeEntry?.provider_id ?? null} />
          <div className="flex items-center justify-between text-[11px] text-muted-foreground">
            <span>{t("models.dialog.lastSync", { time: lastSync ?? t("models.dialog.neverSynced") })}</span>
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-7 gap-1.5 text-[11px]"
              onClick={() => void act.resync()}
              disabled={!act.activeEntry || act.busy}
            >
              <RefreshCw className={act.busy ? "h-3 w-3 animate-spin" : "h-3 w-3"} />
              {t("models.dialog.rewrite")}
            </Button>
          </div>
        </section>
      </div>

      <footer className="flex shrink-0 items-center justify-between border-t border-border/50 px-6 py-3">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="gap-1.5 text-destructive hover:bg-destructive/10 hover:text-destructive"
          onClick={() => {
            void act.deactivate();
          }}
          disabled={!act.activeEntry || act.busy}
        >
          <Unplug className="h-3.5 w-3.5" />
          {t("models.card.disconnect")}
        </Button>
        <Button variant="outline" size="sm" onClick={requestClose}>
          {t("models.save.done")}
        </Button>
      </footer>
    </ModalShell>
  );
}
