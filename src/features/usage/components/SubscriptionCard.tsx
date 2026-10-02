import { motion, useReducedMotion } from "framer-motion";
import { useState } from "react";
import { cn } from "@/lib/utils";
import { getBrandTheme } from "../lib/brandThemes";
import { desktopAppIdForCatalog } from "../lib/desktopApps";
import { cliAccountBadgeFor } from "../lib/cliCustody";
import { monthlyEquivalentPrice } from "../lib/pricing";
import { getPrimaryResetInfo, subscriptionCardTitle } from "../lib/usageLabels";
import { displayAccountIdentity } from "../lib/accountPrivacy";
import { computeBodyOwnsPrimaryReset } from "../lib/resetOwnership";
import type { CatalogEntry, CliAccountState, ConsumptionTotals, CreditInfo, Subscription } from "../types";
import { priorityCardClass } from "./ResetCountdown";
import {
  TodayConsumptionLine,
  UsageCardBody,
  UsageCardFooter,
  UsageCardHeader,
  UsageCardMetaStrip,
  brandThemeToCssVars,
  resolveUsageBodyRegistration,
  usageCardShellClassName,
  usageCardSlotClassName,
} from "./card";
import { AppInstancesOverlay } from "./instances/AppInstancesOverlay";

interface SubscriptionCardProps {
  subscription: Subscription;
  catalog: CatalogEntry | undefined;
  /** Today's gateway-metered consumption for this card's provider, from the
   *  page-wide `by_catalog` summary. `undefined` (summary not loaded yet)
   *  hides the line; `null` renders the "no records yet" sentence. */
  todayConsumption?: ConsumptionTotals | null;
  /** `catalog_id -> which account that CLI is actually serving`. The "current"
   *  badge is drawn from this, not from the `is_active` pin, which is only a
   *  cache of it. Absent entries fall back to the pin. */
  cliAccounts?: Record<string, CliAccountState>;
  hideAccountEmails?: boolean;
  onRefresh: (id: string) => Promise<void>;
  onResetQuota?: (id: string) => Promise<void>;
  onEdit: (id: string) => void;
  onDelete: (id: string) => void;
  onReauth?: (id: string) => void;
  /** Switch this subscription to be the active account for its catalog
   *  (Phase 7 multi-account). When omitted, the switch button is hidden. */
  onSetActive?: (id: string) => Promise<void>;
  /** Re-push the active account's credentials to its local tool (retry path
   *  shown when the previous switch failed). Catalog must support local-tool
   *  switching (`supports_cli_switch`, retained as the wire field name). */
  onSwitchToCli?: (catalogId: string) => Promise<void>;
  refreshDisabled?: boolean;
  /** Drag handle pointer-down; passed through to dnd lib. */
  onDragHandlePointerDown?: (e: React.PointerEvent) => void;
  /** When set, the card's today line becomes the cross-view entry: which
   *  agents route to this provider (Usage → Models triangle, slice 13). */
  onOpenGatewayRoutes?: () => void;
}

/**
 * Thin composition shell: motion.article + frozen shell tokens + header/meta/body/footer.
 * Vendor body selection lives in `UsageCardBody` / `bodyRegistry` (not here).
 */
