import { Gauge } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { AiTranslateMetrics } from "../../types";

type TranslationMetricsPillProps = {
  metrics: AiTranslateMetrics | null;
};

function formatTps(tps: number | null | undefined): string | null {
  if (typeof tps !== "number" || !Number.isFinite(tps) || tps <= 0) return null;
  return tps >= 100 ? tps.toFixed(0) : tps.toFixed(1);
}

export function TranslationMetricsPill({ metrics }: TranslationMetricsPillProps) {
  const { t } = useTranslation();
  if (!metrics) return null;

  const tps = formatTps(metrics.tps);
  const status = metrics.cacheHit
    ? t("skillEditor.translationCacheHit")
    : tps
      ? t("skillEditor.translationTps", { tps })
      : t("skillEditor.translationUsageUnavailable");

  return (
    <span
      className="hidden md:inline-flex h-7 max-w-[260px] items-center gap-1.5 rounded-md border border-border/70 bg-muted/45 px-2 text-[11px] font-medium text-muted-foreground"
      title={t("skillEditor.translationMetricsTitle", {
        model: metrics.model,
        status,
        tokens: metrics.completionTokens ?? "-",
        elapsed: (metrics.elapsedMs / 1000).toFixed(1),
      })}
    >
      <Gauge className="h-3.5 w-3.5 shrink-0 text-primary/80" aria-hidden />
      <span className="truncate">{metrics.model}</span>
      <span className="shrink-0 text-muted-foreground/60">|</span>
      <span className="shrink-0 tabular-nums">{status}</span>
    </span>
  );
}
