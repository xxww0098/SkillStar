/**
 * Dev-mock fragment: usage mode — subscription/quota tracking. Sample data
 * lives in ./usageData.ts. (These commands are invoked dynamically from
 * src/features/usage/api.ts rather than declared in ../commands/*.ts.)
 */

import type { OAuthFlow } from "@/types/generated/OAuthFlow";
import type { OAuthStart } from "@/types/generated/OAuthStart";

import type { DevMockHandlers } from "./shared";
import { USAGE_ALERTS, USAGE_CATALOG, USAGE_SUBSCRIPTIONS, USAGE_SUMMARY } from "./usageData";

/**
 * Browser-dev `start_oauth_login`. Defaults to today's local-callback panel.
 * Tests (and a manual preview) pass `flow` — `"remote-poll"`, `"scheme-paste"`,
 * or `"immediate"` — without adding a catalog row. Optional `user_code`,
 * `verification_uri`, `interval_secs`, and `scheme_prefix` fill the other shapes.
 */
export function mockOAuthStart(args: Record<string, unknown> = {}): OAuthStart {
  const requested = args.flow ?? "local-callback";
  const base = {
    pending_id: typeof args.pending_id === "string" ? args.pending_id : "pending-demo",
    auth_url: typeof args.auth_url === "string" ? args.auth_url : "https://example.test/oauth/authorize",
    user_code: null,
    verification_uri: null,
    interval_secs: null,
  };
  const catalog = args.catalogId ?? args.catalog_id;
  // Qoder's real login is a remote poll with no user code. The default mock
  // still shows a device code, which would mis-preview this catalog.
  if (catalog === "qoder" && args.flow == null) {
    return {
      ...base,
      auth_url: "https://qoder.com/device/selectAccounts",
      flow: "remote-poll",
      user_code: null,
      verification_uri: null,
      interval_secs: 1,
    };
  }
  if (typeof catalog === "string" && (catalog === "trae" || catalog.startsWith("trae-")) && args.flow == null) {
    return {
      ...base,
      auth_url: catalog.endsWith("cn") ? "https://www.trae.cn" : "https://www.trae.ai",
      flow: "immediate",
    };
  }
  if ((catalog === "codebuddy" || catalog === "codebuddy-cn") && args.flow == null) {
    return {
      ...base,
      auth_url: catalog === "codebuddy-cn" ? "https://www.codebuddy.cn/login" : "https://www.codebuddy.ai/login",
      flow: "remote-poll",
      user_code: null,
      verification_uri: null,
      interval_secs: 2,
    };
  }
  if (requested === "remote-poll") {
    const verification =
      typeof args.verification_uri === "string" ? args.verification_uri : "https://example.test/device";
    return {
      ...base,
      auth_url: verification,
      flow: "remote-poll",
      user_code: typeof args.user_code === "string" ? args.user_code : "ABCD-EFGH",
      verification_uri: verification,
      interval_secs: typeof args.interval_secs === "number" ? args.interval_secs : 5,
    };
  }
  if (requested === "immediate") {
    return { ...base, flow: "immediate" };
  }
  if (requested === "scheme-paste" || isSchemePaste(requested)) {
    const prefix = schemePrefixFrom(requested, args);
    return {
      ...base,
      auth_url: `${prefix}login`,
      flow: { "scheme-paste": { scheme_prefix: prefix } },
    };
  }
  return { ...base, flow: "local-callback" };
}

function isSchemePaste(flow: unknown): flow is OAuthFlow & object {
  return typeof flow === "object" && flow !== null && "scheme-paste" in flow;
}

function schemePrefixFrom(flow: unknown, args: Record<string, unknown>): string {
  if (typeof args.scheme_prefix === "string" && args.scheme_prefix.length > 0) return args.scheme_prefix;
  if (isSchemePaste(flow)) return flow["scheme-paste"].scheme_prefix;
  return "zcode://";
}

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
  // Default shape keeps the current paste panel. `await` never resolves so the
  // panel stays on screen in browser dev; cancel just resets the dialog.
  start_oauth_login: (args) => mockOAuthStart(args),
  await_oauth_completion: () => new Promise(() => {}),
  submit_oauth_callback: () => undefined,
  cancel_oauth_login: () => undefined,
  // The paste stays in the request. The mock returns a card and does not echo it.
  import_subscription_token: () => USAGE_SUBSCRIPTIONS[0],
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
