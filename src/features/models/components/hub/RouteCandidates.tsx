import { useTranslation } from "react-i18next";
import { StackedTokenBar } from "@/components/shared/StackedTokenBar";
import { cn } from "@/lib/utils";
import { formatCompactCount, formatEstimateUsd } from "@/lib/numberFormat";
import type { RouteComparison } from "@/types/generated/RouteComparison";
import type { RouteCost } from "@/types/generated/RouteCost";
import { useServingAgents } from "../../api/routes";
import { getAgent } from "../../lib/agentRegistry";
import { AgentToolIcon } from "../shared/AgentToolIcon";

/** `HH:MM` of the renewal moment, local clock; null when unknown/past. */
function renewsTime(ms?: number): string | null {
  if (ms === undefined || ms <= 0) return null;
  return new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/**
 * One candidate's chip: allowance first (the slice-12 snapshot reaching the
 * UI for the first time), then the measured route — calls, error rate, p50 /
 * p95 latency, tokens, and the always-estimated cost. Monospaced tabular
 * digits; a resting candidate says so instead of hiding.
 */
function CandidateChip({
  candidate,
  maxTokens,
  title,
}: {
  candidate: RouteCost;
  /** The comparison's busiest candidate; its bar renders at full height. */
  maxTokens: number;
  /** Hover explanation for the chip (agent or model context). */
  title: string;
}) {
  const { t } = useTranslation();
  const input = candidate.tokens.input + candidate.tokens.cache_read + candidate.tokens.cache_write;
  const output = candidate.tokens.output;
  const renews = renewsTime(candidate.renews_at_ms);
  return (
    <li
      className="rounded-xl border border-border/60 bg-muted/20 px-2.5 py-2"
      title={title}
      data-testid="route-candidate-chip"
    >
      <div className="flex items-center gap-2">
        <span className="truncate font-mono text-[11px] font-medium text-foreground">{candidate.catalog}</span>
        {candidate.resting ? (
          <span className="shrink-0 rounded bg-destructive/10 px-1 py-0.5 text-[10px] text-destructive">
            {t("models.routes.resting")}
          </span>
        ) : null}
        {candidate.percent != null ? (
          <span
            className={cn(
              "ml-auto shrink-0 font-mono text-[10px] tabular-nums",
              candidate.percent >= 98 ? "text-destructive" : "text-muted-foreground",
            )}
            title={renews ? t("models.routes.renewsAt", { time: renews }) : undefined}
          >
            {t("models.routes.percentUsed", { percent: Math.round(candidate.percent) })}
          </span>
        ) : null}
      </div>
      <div className="mt-1.5 flex items-center gap-2">
        <StackedTokenBar
          input={input}
          output={output}
          max={maxTokens}
          label={t("usage.todaySessionsTokens", {
            input: formatCompactCount(input),
            output: formatCompactCount(output),
          })}
        />
        <div className="flex min-w-0 flex-col gap-0.5">
          <span className="font-mono text-[10px] tabular-nums text-muted-foreground">
            {t("models.routes.calls", { count: candidate.calls })}
            {candidate.calls > 0
              ? ` · ${t("models.routes.errorRate", { rate: `${Math.round(candidate.error_rate * 100)}%` })}`
              : ""}
          </span>
          <span className="font-mono text-[10px] tabular-nums text-muted-foreground">
            {t("models.routes.latency", {
              p50: formatCompactCount(candidate.p50_latency_ms),
              p95: formatCompactCount(candidate.p95_latency_ms),
            })}
          </span>
          {candidate.cost_usd != null ? (
            <span className="font-mono text-[10px] tabular-nums text-muted-foreground">
              {t("usage.todaySessionsCost", { cost: formatEstimateUsd(candidate.cost_usd) })}
            </span>
          ) : null}
        </div>
      </div>
    </li>
  );
}

/** The comparison's busiest candidate, for the chip bars' shared scale. */
function maxTokensOf(comparison: RouteComparison): number {
  return comparison.candidates.reduce(
    (max, candidate) =>
      Math.max(
        max,
        candidate.tokens.input + candidate.tokens.cache_read + candidate.tokens.cache_write + candidate.tokens.output,
      ),
    0,
  );
}

/**
 * The Gateway panel's candidate chips (spec slice 13): the focused agent's
 * model, its routes in `route_smart` order, allowance on the UI for the
 * first time. Clicking a chip opens the agent's model selector — the
 * triangle's last leg (usage card → gateway candidates → selector).
 */
export function RouteCandidates({
  agentName,
  modelRef,
  comparison,
  loading,
  onOpenPicker,
}: {
  agentName: string;
  modelRef: string;
  comparison: RouteComparison | undefined;
  loading: boolean;
  /** Open the agent's model selector (the triangle's selector leg). */
  onOpenPicker: () => void;
}) {
  const { t } = useTranslation();
  const max = comparison ? maxTokensOf(comparison) : 0;
  return (
    <div className="space-y-1.5" data-testid="route-candidates">
      <div className="flex items-baseline gap-2">
        <div className="text-[11px] font-medium tracking-wider text-muted-foreground uppercase">
          {t("models.routes.title")}
        </div>
        <span className="truncate font-mono text-[10px] text-muted-foreground/80" title={modelRef}>
          {modelRef}
        </span>
      </div>
      {loading && !comparison ? (
        <p className="text-xs text-muted-foreground">{t("common.loading", "…")}</p>
      ) : !comparison || comparison.candidates.length === 0 ? (
        <p className="text-xs text-muted-foreground">{t("models.routes.noCandidates")}</p>
      ) : (
        <>
          <ul className="space-y-1.5">
            {comparison.candidates.map((candidate) => (
              <CandidateChip
                key={candidate.catalog}
                candidate={candidate}
                maxTokens={max}
                title={`${agentName} · ${candidate.catalog}`}
              />
            ))}
          </ul>
          <button
            type="button"
            onClick={onOpenPicker}
            className="w-full rounded-lg border border-border/60 bg-background px-2 py-1 text-[11px] text-muted-foreground transition hover:bg-muted/40 hover:text-foreground"
          >
            {t("models.routes.openPicker")}
          </button>
        </>
      )}
    </div>
  );
}

/**
 * The catalog leg of the triangle: which agents' current models route to
 * one catalog (the Usage quota card's jump target), each row opening that
 * agent's selector. Empty until the join lands; the fixed「no records yet」
 * sentence when no agent's model resolves here.
 */
export function ServingAgents({
  catalogId,
  agents,
  onOpenPicker,
}: {
  catalogId: string;
  agents: { id: string; name: string; model_label?: string | null }[];
  onOpenPicker: (agentId: string) => void;
}) {
  const { t } = useTranslation();
  const query = useServingAgents(catalogId, agents);
  const rows = query.data ?? [];
  return (
    <div className="space-y-1.5" data-testid="serving-agents">
      <div className="text-[11px] font-medium tracking-wider text-muted-foreground uppercase">
        {t("models.routes.servingTitle", { catalog: catalogId })}
      </div>
      {query.isLoading ? (
        <p className="text-xs text-muted-foreground">{t("common.loading", "…")}</p>
      ) : rows.length === 0 ? (
        <p className="text-xs text-muted-foreground">{t("models.routes.servingEmpty")}</p>
      ) : (
        <ul className="space-y-1.5">
          {rows.map((row) => {
            const candidate = row.comparison.candidates[row.candidateIndex];
            const iconId = getAgent(row.agentId)?.iconId;
            return (
              <li key={row.agentId}>
                <button
                  type="button"
                  onClick={() => onOpenPicker(row.agentId)}
                  className="flex w-full cursor-pointer flex-col gap-1 rounded-xl border border-border/60 bg-muted/20 px-2 py-1.5 text-left transition hover:bg-muted/40"
                  title={t("models.routes.openPicker")}
                >
                  <span className="flex min-w-0 items-center gap-1.5">
                    {iconId ? (
                      <AgentToolIcon toolId={iconId} size="sm" />
                    ) : (
                      <span aria-hidden className="h-6 w-6 shrink-0 rounded-md border border-border/50 bg-muted/40" />
                    )}
                    <span className="truncate text-xs font-medium text-foreground">{row.agentName}</span>
                    <span className="ml-auto truncate font-mono text-[10px] text-muted-foreground" title={row.modelRef}>
                      {row.modelRef}
                    </span>
                  </span>
                  {candidate ? (
                    <span className="flex items-center gap-2 pl-7">
                      <span className="font-mono text-[10px] tabular-nums text-muted-foreground">
                        {t("models.routes.calls", { count: candidate.calls })}
                        {candidate.percent != null
                          ? ` · ${t("models.routes.percentUsed", { percent: Math.round(candidate.percent) })}`
                          : ""}
                      </span>
                      {candidate.resting ? (
                        <span className="rounded bg-destructive/10 px-1 py-0.5 text-[10px] text-destructive">
                          {t("models.routes.resting")}
                        </span>
                      ) : null}
                    </span>
                  ) : null}
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
