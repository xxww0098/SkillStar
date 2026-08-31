import type {
  LocalFirstResult,
  McpCustomSource,
  McpInstallAnswer,
  McpInstallOutcome,
  McpInstallPlan,
  McpInstallPreview,
  McpMarketServerDetail,
  McpPublisherSummary,
  McpServerPage,
  McpServerQuery,
  McpSourceDescriptor,
  SyncStateEntry,
} from "../../../types";

/**
 * MCP marketplace — browse the merged MCP catalog local-first, then install
 * via `mcp_market_install_plan` for the confirmation payload,
 * `mcp_market_install_preview` for the entry the collected answers produce,
 * and `mcp_market_install` to commit it. The manual "add server" form keeps
 * submitting through `create_mcp_server`: it carries a user-authored entry, so
 * a publisher's required inputs have nothing to be enforced against.
 *
 * The catalog is the merge of every enabled source (the official MCP Registry
 * as primary, GitHub's registry as an enrichment mirror, plus any user-added
 * registry URL or local JSON directory) — see `list_mcp_sources` and friends.
 *
 * Backed by `skillstar_marketplace::mcp_snapshot` (cache + FTS) and
 * `skillstar_app::mcp` (runtime selection + install plan) via
 * `src-tauri/src/commands/mcp_marketplace.rs`.
 */
export interface McpMarketplaceCommands {
  /**
   * Filtered / sorted / paginated card query. Every field of `McpServerQuery`
   * has a Rust-side default, so pass only the ones you set. The result carries
   * `total` alongside the page, so "showing 60 of 21363" needs no second call.
   */
  query_mcp_market_servers_local: {
    args: { query: Partial<McpServerQuery> };
    result: LocalFirstResult<McpServerPage>;
  };
  list_mcp_publishers_local: {
    args: Record<string, never>;
    result: McpPublisherSummary[];
  };
  get_mcp_market_server_detail_local: {
    args: { id: string };
    result: LocalFirstResult<McpMarketServerDetail | null>;
  };
  sync_mcp_market_scope: { args: { scope: string }; result: void };
  /**
   * One row per source (`mcp_registry:<sourceId>`), each with its own
   * `lastError` and `degradedReason` — this is what lets the UI say "this sync
   * was incomplete, because X" instead of reporting a partial catalog as whole.
   */
  get_mcp_source_sync_states: { args: Record<string, never>; result: SyncStateEntry[] };

  /** Every configured source: built-ins with user overrides, then user sources. */
  list_mcp_sources: { args: Record<string, never>; result: McpSourceDescriptor[] };
  /** Add (or replace) a user registry URL / local JSON directory file. */
  add_mcp_source: { args: { source: McpCustomSource }; result: McpSourceDescriptor[] };
  remove_mcp_source: { args: { id: string }; result: McpSourceDescriptor[] };
  /** Turn any source on/off — built-in ids included. */
  set_mcp_source_enabled: { args: { id: string; enabled: boolean }; result: McpSourceDescriptor[] };

  /**
   * Pre-install confirmation payload: the complete untruncated command that
   * will run, the binary it resolves to, the runtime alternatives, and every
   * input the form must collect with full `server.json` semantics.
   */
  mcp_market_install_plan: {
    args: { id: string; runtimeId?: string };
    result: McpInstallPlan;
  };

  /**
   * The same derivation with the user's answers folded in: the entry that
   * would be written and the command line that would run, plus the required
   * inputs still blank.
   *
   * Cheap by construction (no `PATH` walk, no filesystem), so the wizard calls
   * it as the form is filled. **Never cache the result**: the answers carry the
   * user's secrets, and a cache key holding a secret is a secret at rest.
   */
  mcp_market_install_preview: {
    args: { id: string; runtimeId?: string; answers: McpInstallAnswer[] };
    result: McpInstallPreview;
  };

  /**
   * Commit the install. The backend re-derives the entry from these answers
   * against the catalog row *as it stands now*, and refuses unless that still
   * produces `approvedTarget` — `McpInstallPreview.approvalTarget` exactly as
   * the preview handed it over, **unmasked**. It covers everything the
   * confirmation step showed: the command line (or the resolved url for a
   * remote shape), the environment, the headers and the config key. Never
   * rebuild it here — deriving it a second time at the edge is what let a
   * registry sync slip an unseen `HTTP_PROXY` past the check.
   *
   * A refusal comes back as `{ status: "rejected" }` rather than a thrown
   * error, because its two reasons — a required input still blank, and a row
   * that changed under the user — have to be told apart, and an error is only
   * a string on this wire.
   */
  mcp_market_install: {
    args: {
      id: string;
      runtimeId?: string;
      answers: McpInstallAnswer[];
      enabled: Record<string, boolean>;
      approvedTarget: string;
    };
    result: McpInstallOutcome;
  };
}
