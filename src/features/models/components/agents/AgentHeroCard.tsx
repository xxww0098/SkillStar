import { motion } from "framer-motion";
import { ArrowRight, ExternalLink, Plug, RefreshCw, Settings2, Unplug } from "lucide-react";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../../components/ui/button";
import { ExternalAnchor } from "../../../../components/ui/ExternalAnchor";
import { cn } from "../../../../lib/utils";
import { useAgentActivation } from "../../hooks/useAgentActivation";
import type { AgentHealth } from "../../hooks/useAgentHealth";
import type { AgentDescriptor } from "../../lib/agentRegistry";
import { type AgentStatus, computeAgentStatus } from "../../lib/agentStatus";
import { buildModelCatalog, getModelCatalogFromMeta } from "../../lib/providerPatch";
import { AgentToolIcon } from "../shared/AgentToolIcon";
import { ModelSelectPopover } from "../shared/ModelSelectPopover";
import { ProviderSelectPopover } from "../shared/ProviderSelectPopover";
import { AgentStatusPill, statusTone } from "./AgentStatusPill";

export interface AgentHeroCardProps {
  agent: AgentDescriptor;
  health: AgentHealth;
  onAddProvider: () => void;
  onOpenSettings: () => void;
  onOpenProviderDrawer: (providerId: string) => void;
}

const TONE_CARD: Record<ReturnType<typeof statusTone>, { border: string; glow: string; strip: string }> = {
  ok: {
    border: "border-emerald-500/25",
    glow: "shadow-[0_30px_60px_-32px_rgba(16,185,129,0.35)]",
    strip: "bg-gradient-to-r from-emerald-400/30 via-emerald-400/70 to-emerald-400/30",
  },
  warn: {
    border: "border-amber-500/30",
    glow: "shadow-[0_24px_60px_-32px_rgba(245,158,11,0.30)]",
    strip: "bg-gradient-to-r from-amber-400/30 via-amber-400/70 to-amber-400/30",
  },
  bad: {
    border: "border-red-500/30",
    glow: "shadow-[0_24px_60px_-32px_rgba(239,68,68,0.30)]",
    strip: "bg-gradient-to-r from-red-400/30 via-red-400/70 to-red-400/30",
  },
  off: {
    border: "border-border/55",
    glow: "shadow-[0_24px_60px_-40px_var(--color-shadow)]",
    strip: "bg-gradient-to-r from-primary/10 via-primary/35 to-primary/10",
  },
  busy: {
    border: "border-primary/30",
    glow: "shadow-[0_24px_60px_-36px_rgba(59,130,246,0.35)]",
    strip: "bg-gradient-to-r from-primary/20 via-primary/60 to-primary/20",
  },
};

/** Status-aware banner (priority: not_installed > misconfigured > probe error). */
function StatusBanner({
  status,
  agent,
  boundProviderId,
  onOpenProviderDrawer,
  onRetest,
}: {
  status: AgentStatus;
  agent: AgentDescriptor;
  boundProviderId: string | null;
  onOpenProviderDrawer: (providerId: string) => void;
  onRetest: () => void;
}) {
  const { t } = useTranslation();
  if (status.kind === "not_installed") {
    return (
      <div className="rounded-xl border border-amber-500/20 bg-amber-500/[0.06] px-3 py-2.5 text-[11px] text-amber-400">
        <p>{t("models.card.notInstalled")}</p>
        <ExternalAnchor
          href={agent.installDocsUrl}
          className="mt-1 inline-flex items-center gap-1 font-medium text-primary hover:underline"
        >
          {t("models.card.installDocs")} <ExternalLink className="h-3 w-3" />
        </ExternalAnchor>
      </div>
    );
  }
  if (status.kind === "misconfigured" && boundProviderId) {
    return (
      <div className="rounded-xl border border-amber-500/20 bg-amber-500/[0.06] px-3 py-2.5 text-[11px] text-amber-400">
        {t("models.card.missingEndpoint", {
          endpoint: status.requiredUrlField === "anthropic" ? "Anthropic" : "OpenAI",
        })}
        <button
          type="button"
          onClick={() => onOpenProviderDrawer(boundProviderId)}
          className="mt-1 inline-flex items-center gap-1 font-medium text-primary hover:underline"
        >
          {t("models.card.goConfigure")} <ArrowRight className="h-3 w-3" />
        </button>
      </div>
    );
  }
  if (status.kind === "auth_failed" || status.kind === "timeout" || status.kind === "error") {
    return (
      <div className="rounded-xl border border-red-500/20 bg-red-500/[0.06] px-3 py-2.5 text-[11px] text-red-400">
        {status.kind === "auth_failed"
          ? t("models.card.authFailed")
          : status.kind === "timeout"
            ? t("models.card.timeout")
            : t("models.card.connectFailed")}
        <button
          type="button"
          onClick={onRetest}
          className="ml-2 inline-flex items-center gap-1 font-medium text-primary hover:underline"
        >
          {t("models.card.retry")} <RefreshCw className="h-3 w-3" />
        </button>
      </div>
    );
  }
  return null;
}