export function SubscriptionCard({
  subscription: sub,
  catalog,
  todayConsumption,
  cliAccounts,
  hideAccountEmails = false,
  onRefresh,
  onResetQuota,
  onEdit,
  onDelete,
  onReauth,
  onSetActive,
  onSwitchToCli,
  refreshDisabled = false,
  onDragHandlePointerDown,
  onOpenGatewayRoutes,
}: SubscriptionCardProps) {
  const reduceMotion = useReducedMotion();
  const [instancesOpen, setInstancesOpen] = useState(false);
  const instanceApp = desktopAppIdForCatalog(sub.catalog_id);
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
  const cardTitle = subscriptionCardTitle(sub.display_name, catalog?.display_name);
  const cardDisplayName = displayAccountIdentity(cardTitle, hideAccountEmails);
  const cliBadge = cliAccountBadgeFor(sub, cliAccounts ?? {});
  const resetCreditsRemaining = readGrokResetCredits(sub.catalog_id, usage?.credits);

  return (
    <motion.article
      layout={!reduceMotion}
      initial={reduceMotion ? false : { opacity: 0, scale: 0.96 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={reduceMotion ? undefined : { opacity: 0, scale: 0.96 }}
      transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
      style={brandThemeToCssVars(theme)}
      className={cn(
        usageCardShellClassName({
          // The ring means "the CLI is on this account", so it follows the
          // reconciled state and not the pin that only records the request.
          isActive: cliBadge === "current",
          requiresReauth: sub.requires_reauth,
          priorityClass: resetInfo && priorityCardClass(resetInfo.resetAt, resetInfo.usedPercent, resetInfo.mode),
        }),
        "h-full",
      )}
      aria-label={cardDisplayName}
    >
      <header className="relative z-10">
        <UsageCardHeader
          catalogId={sub.catalog_id}
          displayName={cardDisplayName}
          brandColorHex={brandColorHex}
          theme={theme}
          planName={planName}
          cliBadge={cliBadge}
          onDragHandlePointerDown={onDragHandlePointerDown}
        />
        <UsageCardMetaStrip
          requiresReauth={sub.requires_reauth}
          hasCredential={sub.has_credential}
          note={sub.note}
          resetInfo={resetInfo}
          bodyOwnsPrimaryReset={bodyOwnsPrimaryReset}
        />
      </header>

      <div className={usageCardSlotClassName.body}>
        <UsageCardBody subscription={sub} brandColorHex={brandColorHex} density="comfortable" surface="grid" />
        {/* One quiet today line closes the body; the quota meters above it
         *  stay the card's hero (card rhythm rule). With traffic it doubles
         *  as the cross-view entry into the agents this provider serves. */}
        {todayConsumption !== undefined && (
          <TodayConsumptionLine today={todayConsumption} onNavigate={onOpenGatewayRoutes} />
        )}
      </div>

      <UsageCardFooter
        subscription={sub}
        monthlyCost={monthlyCost}
        showRenewFooter={showRenewFooter}
        renewDays={renewDays}
        fetchedAt={usage?.fetched_at ?? 0}
        subscriptionUrl={catalog?.subscription_url}
        onRefresh={() => onRefresh(sub.id)}
        onResetQuota={sub.catalog_id === "xai" && onResetQuota ? () => onResetQuota(sub.id) : undefined}
        resetCreditsRemaining={resetCreditsRemaining}
        onEdit={() => onEdit(sub.id)}
        onDelete={() => onDelete(sub.id)}
        onReauth={onReauth ? () => onReauth(sub.id) : undefined}
        onSetActive={onSetActive ? () => onSetActive(sub.id) : undefined}
        onSwitchToCli={onSwitchToCli ? () => onSwitchToCli(sub.catalog_id) : undefined}
        onOpenInstances={instanceApp ? () => setInstancesOpen(true) : undefined}
        refreshDisabled={refreshDisabled}
      />
      {instancesOpen && instanceApp ? (
        <AppInstancesOverlay appId={instanceApp} onClose={() => setInstancesOpen(false)} />
      ) : null}
    </motion.article>
  );
}

const GROK_RESET_CREDITS = "grok-reset-credits";

function readGrokResetCredits(catalogId: string, credits?: CreditInfo[]): number | null {
  if (catalogId !== "xai") return null;
  const raw = credits?.find((credit) => credit.credit_type === GROK_RESET_CREDITS)?.credit_amount;
  if (raw == null || !/^\d+$/.test(raw.trim())) return null;
  const count = Number(raw);
  return Number.isSafeInteger(count) ? count : null;
}

function daysUntil(epoch: number): number | null {
  if (!epoch || epoch <= 0) return null;
  const now = Math.floor(Date.now() / 1000);
  const diff = epoch - now;
  return Math.floor(diff / 86_400);
}
