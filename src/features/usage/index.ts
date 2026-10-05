//! Public surface of the Usage feature. Other features (the Accounts mode)
//! must import from here — deep imports into `components/` or `hooks/` are
//! rejected by `check_feature_imports.sh`.

// Data access
export { UsageDataProvider, useUsageDataContext } from "./context/UsageDataContext";

// Management surface consumed by the Accounts mode
export { SubscriptionEditDialog } from "./components/SubscriptionEditDialog";
export { UsageActionBar } from "./components/UsageActionBar";
export { UsageGrid } from "./components/UsageGrid";
export { UsageGridSkeleton } from "./components/UsageSkeleton";
export { UsageAlertBanner } from "./components/UsageAlertBanner";
export { UsageRefreshControl } from "./components/UsageRefreshControl";
export { ProviderLogo } from "./components/ProviderLogo";

// Shared vocabulary
export type { CatalogEntry, CatalogFilter, Subscription } from "./types";
export { FILTER_ALL, GROK_BOT_FILTER } from "./types";
export { readHideAccountEmails, writeHideAccountEmails } from "./lib/accountPrivacy";
export { isDegradedCopyBinding } from "./lib/cliCustody";
