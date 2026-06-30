import { motion } from "framer-motion";
import { ArrowRight, Check, ExternalLink, Plug, RefreshCw, Settings2, Trash2, Unplug } from "lucide-react";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../../components/ui/button";
import { ExternalAnchor } from "../../../../components/ui/ExternalAnchor";
import { cn } from "../../../../lib/utils";
import { useAgentActivation } from "../../hooks/useAgentActivation";
import type { AgentHealth } from "../../hooks/useAgentHealth";
import type { AgentDescriptor } from "../../lib/agentRegistry";
import { computeAgentStatus } from "../../lib/agentStatus";
import { buildModelCatalog, getModelCatalogFromMeta } from "../../lib/providerPatch";
import { AgentToolIcon } from "../shared/AgentToolIcon";
import { ModelSelectPopover } from "../shared/ModelSelectPopover";
import { ProviderBrandIcon } from "../shared/ProviderBrandIcon";
import { ProviderSelectPopover } from "../shared/ProviderSelectPopover";
import { AgentStatusPill, statusTone } from "./AgentStatusPill";

export interface MultiProviderCardProps {
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

/**
 * Provider card for multi-provider agents (Codex, OpenCode). Unlike the
 * single-provider card, it lists EVERY bound provider as a row — each with its
 * own model picker, an "active" radio (the entry the agent's pointer selects),
 * and a remove button — plus an "add provider" control. This mirrors how the
 * agent's own config file holds several providers with one active pointer.
 */
export function MultiProviderCard({
  agent,
  health,
  onAddProvider,
  onOpenSettings,
  onOpenProviderDrawer,
}: MultiProviderCardProps) {
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
  const hasEntries = act.entries.length > 0;
  const activeProviderId = act.activeEntry?.provider_id ?? null;

  // Compatible providers not yet bound — the "add" candidates.
  const addable = useMemo(
    () => act.compatibleProviders.filter((p) => !act.entries.some((e) => e.provider.id === p.id)),
    [act.compatibleProviders, act.entries],
  );

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
              onRetest={hasEntries ? () => health.retest(agent.toolId) : undefined}
            />
            {hasEntries ? (
              <span className="rounded-full bg-primary/10 px-1.5 py-0.5 text-[10px] font-medium text-primary">
                {t("models.card.providerCount", { count: act.entries.length })}
              </span>
            ) : null}
          </div>
          <p className="mt-0.5 text-[11px] text-muted-foreground">{agent.tagline}</p>
        </div>
      </header>

      <div className="flex-1 space-y-3 px-5 pt-4 pb-3">
        {status.kind === "not_installed" ? (
          <div className="rounded-xl border border-amber-500/20 bg-amber-500/[0.06] px-3 py-2.5 text-[11px] text-amber-400">
            <p>{t("models.card.notInstalled")}</p>
            <ExternalAnchor
              href={agent.installDocsUrl}
              className="mt-1 inline-flex items-center gap-1 font-medium text-primary hover:underline"
            >
              {t("models.card.installDocs")} <ExternalLink className="h-3 w-3" />
            </ExternalAnchor>
          </div>
        ) : null}

        {hasEntries ? (
          <ul className="space-y-2">
            {act.entries.map(({ entry, provider }) => {
              const isActive = provider.id === activeProviderId;
              const models = buildModelCatalog([provider.default_model, ...(provider.models ?? [])]);
              const catalog = getModelCatalogFromMeta(provider.meta);
              return (
                <li
                  key={provider.id}
                  className={cn(
                    "rounded-xl border bg-background/30 px-3 py-2.5 transition",
                    isActive ? "border-primary/40 bg-primary/[0.05]" : "border-border/45",
                  )}
                >
                  <div className="flex items-center gap-2">
                    <button
                      type="button"
                      onClick={() => !isActive && void act.setActive(provider.id)}
                      disabled={act.busy || isActive}
                      title={isActive ? t("models.card.activeProvider") : t("models.card.setActive")}
                      className={cn(
                        "flex h-4 w-4 shrink-0 items-center justify-center rounded-full border transition",
                        isActive ? "border-primary bg-primary text-primary-foreground" : "border-muted-foreground/40",
                      )}
                    >
                      {isActive ? <Check className="h-2.5 w-2.5" /> : null}
                    </button>
                    <ProviderBrandIcon
                      presetId={provider.preset_id}
                      providerName={provider.name}
                      iconColor={provider.icon_color}
                      size="xs"
                    />
                    <span className="min-w-0 flex-1 truncate text-xs font-medium text-foreground">{provider.name}</span>
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      onClick={() => void act.removeEntry(provider.id)}
                      disabled={act.busy}
                      title={t("models.card.removeProvider")}
                      className="text-muted-foreground hover:text-destructive"
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </Button>
                  </div>
                  <div className="mt-2 pl-6">
                    <ModelSelectPopover
                      models={models}
                      catalog={catalog}
                      current={entry.model || provider.default_model || ""}
                      onPick={(m) => void act.pickModel(m, provider.id)}
                      disabled={act.busy}
                      footerAction={{
                        label: t("models.picker.manageModels"),
                        onClick: () => onOpenProviderDrawer(provider.id),
                      }}
                    />
                  </div>
                </li>
              );
            })}
          </ul>
        ) : null}

        {status.kind === "misconfigured" && act.boundProvider ? (
          <div className="rounded-xl border border-amber-500/20 bg-amber-500/[0.06] px-3 py-2.5 text-[11px] text-amber-400">
            {t("models.card.missingEndpoint", { endpoint: "OpenAI" })}
            <button
              type="button"
              onClick={() => onOpenProviderDrawer(act.boundProvider?.id ?? "")}
              className="mt-1 inline-flex items-center gap-1 font-medium text-primary hover:underline"
            >
              {t("models.card.goConfigure")} <ArrowRight className="h-3 w-3" />
            </button>
          </div>
        ) : null}

        {/* Add another provider */}
        {status.kind !== "not_installed" ? (
          <div className="space-y-1.5">
            <label className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
              {hasEntries ? t("models.card.addAnotherProvider") : t("models.card.providerLabel")}
            </label>
            <ProviderSelectPopover
              providers={addable}
              currentId={null}
              onPick={(id) => void act.addProvider(id)}
              onAddProvider={onAddProvider}
              busy={act.busy}
              disabled={act.busy}
            />
          </div>
        ) : null}
      </div>

      <footer className="flex items-center gap-1 border-t border-border/40 bg-background/20 px-4 py-2.5">
        <Button
          variant="ghost"
          size="icon-sm"
          onClick={onOpenSettings}
          title={t("models.card.settings")}
          className="text-muted-foreground hover:text-foreground"
        >
          <Settings2 className="h-3.5 w-3.5" />
        </Button>
        {hasEntries ? (
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
              onClick={() => void act.deactivate()}
              disabled={act.busy}
              title={t("models.card.disconnect")}
              className="ml-auto text-muted-foreground hover:text-destructive"
            >
              <Unplug className="h-3.5 w-3.5" />
            </Button>
          </>
        ) : (
          <Button variant="outline" size="sm" onClick={onAddProvider} className="ml-auto h-7 text-[11px]">
            <Plug className="mr-1.5 h-3 w-3" />
            {t("models.card.addAndConnect")}
          </Button>
        )}
      </footer>
    </motion.section>
  );
}
