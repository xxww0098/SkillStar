/**
 * Dev-mock fragment: usage mode — subscription/quota tracking. Sample data
 * lives in ./usageData.ts. (These commands are invoked dynamically from
 * src/features/usage/api.ts rather than declared in ../commands/*.ts.)
 */

import type { DevMockHandlers } from "./shared";
import { USAGE_ALERTS, USAGE_CATALOG, USAGE_SUBSCRIPTIONS, USAGE_SUMMARY } from "./usageData";

export const USAGE_HANDLERS: DevMockHandlers = {
  list_usage_catalog: () => USAGE_CATALOG,
  list_subscriptions: () => USAGE_SUBSCRIPTIONS,
  get_active_subscriptions: () => ({
    cursor: "sub-cursor",
    codex: "sub-codex",
    deepseek: "sub-deepseek",
    glm: "sub-glm",
  }),
  // What the CLIs are actually serving. Only catalogs with a CLI behind them
  // appear here; everything else falls back to the pin above, which for those
  // catalogs is the whole truth.
  reconcile_cli_accounts: () => ({
    codex: { kind: "linkedTo", subscriptionId: "sub-codex" },
  }),
  get_subscription_alerts: () => USAGE_ALERTS,
  get_usage_summary: () => USAGE_SUMMARY,
  // Returns full Subscription list (backend shape). Optional catalogId is
  // accepted for API parity; mock still returns every sample row.
  refresh_all_subscriptions: (_args?: Record<string, unknown>) => USAGE_SUBSCRIPTIONS,
  refresh_subscription_usage: (args) => USAGE_SUBSCRIPTIONS.find((s) => s.id === args?.id)?.usage ?? null,
  get_subscription_api_key: () => "sk-demo-********",
  list_desktop_apps: () => [
    { id: "cursor", display_name: "Cursor", catalog_id: "cursor", macos_app_name: "Cursor.app" },
    { id: "grok-bot", display_name: "Grok Bot", catalog_id: null, macos_app_name: "Grok Bot.app" },
    {
      id: "antigravity",
      display_name: "Antigravity",
      catalog_id: "antigravity",
      macos_app_name: "Antigravity.app",
    },
  ],
  list_app_instances: (args) => DEMO_INSTANCES.filter((row) => !args?.app || row.app === args.app),
  create_app_instance: (args) => ({
    id: `inst-${String(args?.app ?? "app")}-new`,
    app: args?.app ?? "cursor",
    name: String(args?.name ?? "New"),
    user_data_dir: `~/.skillstar/instances/${String(args?.app ?? "app")}/new`,
    extra_args: [],
    running: false,
    pid: null,
    created_at: Date.now() / 1000,
  }),
  start_app_instance: (args) => {
    const row = DEMO_INSTANCES.find((item) => item.id === args?.id) ?? DEMO_INSTANCES[0];
    return { ...row, running: true, pid: 4242 };
  },
  stop_app_instance: (args) => {
    const row = DEMO_INSTANCES.find((item) => item.id === args?.id) ?? DEMO_INSTANCES[0];
    return { ...row, running: false, pid: null };
  },
  delete_app_instance: () => undefined,
};

const DEMO_INSTANCES = [
  {
    id: "inst-cursor-work",
    app: "cursor" as const,
    name: "Work",
    user_data_dir: "~/.skillstar/instances/cursor/inst-cursor-work",
    extra_args: [] as string[],
    running: false,
    pid: null as number | null,
    created_at: 1_700_000_000,
  },
];