export function AgentHeroCard({
  agent,
  health,
  onAddProvider,
  onOpenSettings,
  onOpenProviderDrawer,
}: AgentHeroCardProps) {
  const { t } = useTranslation();
  const act = useAgentActivation(agent.toolId);
  const probe = health.results[agent.toolId] ?? null;
  const probing = health.testing[agent.toolId] ?? false;

  const status = computeAgentStatus({
    agent,
    activation: act.activeEntry,
    boundProvider: act.boundProvider,
    installed: act.install.installed,
    installLoading: act.install.loading,
    isSyncing: act.busy,
    probe,
    probing,
  });
  const tone = statusTone(status);
  const card = TONE_CARD[tone];
  const connected = status.kind !== "not_installed" && status.kind !== "inactive" && Boolean(act.activeEntry);

  const availableModels = useMemo(() => {
    if (!act.boundProvider) return [];
    return buildModelCatalog([act.boundProvider.default_model, ...(act.boundProvider.models ?? [])]);
  }, [act.boundProvider]);

  const modelCatalog = useMemo(() => getModelCatalogFromMeta(act.boundProvider?.meta), [act.boundProvider]);

  return (
    <motion.section
      initial={{ opacity: 0, y: 14 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.3, ease: [0.22, 1, 0.36, 1] }}
      className={cn(
        "relative flex h-full flex-col rounded-3xl border bg-card/75 backdrop-blur-2xl",
        "transition-transform duration-300 hover:-translate-y-0.5",
        card.border,
        card.glow,
      )}
    >
      <span aria-hidden className={cn("absolute inset-x-0 top-0 h-[2px]", card.strip)} />

      <header className="flex items-start gap-3 px-5 pt-5">
        <AgentToolIcon toolId={agent.iconId} size="md" />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h3 className="truncate text-base font-bold text-foreground">{agent.displayName}</h3>
            <AgentStatusPill
              status={status}
              testing={probing}
              onRetest={connected ? () => health.retest(agent.toolId) : undefined}
            />
          </div>
          <p className="mt-0.5 text-[11px] text-muted-foreground">{agent.tagline}</p>
        </div>
      </header>

      <div className="flex-1 space-y-3 px-5 pt-4 pb-3">
        <StatusBanner
          status={status}
          agent={agent}
          boundProviderId={act.boundProvider?.id ?? null}
          onOpenProviderDrawer={onOpenProviderDrawer}
          onRetest={() => health.retest(agent.toolId)}
        />

        <div className="space-y-1.5">
          <label className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            {t("models.card.providerLabel")}
          </label>
          <ProviderSelectPopover
            providers={act.compatibleProviders}
            currentId={act.activeEntry?.provider_id}
            onPick={(id) => void act.activate(id)}
            onAddProvider={onAddProvider}
            busy={act.busy}
            disabled={status.kind === "not_installed"}
            triggerClassName={cn(
              tone === "ok" && "border-emerald-500/25 bg-emerald-500/[0.04]",
              tone === "warn" && status.kind === "misconfigured" && "border-amber-500/25 bg-amber-500/[0.04]",
            )}
          />
        </div>

        {connected && availableModels.length > 0 ? (
          <div className="space-y-1.5">
            <label className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
              {t("models.card.modelLabel")}
            </label>
            <ModelSelectPopover
              models={availableModels}
              catalog={modelCatalog}
              current={act.currentModel}
              onPick={(m) => void act.pickModel(m)}
              disabled={act.busy}
              footerAction={
                act.boundProvider
                  ? {
                      label: t("models.picker.manageModels"),
                      onClick: () => onOpenProviderDrawer(act.boundProvider?.id ?? ""),
                    }
                  : undefined
              }
            />
          </div>
        ) : null}

        {connected && availableModels.length === 0 && act.boundProvider ? (
          <p className="text-[11px] text-amber-500">
            {t("models.card.noModelsFetched")}
            <button
              type="button"
              onClick={() => onOpenProviderDrawer(act.boundProvider?.id ?? "")}
              className="ml-1 font-medium text-primary hover:underline"
            >
              {t("models.card.goFetch")}
            </button>
          </p>
        ) : null}
      </div>

      <footer className="flex items-center gap-1 border-t border-border/40 bg-background/20 px-4 py-2.5">
        {connected ? (
          <>
            <Button
              variant="ghost"
              size="icon-sm"
              onClick={() => void act.resync()}
              disabled={act.busy}
              title={t("models.card.resync")}
              className="text-muted-foreground hover:text-foreground"
            >
              <RefreshCw className={cn("h-3.5 w-3.5", act.busy && "animate-spin")} />
            </Button>
            <Button
              variant="ghost"
              size="icon-sm"
              onClick={onOpenSettings}
              title={t("models.card.settings")}
              className="text-muted-foreground hover:text-foreground"
            >
              <Settings2 className="h-3.5 w-3.5" />
            </Button>
            <Button
              variant="ghost"
              size="icon-sm"
              onClick={() => void act.deactivate()}
              disabled={act.busy}
              title={t("models.card.disconnect")}
              className="ml-auto text-muted-foreground hover:text-destructive"
            >
              <Unplug className="h-3.5 w-3.5" />
            </Button>
          </>
        ) : (
          <>
            <Button
              variant="ghost"
              size="icon-sm"
              onClick={onOpenSettings}
              title={t("models.card.settings")}
              className="text-muted-foreground hover:text-foreground"
            >
              <Settings2 className="h-3.5 w-3.5" />
            </Button>
            <Button variant="outline" size="sm" onClick={onAddProvider} className="ml-auto h-7 text-[11px]">
              <Plug className="mr-1.5 h-3 w-3" />
              {t("models.card.addAndConnect")}
            </Button>
          </>
        )}
      </footer>
    </motion.section>
  );
}
