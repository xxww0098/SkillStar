import { AlertTriangle, Box, Check, Cloud, Package, Sparkles, TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "../../../lib/utils";
import type { McpRuntimeCandidate, McpRuntimeSelection, McpRuntimeShape } from "../../../types";

/**
 * Runtime shape picker.
 *
 * A server usually publishes several shapes — a hosted endpoint and one or more
 * local packages — and which one gets installed used to be decided by array
 * order. The backend now ranks them (remote streamable-http → sse → oci → mcpb
 * → plain packages, with unavailable toolchains demoted) and returns *all* of
 * them plus the recommendation, so this component's job is to show why the
 * recommendation won and let the user disagree.
 *
 * Two things must never be swallowed: an `sse` candidate carries a deprecated
 * transport, and a candidate can be listed yet not installable here (an MCPB
 * bundle SkillStar cannot verify, a missing launcher). Both come through as
 * `warnings` / `blockedReason` and are rendered verbatim.
 */

const SHAPE_ICON: Record<McpRuntimeShape, typeof Cloud> = {
  remoteStreamableHttp: Cloud,
  remoteSse: Cloud,
  packageOci: Box,
  packageMcpb: Package,
  packagePlain: Package,
};

function candidateTitle(candidate: McpRuntimeCandidate): string {
  if (candidate.url) return candidate.url;
  const version = candidate.version ? `@${candidate.version}` : "";
  return `${candidate.identifier ?? candidate.registryType ?? ""}${version}`;
}

interface McpRuntimePickerProps {
  selection: McpRuntimeSelection;
  selectedId: string | null;
  onSelect: (candidateId: string) => void;
  disabled?: boolean;
}

export function McpRuntimePicker({ selection, selectedId, onSelect, disabled }: McpRuntimePickerProps) {
  const { t } = useTranslation();
  const effectiveId = selectedId ?? selection.recommendedId ?? null;

  if (selection.candidates.length === 0) {
    return (
      <p className="rounded-lg border border-border/60 bg-background/40 px-3 py-2 text-xs text-muted-foreground">
        {t("mcp.runtimeNone")}
      </p>
    );
  }

  return (
    <div className="space-y-2">
      {selection.candidates.map((candidate) => {
        const Icon = SHAPE_ICON[candidate.shape] ?? Package;
        const active = candidate.id === effectiveId;
        const recommended = candidate.id === selection.recommendedId;
        return (
          <button
            key={candidate.id}
            type="button"
            aria-pressed={active}
            disabled={disabled || !candidate.installable}
            onClick={() => onSelect(candidate.id)}
            className={cn(
              "w-full rounded-lg border px-3 py-2.5 text-left transition",
              active
                ? "border-primary/60 bg-primary/8"
                : "border-border/70 bg-background/40 hover:border-border hover:bg-muted/30",
              !candidate.installable && "cursor-not-allowed opacity-60",
            )}
          >
            <div className="flex items-center gap-2">
              <Icon className="h-3.5 w-3.5 shrink-0 text-primary" />
              <span className="text-xs font-medium text-foreground">{t(`mcp.shape_${candidate.shape}`)}</span>
              <span className="rounded bg-muted/70 px-1.5 py-0.5 font-mono text-micro text-muted-foreground">
                {candidate.transport}
              </span>
              {recommended ? (
                <span className="inline-flex items-center gap-1 rounded bg-primary/12 px-1.5 py-0.5 text-micro font-medium text-primary">
                  <Sparkles className="h-3 w-3" />
                  {t("mcp.runtimeRecommended")}
                </span>
              ) : null}
              {active ? <Check className="ml-auto h-3.5 w-3.5 shrink-0 text-primary" /> : null}
            </div>

            <p className="mt-1 break-all font-mono text-[11px] text-muted-foreground">{candidateTitle(candidate)}</p>

            {candidate.shape === "remoteStreamableHttp" ? (
              <p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
                {t("mcp.shapeHint_remoteStreamableHttp")}
              </p>
            ) : null}

            {candidate.runtimeCommand ? (
              <p className="mt-1 text-[11px] text-muted-foreground">
                {t("mcp.runtimeLauncher", { command: candidate.runtimeCommand })}
                <span
                  className={cn(
                    "ml-1.5 font-medium",
                    candidate.runtimeAvailable === false
                      ? "text-amber-600 dark:text-amber-400"
                      : "text-emerald-600 dark:text-emerald-400",
                  )}
                >
                  {candidate.runtimeAvailable === false ? t("mcp.runtimeMissing") : t("mcp.runtimeAvailable")}
                </span>
              </p>
            ) : null}

            {candidate.blockedReason ? (
              <p className="mt-1.5 flex items-start gap-1.5 text-[11px] leading-relaxed text-destructive">
                <TriangleAlert className="mt-0.5 h-3 w-3 shrink-0" />
                {candidate.blockedReason}
              </p>
            ) : null}

            {(candidate.warnings ?? []).map((warning) => (
              <p
                key={warning}
                className="mt-1.5 flex items-start gap-1.5 text-[11px] leading-relaxed text-amber-600 dark:text-amber-400"
              >
                <AlertTriangle className="mt-0.5 h-3 w-3 shrink-0" />
                {warning}
              </p>
            ))}
          </button>
        );
      })}
    </div>
  );
}
