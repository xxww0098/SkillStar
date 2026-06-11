import { Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "../../../../lib/utils";
import type { AgentStatus } from "../../lib/agentStatus";
import { getLatencyColor } from "../../lib/latencyColor";

/** Visual tone groups for card chrome (border / accent / glow). */
export type StatusTone = "ok" | "warn" | "bad" | "off" | "busy";

export function statusTone(status: AgentStatus): StatusTone {
  switch (status.kind) {
    case "healthy":
    case "unverified":
      return "ok";
    case "not_installed":
    case "misconfigured":
    case "timeout":
      return "warn";
    case "auth_failed":
    case "error":
      return "bad";
    case "syncing":
      return "busy";
    case "inactive":
      return "off";
  }
}

/** i18n key (under `models.status.*`) for a status — `null` means a raw latency label. */
export function statusLabelKey(status: AgentStatus): string | null {
  switch (status.kind) {
    case "not_installed":
      return "models.status.notInstalled";
    case "inactive":
      return "models.status.inactive";
    case "misconfigured":
      return "models.status.missingEndpoint";
    case "syncing":
      return "models.status.syncing";
    case "unverified":
      return "models.status.connected";
    case "healthy":
      return status.latencyMs != null ? null : "models.status.connected";
    case "auth_failed":
      return "models.status.authFailed";
    case "timeout":
      return "models.status.timeout";
    case "error":
      return status.detail === "model_unavailable" ? "models.status.modelUnavailable" : "models.status.error";
  }
}

const TONE_CHIP: Record<StatusTone, string> = {
  ok: "bg-emerald-500/15 text-emerald-400 ring-emerald-500/20",
  warn: "bg-amber-500/15 text-amber-400 ring-amber-500/20",
  bad: "bg-red-500/15 text-red-400 ring-red-500/20",
  off: "bg-muted text-muted-foreground ring-border",
  busy: "bg-primary/15 text-primary ring-primary/25",
};

/** healthy latency overrides the emerald tint with the canonical latency color. */
const LATENCY_CHIP: Record<string, string> = {
  green: "bg-emerald-500/15 text-emerald-400 ring-emerald-500/20",
  yellow: "bg-amber-500/15 text-amber-400 ring-amber-500/20",
  red: "bg-red-500/15 text-red-400 ring-red-500/20",
  gray: "bg-muted text-muted-foreground ring-border",
};

export interface AgentStatusPillProps {
  status: AgentStatus;
  /** When set and the agent is bound, clicking the pill re-runs the probe. */
  onRetest?: () => void;
  testing?: boolean;
}

export function AgentStatusPill({ status, onRetest, testing }: AgentStatusPillProps) {
  const { t } = useTranslation();
  const tone = statusTone(status);
  const clickable = Boolean(onRetest) && (tone === "ok" || tone === "bad" || status.kind === "timeout");
  const chip = status.kind === "healthy" ? LATENCY_CHIP[getLatencyColor(status.latencyMs)] : TONE_CHIP[tone];

  const labelKey = statusLabelKey(status);
  const label = labelKey
    ? t(labelKey)
    : status.kind === "healthy" && status.latencyMs != null
      ? `${status.latencyMs}ms`
      : "";

  const content = (
    <>
      {(status.kind === "syncing" || testing) && <Loader2 className="h-2.5 w-2.5 animate-spin" />}
      {testing ? t("models.status.testing") : label}
    </>
  );

  const className = cn(
    "inline-flex shrink-0 items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider ring-1",
    chip,
    clickable && "cursor-pointer transition hover:brightness-125",
  );

  if (clickable) {
    return (
      <button type="button" onClick={onRetest} title={t("models.status.retestTitle")} className={className}>
        {content}
      </button>
    );
  }
  return <span className={className}>{content}</span>;
}
