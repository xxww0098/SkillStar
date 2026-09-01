import { Activity, KeyRound, PackageX, Stethoscope, WifiOff } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { InsetPanel } from "../../../components/ui/InsetPanel";
import { StatusChip, type StatusChipTone } from "../../../components/ui/StatusChip";
import type { McpProbeStatus } from "../../../types";
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

const STATUS_TONE: Record<McpProbeStatus, StatusChipTone> = {
  healthy: "success",
  "authorization-required": "info",
  "runtime-missing": "warning",
  unreachable: "danger",
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
    <InsetPanel className={className}>
      <div className="flex flex-wrap items-center gap-2">
        <p className="flex items-center gap-1.5 text-xs font-semibold text-foreground">
          <Stethoscope className="h-3.5 w-3.5 text-primary" />
          {t("mcp.probeTitle")}
        </p>
        {report ? (
          <StatusChip tone={STATUS_TONE[report.status]}>
            <Icon className="h-3 w-3" />
            {t(`mcp.probeStatus_${report.status}`)}
          </StatusChip>
        ) : null}
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="ml-auto h-7 gap-1.5 px-2 text-[11px]"
          onClick={onProbe}
          disabled={entry.pending}
        >
          <Activity className={entry.pending ? "h-3 w-3 animate-pulse" : "h-3 w-3"} />
          {entry.pending ? t("mcp.probeRunning") : t("mcp.probeRun")}
        </Button>
      </div>

      {entry.error ? <p className="text-[11px] leading-relaxed text-destructive">{entry.error}</p> : null}

      {report ? (
        <div className="space-y-2 text-[11px] text-muted-foreground">
          <p className="leading-relaxed">{t(`mcp.probeStatusHint_${report.status}`)}</p>

          <div className="flex flex-wrap gap-x-3 gap-y-1">
            {report.epoch ? <span>{t(`mcp.probeEpoch_${report.epoch}`)}</span> : null}
            {report.protocolVersion ? (
              <span className="font-mono">{t("mcp.probeProtocol", { version: report.protocolVersion })}</span>
            ) : null}
            {report.cachePrivate ? <span>{t("mcp.probeCachePrivate")}</span> : null}
          </div>

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
    </InsetPanel>
  );
}
