import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { StackedTokenBar } from "@/components/shared/StackedTokenBar";
import { cn } from "@/lib/utils";
import { formatQuotaNumber } from "../lib/usageLabels";
import type { SessionChip, TodayConsumption } from "../types";
import { formatEstimateUsd } from "./card";

/** The chip strip's bar scale: the busiest session renders at full height. */
function maxTotal(chips: SessionChip[]): number {
  return chips.reduce(
    (max, chip) =>
      Math.max(max, chip.tokens.input + chip.tokens.cache_read + chip.tokens.cache_write + chip.tokens.output),
    0,
  );
}

/** `HH:MM` of the chip's newest call, local clock, quiet and monospaced. */
function shortTime(ms: number): string {
  return new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/**
 * Today's session chips (spec slice 13): one chip per session with an id,
 * newest activity first — the Usage page's「card → session → agent」entry.
 * Clicking a chip opens that agent in the Models hub.
 *
 * Operate discipline: a quiet strip under the spend row, never a second KPI
 * tile; monospaced tabular digits; the cost always carries the「estimate」
 * marker; the fixed「no records yet」sentence when nothing landed; the bars
 * are static CSS so `prefers-reduced-motion` needs no branch. Every number
 * is a derived view over the merged consumption view.
 */
export function TodaySessions({
  today,
  onFocusAgent,
  className,
}: {
  /** `null` = the read landed but nothing ran today; omitted hides the strip. */
  today?: TodayConsumption | null;
  /** Cross-view navigation: open this agent's model routes in Models. */
  onFocusAgent: (agentId: string) => void;
  className?: string;
}) {
  const { t } = useTranslation();
  const chips = today?.chips ?? [];
  const max = useMemo(() => maxTotal(chips), [chips]);

  if (today === undefined) return null;

  return (
    <section
      aria-label={t("usage.todaySessionsTitle")}
      className={cn("shrink-0 border-b border-border/60 px-3 py-2", className)}
      data-testid="today-sessions"
    >
      <div className="flex items-baseline gap-2 pb-1.5">
        <h2 className="text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">
          {t("usage.todaySessionsTitle")}
        </h2>
        <span className="text-[11px] tabular-nums text-muted-foreground/80">{chips.length}</span>
      </div>
      {chips.length === 0 ? (
        <p className="pb-1 text-[11px] text-muted-foreground">{t("usage.todaySessionsEmpty")}</p>
      ) : (
        <ul className="flex min-w-0 gap-1.5 overflow-x-auto pb-1 [scrollbar-gutter:stable]">
          {chips.map((chip) => (
            <li key={`${chip.agent}/${chip.session}`}>
              <button
                type="button"
                onClick={() => onFocusAgent(chip.agent)}
                title={t("usage.todaySessionsAgentEntry", { agent: chip.agent })}
                className={cn(
                  "flex cursor-pointer items-center gap-2 rounded-lg border border-border/60 bg-card/70 px-2 py-1.5 text-left transition hover:bg-muted/40",
                  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/50",
                )}
              >
                <StackedTokenBar
                  input={chip.tokens.input + chip.tokens.cache_read + chip.tokens.cache_write}
                  output={chip.tokens.output}
                  max={max}
                  label={t("usage.todaySessionsTokens", {
                    input: formatQuotaNumber(chip.tokens.input + chip.tokens.cache_read + chip.tokens.cache_write),
                    output: formatQuotaNumber(chip.tokens.output),
                  })}
                />
                <span className="flex min-w-0 flex-col gap-0.5">
                  <span className="flex items-baseline gap-1.5">
                    <span className="truncate font-mono text-[11px] font-medium text-foreground">{chip.agent}</span>
                    <span className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground">
                      {shortTime(chip.last_active)}
                    </span>
                  </span>
                  <span className="flex items-baseline gap-1.5">
                    <span
                      className="max-w-40 truncate font-mono text-[10px] text-muted-foreground"
                      title={chip.title ?? undefined}
                    >
                      {chip.title ?? chip.session}
                    </span>
                    {chip.cost_usd != null ? (
                      <span className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground">
                        {t("usage.todaySessionsCost", { cost: formatEstimateUsd(chip.cost_usd) })}
                      </span>
                    ) : null}
                    {chip.via_gateway ? (
                      <span
                        role="img"
                        aria-label={t("models.routes.viaGateway")}
                        title={t("models.routes.viaGateway")}
                        className="h-1.5 w-1.5 shrink-0 rounded-full bg-primary/70"
                      />
                    ) : null}
                  </span>
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
