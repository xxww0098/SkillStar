import { BadgeCheck, GripVertical, TriangleAlert, Unplug } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import type { BrandTheme } from "../../lib/brandThemes";
import type { CliAccountBadge } from "../../lib/cliCustody";
import { PlanBadge } from "../PlanBadge";
import { hasBrandIcon, ProviderLogo } from "../ProviderLogo";
import { usageCardSlotClassName } from "./usageCardShell";

export interface UsageCardHeaderProps {
  catalogId: string;
  displayName: string;
  brandColorHex: string;
  theme: BrandTheme;
  planName: string | null;
  /**
   * What the local tool is actually doing with this account.
   *
   * Three states, not a boolean: "the tool is on this account", "the tool is on
   * something else", and "the tool has nobody" are different things to tell a
   * user, and the last two used to render identically to "not current".
   */
  cliBadge?: CliAccountBadge;
  onDragHandlePointerDown?: (e: React.PointerEvent) => void;
}

/**
 * Colour is the honest part of the signal: a card the local tool has moved off must
 * not keep wearing the same green chip a serving card wears.
 */
const BADGE_STYLES: Record<Exclude<CliAccountBadge, "none">, string> = {
  current: "bg-emerald-400/25 text-emerald-50 ring-emerald-200/50",
  diverged: "bg-amber-400/30 text-amber-50 ring-amber-200/60",
  missing: "bg-zinc-900/30 text-zinc-100 ring-white/30",
};

const BADGE_COPY: Record<Exclude<CliAccountBadge, "none">, { label: string; title: string }> = {
  current: { label: "usage.cardActive", title: "usage.cardActiveTitle" },
  diverged: { label: "usage.cardCliDiverged", title: "usage.cardCliDivergedTitle" },
  missing: { label: "usage.cardCliMissing", title: "usage.cardCliMissingTitle" },
};

/** Brand band — identity, local-tool state, plan, drag. */
export function UsageCardHeader({
  catalogId,
  displayName,
  brandColorHex,
  theme,
  planName,
  cliBadge = "none",
  onDragHandlePointerDown,
}: UsageCardHeaderProps) {
  const { t } = useTranslation();
  const brandIcon = hasBrandIcon(catalogId);
  const badgeCopy =
    catalogId === "antigravity" || catalogId === "cursor"
      ? {
          ...BADGE_COPY,
          diverged: { label: "usage.cardIdeDiverged", title: "usage.cardIdeDivergedTitle" },
          missing: { label: "usage.cardIdeMissing", title: "usage.cardIdeMissingTitle" },
        }
      : BADGE_COPY;
  const isEmailIdentity = displayName.includes("@");
  const badgeLabel = cliBadge !== "none" ? t(badgeCopy[cliBadge].label) : "";
  const badgeTitle = cliBadge !== "none" ? t(badgeCopy[cliBadge].title) : "";

  return (
    <div
      className={usageCardSlotClassName.headerBand}
      style={{ background: `linear-gradient(135deg, ${theme.header[0]}, ${theme.header[1]})`, color: theme.fg }}
    >
      <div className="relative flex items-start gap-3" style={{ textShadow: "0 1px 2px rgba(0,0,0,0.18)" }}>
        {brandIcon ? (
          // text-zinc-900 fixes mono logos that inherit band white fg on the white chip.
          <div
            className="grid h-9 w-9 shrink-0 place-items-center rounded-xl bg-white text-zinc-900 shadow-[0_2px_8px_rgba(0,0,0,0.18)] ring-1 ring-black/5 [text-shadow:none]"
            aria-hidden
          >
            <ProviderLogo catalogId={catalogId} displayName={displayName} brandColor={brandColorHex} size="md" />
          </div>
        ) : (
          <div aria-hidden className="shrink-0">
            <ProviderLogo
              catalogId={catalogId}
              displayName={displayName}
              brandColor={brandColorHex}
              size="lg"
              className="shadow-[0_2px_8px_rgba(0,0,0,0.2)] ring-1 ring-white/30"
            />
          </div>
        )}
        <div className="min-w-0 flex-1">
          <h3
            className={cn(
              "min-h-[2.25rem] min-w-0 pr-1 whitespace-normal text-sm leading-snug font-bold",
              isEmailIdentity && "break-all text-[13px]",
            )}
          >
            {displayName}
          </h3>
          <div className="mt-1 flex h-[18px] items-center gap-1.5 overflow-hidden">
            {cliBadge !== "none" && (
              <span
                className={cn(
                  "inline-flex min-w-0 max-w-full items-center gap-0.5 rounded-md px-1.5 py-0.5 text-[10px] leading-none font-bold tracking-wide ring-1 backdrop-blur-[2px]",
                  BADGE_STYLES[cliBadge],
                )}
                title={badgeTitle}
                data-cli-badge={cliBadge}
              >
                {cliBadge === "current" ? (
                  <BadgeCheck className="h-2.5 w-2.5 shrink-0" aria-hidden />
                ) : cliBadge === "diverged" ? (
                  <TriangleAlert className="h-2.5 w-2.5 shrink-0" aria-hidden />
                ) : (
                  <Unplug className="h-2.5 w-2.5 shrink-0" aria-hidden />
                )}
                <span className="truncate">{badgeLabel}</span>
              </span>
            )}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-1 self-start">
          <PlanBadge plan={planName} variant="onBrand" />
          <button
            type="button"
            onPointerDown={onDragHandlePointerDown}
            className={cn(
              "grid size-6 shrink-0 place-items-center rounded-md text-current/80",
              "hover:bg-white/15 hover:text-current focus-visible:ring-2 focus-visible:ring-white/80 focus-visible:outline-none",
              "cursor-grab active:cursor-grabbing",
            )}
            aria-label={t("usage.dragHandle")}
            tabIndex={-1}
          >
            <GripVertical className="h-3.5 w-3.5" aria-hidden />
          </button>
        </div>
      </div>
    </div>
  );
}
