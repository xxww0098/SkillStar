import { Activity, KeyRound, PackageX, Stethoscope, WifiOff } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { cn } from "../../../lib/utils";
import type { McpProbeStatus, McpSpecEpoch } from "../../../types";
import type { McpProbeEntry } from "../hooks/useMcpProbe";

/**
 * Health check for one installed server.
 *
 * The 2026-07-28 revision removed the `initialize` handshake, so a probe first
 * has to work out which epoch the server speaks (`server/discover` for modern,
 * `initialize` for legacy) before it can prove liveness with `tools/list`. Both
 * the epoch and the tool list are worth showing: they are the difference
 * between "it answered" and "it answered, and here is what it can do".
 *
 * **`authorization-required` is not an error.** A remote server replying `401`
 * with a `WWW-Authenticate` challenge is behaving correctly and asking for
 * OAuth; painting it red trains users to ignore the one signal that means "sign
 * in". It gets its own neutral treatment, as does `runtime-missing`, which
 * means "install Node", not "this server is broken".
 */

const STATUS_ICON: Record<McpProbeStatus, typeof Activity> = {
  healthy: Activity,
  "authorization-required": KeyRound,
  "runtime-missing": PackageX,
  unreachable: WifiOff,
};

const STATUS_TONE: Record<McpProbeStatus, string> = {
  healthy: "bg-emerald-500/12 text-emerald-600 ring-emerald-500/25 paper:text-emerald-700",
  "authorization-required": "bg-sky-500/12 text-sky-600 ring-sky-500/25 paper:text-sky-700",
  "runtime-missing": "bg-amber-500/12 text-amber-600 ring-amber-500/25 paper:text-amber-700",
  unreachable: "bg-destructive/12 text-destructive ring-destructive/25",
};

const EPOCH_TONE: Record<McpSpecEpoch, string> = {
  modern: "bg-emerald-500/12 text-emerald-600 ring-emerald-500/25 paper:text-emerald-700",
  legacy: "bg-amber-500/12 text-amber-600 ring-amber-500/25 paper:text-amber-700",
};

interface McpProbePanelProps {
  entry: McpProbeEntry;
  onProbe: () => void;
  className?: string;
}

export function McpProbePanel({ entry, onProbe, className }: McpProbePanelProps) {
  const { t } = useTranslation();
  const report = entry.report;
  const Icon = report ? STATUS_ICON[report.status] : Stethoscope;

  return (
    <div className={cn("space-y-2.5 rounded-xl border border-border/60 bg-background/40 p-3.5", className)}>
      <div className="flex flex-wrap items-center gap-2">
        <p className="flex items-center gap-1.5 text-xs font-semibold text-foreground">
          <Stethoscope className="h-3.5 w-3.5 text-primary" />
          {t("mcp.probeTitle")}
        </p>
        {report ? (
          <span
            className={cn(
              "inline-flex h-5 items-center gap-1 rounded px-1.5 text-micro font-medium ring-1 ring-inset",
              STATUS_TONE[report.status],
            )}
          >
            <Icon className="h-3 w-3" />
            {t(`mcp.probeStatus_${report.status}`)}
          </span>
        ) : null}
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="ml-auto h-7 gap-1.5 px-2 text-[11px]"
          onClick={onProbe}
          disabled={entry.pending}
        >
          <Activity className={entry.pending ? "h-3 w-3 motion-safe:animate-pulse" : "h-3 w-3"} />
          {entry.pending ? t("mcp.probeRunning") : t("mcp.probeRun")}
        </Button>
      </div>

      {entry.error ? <p className="text-[11px] leading-relaxed text-destructive">{entry.error}</p> : null}

      {report ? (
        <div className="space-y-2 text-[11px] text-muted-foreground">
          <p className="leading-relaxed">{t(`mcp.probeStatusHint_${report.status}`)}</p>

          <div className="flex flex-wrap gap-1.5">
            {report.epoch ? (
              <span
                className={cn(
                  "inline-flex h-5 items-center rounded px-1.5 text-micro font-medium ring-1 ring-inset",
                  EPOCH_TONE[report.epoch],
                )}
              >
                {t(`mcp.probeEpoch_${report.epoch}`)}
              </span>
            ) : null}
            {report.protocolVersion ? (
              <span className="inline-flex h-5 items-center rounded bg-muted/70 px-1.5 font-mono text-micro text-foreground/80">
                {t("mcp.probeProtocol", { version: report.protocolVersion })}
              </span>
            ) : null}
            {report.cachePrivate ? (
              <span className="inline-flex h-5 items-center rounded bg-muted/70 px-1.5 text-micro">
                {t("mcp.probeCachePrivate")}
              </span>
            ) : null}
            {report.cacheTtlMs != null ? (
              <span className="inline-flex h-5 items-center rounded bg-muted/70 px-1.5 text-micro">
                {t("mcp.probeCacheTtl", { ms: report.cacheTtlMs })}
              </span>
            ) : null}
          </div>

          {report.epoch ? <p className="leading-relaxed">{t(`mcp.probeEpochHint_${report.epoch}`)}</p> : null}

          {report.status === "authorization-required" && report.authChallenge ? (
            <p className="break-all rounded-lg border border-border/50 bg-muted/40 px-2.5 py-1.5 font-mono">
              {report.authChallenge}
            </p>
          ) : null}

          {report.instructions ? (
            <p className="rounded-lg border border-border/50 bg-muted/30 px-2.5 py-1.5 leading-relaxed text-foreground/80">
              {report.instructions}
            </p>
          ) : null}

          {(report.tools ?? []).length > 0 ? (
            <div className="space-y-1">
              <p className="font-medium text-foreground">{t("mcp.probeTools", { count: report.tools?.length ?? 0 })}</p>
              <div className="flex flex-wrap gap-1">
                {(report.tools ?? []).map((tool) => (
                  <span key={tool} className="rounded bg-muted/70 px-1.5 py-0.5 font-mono text-micro">
                    {tool}
                  </span>
                ))}
              </div>
            </div>
          ) : null}

          {report.error ? <p className="break-all font-mono leading-relaxed text-destructive">{report.error}</p> : null}
        </div>
      ) : (
        <p className="text-[11px] leading-relaxed text-muted-foreground">{t("mcp.probeIdleHint")}</p>
      )}
    </div>
  );
}
