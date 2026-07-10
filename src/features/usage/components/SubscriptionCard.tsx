import { motion } from "framer-motion";
import { getBrandTheme } from "../lib/brandThemes";
import { monthlyEquivalentPrice } from "../lib/pricing";
import { getPrimaryResetInfo } from "../lib/usageLabels";
import { computeBodyOwnsPrimaryReset } from "../lib/resetOwnership";
import type { CatalogEntry, Subscription } from "../types";
import { priorityCardClass } from "./ResetCountdown";
import {
  UsageCardBody,
  UsageCardFooter,
  UsageCardHeader,
  UsageCardMetaStrip,
  brandThemeToCssVars,
  resolveUsageBodyRegistration,
  usageCardShellClassName,
} from "./card";

interface SubscriptionCardProps {
  subscription: Subscription;
  catalog: CatalogEntry | undefined;
  onRefresh: (id: string) => Promise<void>;
  onEdit: (id: string) => void;
  onDelete: (id: string) => void;
  onReauth?: (id: string) => void;
  /** Switch this subscription to be the active account for its catalog
   *  (Phase 7 multi-account). When omitted, the switch button is hidden. */
  onSetActive?: (id: string) => Promise<void>;
  /** Re-push the active account's credentials to its CLI config (retry path
   *  shown when the previous CLI switch failed). Catalog must support CLI
   *  switching (`supports_cli_switch`). */
  onSwitchToCli?: (catalogId: string) => Promise<void>;
  refreshDisabled?: boolean;
  /** Drag handle pointer-down; passed through to dnd lib. */
  onDragHandlePointerDown?: (e: React.PointerEvent) => void;
}

/**
 * Thin composition shell: motion.article + frozen shell tokens + header/meta/body/footer.
 * Vendor body selection lives in `UsageCardBody` / `bodyRegistry` (not here).
 */
export function SubscriptionCard({
  subscription: sub,
  catalog,
  onRefresh,
  onEdit,
  onDelete,
  onReauth,
  onSetActive,
  onSwitchToCli,
  refreshDisabled = false,
  onDragHandlePointerDown,
}: SubscriptionCardProps) {
  const usage = sub.usage ?? null;
  const planName = (usage?.plan_name ?? sub.plan_tier ?? null) || null;
  const showRenewFooter = sub.renew_date > 0;
  const renewDays = daysUntil(sub.renew_date);
  const monthlyCost = monthlyEquivalentPrice(sub);
  const resetInfo = getPrimaryResetInfo(usage);
  const reg = resolveUsageBodyRegistration(sub.catalog_id);
  const bodyOwnsPrimaryReset = computeBodyOwnsPrimaryReset(usage, resetInfo, reg.ownsPrimaryReset);
  const brandColorHex = catalog?.brand_color ?? "6B7280";
  const theme = getBrandTheme(sub.catalog_id, brandColorHex);

  return (
    <motion.article
      layout
      initial={{ opacity: 0, scale: 0.96 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.96 }}
      transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
      style={brandThemeToCssVars(theme)}
      className={usageCardShellClassName({
        isActive: sub.is_active,
        requiresReauth: sub.requires_reauth,
        priorityClass: resetInfo && priorityCardClass(resetInfo.resetAt, resetInfo.usedPercent, resetInfo.mode),
      })}
      aria-label={sub.display_name}
    >
      <header className="relative z-10">
        <UsageCardHeader
          catalogId={sub.catalog_id}
          displayName={sub.display_name}
          description={catalog?.description}
          brandColorHex={brandColorHex}
          theme={theme}
          planName={planName}
          onDragHandlePointerDown={onDragHandlePointerDown}
        />
        <UsageCardMetaStrip
          authMode={sub.auth_mode}
          isActive={sub.is_active}
          fetchedAt={usage?.fetched_at ?? 0}
          resetInfo={resetInfo}
          bodyOwnsPrimaryReset={bodyOwnsPrimaryReset}
        />
      </header>

      <div className="relative z-10 flex-1 space-y-3.5 overflow-hidden px-4 pt-3 pb-2">
        <UsageCardBody subscription={sub} brandColorHex={brandColorHex} density="comfortable" surface="grid" />
      </div>

      <UsageCardFooter
        subscription={sub}
        monthlyCost={monthlyCost}
        showRenewFooter={showRenewFooter}
        renewDays={renewDays}
        subscriptionUrl={catalog?.subscription_url}
        onRefresh={() => onRefresh(sub.id)}
        onEdit={() => onEdit(sub.id)}
        onDelete={() => onDelete(sub.id)}
        onReauth={onReauth ? () => onReauth(sub.id) : undefined}
        onSetActive={onSetActive ? () => onSetActive(sub.id) : undefined}
        onSwitchToCli={onSwitchToCli ? () => onSwitchToCli(sub.catalog_id) : undefined}
        refreshDisabled={refreshDisabled}
      />
    </motion.article>
  );
}

function daysUntil(epoch: number): number | null {
  if (!epoch || epoch <= 0) return null;
  const now = Math.floor(Date.now() / 1000);
  const diff = epoch - now;
  return Math.floor(diff / 86_400);
}
