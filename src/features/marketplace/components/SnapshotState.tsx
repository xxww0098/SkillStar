//! Render layer for marketplace snapshot health.
//!
//! `useMarketplace` emits structured `MarketplaceError` values; everything the
//! user actually reads is decided here, through i18n keys. Raw backend strings
//! live in a collapsed "details" disclosure so they stay available for support
//! without dumping an anyhow chain into the page.

import { AlertTriangle, CloudOff, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { EmptyState } from "../../../components/ui/EmptyState";
import { cn } from "../../../lib/utils";
import type { SnapshotStatus } from "../../../types";
import { MARKETPLACE_ERROR_COPY, type MarketplaceError } from "../lib/snapshotState";

interface SnapshotErrorBannerProps {
  error: MarketplaceError;
  refreshing: boolean;
  onRetry: () => void;
}

/** Inline banner above the content area: what broke, and how to recover. */
export function SnapshotErrorBanner({ error, refreshing, onRetry }: SnapshotErrorBannerProps) {
  const { t } = useTranslation();
  const copy = MARKETPLACE_ERROR_COPY[error.kind];
  const isError = copy.severity === "error";
  const Icon = isError ? CloudOff : AlertTriangle;

  return (
    <div
      role="alert"
      className={cn(
        "flex items-start gap-3 border-b px-6 py-2",
        isError
          ? "border-destructive/20 bg-destructive/5 text-destructive"
          : "border-amber-500/20 bg-amber-500/5 text-amber-700 dark:text-amber-400",
      )}
    >
      <Icon className="mt-0.5 h-3.5 w-3.5 shrink-0" />
      <div className="min-w-0 flex-1">
        <p className="text-xs font-medium">{t(copy.titleKey)}</p>
        <p className="text-[11px] opacity-80">{t(copy.descriptionKey)}</p>
        {error.detail && (
          <details className="mt-1">
            <summary className="cursor-pointer text-[11px] opacity-60">{t("marketplace.errorDetailLabel")}</summary>
            <p className="mt-1 max-h-24 overflow-y-auto break-all font-mono text-[10px] leading-relaxed opacity-70">
              {error.detail}
            </p>
          </details>
        )}
      </div>
      <Button
        variant="outline"
        size="sm"
        onClick={onRetry}
        disabled={refreshing}
        className="h-7 shrink-0 gap-1.5 text-xs"
      >
        <RefreshCw className={refreshing ? "h-3 w-3 animate-spin" : "h-3 w-3"} />
        {t("common.retry")}
      </Button>
    </div>
  );
}

interface SnapshotEmptyStateProps {
  /** Only `miss` and `remote_error` reach here; anything else renders nothing. */
  status: SnapshotStatus | null;
  refreshing: boolean;
  onRetry: () => void;
}

/**
 * Empty state for the two statuses where "no rows" does NOT mean "the market is
 * empty": the snapshot never landed (`miss`) or the remote fetch failed
 * (`remote_error`). Both get an explicit action instead of a dead end.
 */
export function SnapshotEmptyState({ status, refreshing, onRetry }: SnapshotEmptyStateProps) {
  const { t } = useTranslation();
  if (status !== "miss" && status !== "remote_error") return null;

  const isMiss = status === "miss";

  return (
    <EmptyState
      icon={
        isMiss ? (
          <RefreshCw className="h-6 w-6 text-muted-foreground" />
        ) : (
          <CloudOff className="h-6 w-6 text-muted-foreground" />
        )
      }
      title={isMiss ? t("marketplace.snapshotMissTitle") : t("marketplace.remoteErrorTitle")}
      description={isMiss ? t("marketplace.snapshotMissDescription") : t("marketplace.remoteErrorEmptyDescription")}
      action={
        <Button variant="outline" size="sm" onClick={onRetry} disabled={refreshing} className="gap-1.5">
          <RefreshCw className={refreshing ? "h-4 w-4 animate-spin" : "h-4 w-4"} />
          {isMiss ? t("marketplace.syncNow") : t("common.retry")}
        </Button>
      }
      size="lg"
    />
  );
}
