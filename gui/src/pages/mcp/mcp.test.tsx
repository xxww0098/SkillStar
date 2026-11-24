import { expect, open, test } from "nsh/test";
import { WINDOW_H, WINDOW_W } from "../../layout";

const HOST = "./mcp.test.host.tsx";

test("managed pane keeps library on screen, adopts found entry, toggles agents in detail", async () => {
  const app = await open(HOST, { width: WINDOW_W, height: WINDOW_H });
  await app.waitUntil(() => app.shows("id:mcp-card-filesystem"));

  // The detail column lands on a server instead of leaving a dead pane.
  expect(app.shows("id:mcp-detail-drawer")).toBe(true);

  // Discovered entries sit behind a one-line disclosure: collapsed by default
  // once there are many, and the rows come back on demand.
  expect(app.shows("id:mcp-found-codex-legacy-git")).toBe(true);
  await app.press("mcp-found-toggle");
  expect(app.shows("id:mcp-adopt-codex-legacy-git")).toBe(false);
  await app.press("mcp-found-toggle");
  await app.waitUntil(() => app.shows("id:mcp-adopt-codex-legacy-git"));

  await app.press("mcp-adopt-codex-legacy-git");
  await app.waitUntil(() => app.shows("id:mcp-card-legacy-git"));

  // Per-agent switches live in the detail column now, not in the row.
  await app.press("mcp-card-legacy-git");
  await app.waitUntil(() => app.shows("id:mcp-drawer-agent-claude"));
  await app.press("mcp-drawer-agent-claude");

  // Sort Z→A / A→Z and toggle select mode on the pinned library bar.
  await app.press("mcp-sort-toggle");
  expect(app.texts()).toContain("Z→A");
  await app.press("mcp-sort-toggle");
  expect(app.texts()).toContain("A→Z");
  await app.press("mcp-select-toggle");
  await app.waitUntil(() => app.shows("id:mcp-batch-bar"));
  await app.press("mcp-select-toggle");

  // Probe from the detail column and disable `write_file`.
  await app.press("mcp-card-filesystem");
  await app.waitUntil(() => app.shows("id:mcp-detail-drawer"));
  await app.press("mcp-btn-probe");
  await app.waitUntil(() => app.shows("id:mcp-tool-row-write_file"));
  await app.press("mcp-tool-row-write_file");
  expect(app.shows("id:mcp-tool-row-read_file")).toBe(true);
});

test("agent grid narrows to installed agents and can flip the scope", async () => {
  const app = await open(HOST, { width: WINDOW_W, height: WINDOW_H });
  await app.waitUntil(() => app.shows("id:mcp-card-filesystem"));

  await app.press("mcp-card-filesystem");
  expect(app.shows("id:mcp-drawer-agent-antigravity")).toBe(false);

  await app.press("mcp-agent-scope-all");
  await app.waitUntil(() => app.shows("id:mcp-drawer-agent-antigravity"));
  await app.press("mcp-agent-all-on");
  await app.press("mcp-agent-scope-installed");
  // Antigravity now receives the server, so it stays in the installed scope.
  expect(app.shows("id:mcp-drawer-agent-antigravity")).toBe(true);

  // Closing the detail column sticks: a snapshot refresh does not reopen it.
  await app.press("mcp-detail-close");
  expect(app.shows("id:mcp-detail-drawer")).toBe(false);
  await app.press("mcp-pause-filesystem");
  expect(app.shows("id:mcp-detail-drawer")).toBe(false);
});

test("builtin store searches and installs server with required input", async () => {
  const app = await open(HOST, { width: WINDOW_W, height: WINDOW_H });
  await app.waitUntil(() => app.shows("id:mcp-tab-store"));

  await app.press("mcp-tab-store");
  await app.waitUntil(() => app.shows("id:mcp-store-card-playwright"));
  expect(app.shows("id:mcp-store-card-brave-search")).toBe(true);

  await app.press("mcp-store-install-brave-search");
  await app.waitUntil(() => app.shows("id:mcp-store-input-BRAVE_API_KEY"));
  await app.type("id:mcp-store-input-BRAVE_API_KEY", "BSA_secret_key");
  await app.press("id:mcp-store-install-confirm");

  await app.press("mcp-tab-managed");
  await app.waitUntil(() => app.shows("id:mcp-card-brave-search"));
});

test("paste modal parses CLI command and project snippets tab previews formats", async () => {
  const app = await open(HOST, { width: WINDOW_W, height: WINDOW_H });
  await app.waitUntil(() => app.shows("id:mcp-btn-paste"));

  await app.press("mcp-btn-paste");
  await app.waitUntil(() => app.shows("id:mcp-paste-input"));
  await app.type("id:mcp-paste-input", "claude mcp add -t http linear https://mcp.linear.app/mcp");
  await app.press("mcp-paste-submit");
  await app.waitUntil(() => app.shows("id:mcp-editor-save"));
  await app.press("id:mcp-editor-save");
  await app.waitUntil(() => app.shows("id:mcp-card-linear"));

  await app.press("mcp-tab-project");
  await app.waitUntil(() => app.shows("id:mcp-project-view"));
  expect(app.texts()).toContain(".mcp.json");
  await app.press("mcp-project-agent-codex");
  expect(app.texts()).toContain(".codex/config.toml");
});

test("store filter chips and online registry toggle filter catalog", async () => {
  const app = await open(HOST, { width: WINDOW_W, height: WINDOW_H });
  await app.waitUntil(() => app.shows("id:mcp-tab-store"));

  await app.press("mcp-tab-store");
  await app.waitUntil(() => app.shows("id:mcp-store-card-playwright"));
  expect(app.shows("id:mcp-store-card-brave-search")).toBe(true);
  expect(app.shows("id:mcp-store-card-sentry")).toBe(true);
  expect(app.shows("id:mcp-store-card-datadog-online")).toBe(false);

  // Featured chip hides non-featured remote entry.
  await app.press("mcp-store-filter-featured");
  await app.waitUntil(() => !app.shows("id:mcp-store-card-sentry"));
  expect(app.shows("id:mcp-store-card-playwright")).toBe(true);
  expect(app.shows("id:mcp-store-card-brave-search")).toBe(true);

  // Stdio chip keeps stdio entries and excludes remote entry.
  await app.press("mcp-store-filter-stdio");
  expect(app.shows("id:mcp-store-card-playwright")).toBe(true);
  expect(app.shows("id:mcp-store-card-sentry")).toBe(false);

  // Remote chip shows only http/sse entries.
  await app.press("mcp-store-filter-remote");
  await app.waitUntil(() => app.shows("id:mcp-store-card-sentry"));
  expect(app.shows("id:mcp-store-card-playwright")).toBe(false);
  expect(app.shows("id:mcp-store-card-brave-search")).toBe(false);

  // All chip restores offline catalog.
  await app.press("mcp-store-filter-all");
  await app.waitUntil(() => app.shows("id:mcp-store-card-playwright"));
  expect(app.shows("id:mcp-store-card-sentry")).toBe(true);

  // Online toggle queries registry and exposes online-only remote entry.
  await app.press("mcp-store-online-toggle");
  await app.waitUntil(() => app.shows("id:mcp-store-card-datadog-online"));

  await app.press("mcp-store-filter-remote");
  await app.waitUntil(() => app.shows("id:mcp-store-card-datadog-online"));
  expect(app.shows("id:mcp-store-card-sentry")).toBe(true);
  expect(app.shows("id:mcp-store-card-playwright")).toBe(false);

  await app.press("mcp-store-online-toggle");
  await app.waitUntil(() => !app.shows("id:mcp-store-card-datadog-online"));
  expect(app.shows("id:mcp-store-card-sentry")).toBe(true);
});

test("detail copy, editor startupTimeoutSec round-trip, and modal dismissal", async () => {
  const app = await open(HOST, { width: WINDOW_W, height: WINDOW_H });
  await app.waitUntil(() => app.shows("id:mcp-card-filesystem"));
  await app.waitUntil(() => app.shows("id:mcp-detail-drawer"));

  // Copy command button writes endpoint to clipboard and flips button label.
  await app.press("mcp-detail-copy");
  await app.waitUntil(
    () => app.clipboard() === "npx -y @modelcontextprotocol/server-filesystem /tmp",
  );
  await app.waitUntil(() => app.texts().includes("已复制"));

  // Close editor modal via close button.
  await app.press("mcp-detail-edit");
  await app.waitUntil(() => app.shows("id:mcp-editor-timeout"));
  await app.press("mcp-editor-close");
  await app.waitUntil(() => !app.shows("id:mcp-editor-save"));

  // Close Overlay modal via Escape (onCancel).
  await app.press("mcp-detail-edit");
  await app.waitUntil(() => app.shows("id:mcp-editor-timeout"));
  await app.keyboard.press("escape");
  await app.waitUntil(() => !app.shows("id:mcp-editor-save"));

  // Save startupTimeoutSec and verify round-trip in detail drawer and reopened editor.
  await app.press("mcp-detail-edit");
  await app.waitUntil(() => app.shows("id:mcp-editor-timeout"));
  await app.type("id:mcp-editor-timeout", "45");
  await app.press("mcp-editor-save");
  await app.waitUntil(() => !app.shows("id:mcp-editor-save"));
  await app.waitUntil(() => app.texts().includes("timeout 45s"));

  await app.press("mcp-detail-edit");
  await app.waitUntil(() => app.shows("id:mcp-editor-timeout"));
  expect(app.value("id:mcp-editor-timeout")).toBe("45");
});

test("project tab hides PageBar search input and copies project snippet", async () => {
  const app = await open(HOST, { width: WINDOW_W, height: WINDOW_H });
  await app.waitUntil(() => app.shows("id:mcp-search-input"));
  expect(app.shows("id:mcp-search-input")).toBe(true);

  await app.press("mcp-tab-project");
  await app.waitUntil(() => app.shows("id:mcp-project-view"));
  expect(app.shows("id:mcp-search-input")).toBe(false);
  expect(app.shows("id:mcp-store-search-input")).toBe(false);

  await app.press("mcp-project-copy");
  await app.waitUntil(
    () =>
      app.clipboard() ===
      JSON.stringify({ mcpServers: { filesystem: { command: "npx" } } }, null, 2),
  );
  await app.waitUntil(() => app.texts().includes("已复制"));

  await app.press("mcp-project-agent-codex");
  await app.waitUntil(() => app.texts().includes("复制片段"));
  await app.press("mcp-project-copy");
  await app.waitUntil(() => app.clipboard() === '[mcp_servers.filesystem]\ncommand = "npx"\n');

  await app.press("mcp-tab-managed");
  await app.waitUntil(() => app.shows("id:mcp-search-input"));
});

test("editor validates bad env/timeout/quotes and preserves quoted args with spaces", async () => {
  const app = await open(HOST, { width: WINDOW_W, height: WINDOW_H });
  await app.waitUntil(() => app.shows("id:mcp-card-filesystem"));

  await app.press("mcp-detail-edit");
  await app.waitUntil(() => app.shows("id:mcp-editor-args"));

  // Unterminated quote blocks save
  await app.clear("id:mcp-editor-args");
  await app.type("id:mcp-editor-args", '"/tmp/my dir');
  await app.press("mcp-editor-save");
  await app.waitUntil(() => app.shows("id:mcp-editor-error"));
  expect(app.shows("id:mcp-editor-save")).toBe(true);

  // Valid quoted arg + bad env line blocks save
  await app.clear("id:mcp-editor-args");
  await app.type("id:mcp-editor-args", '"/tmp/my dir" --flag');
  await app.clear("id:mcp-editor-env");
  await app.type("id:mcp-editor-env", "BAD_ENV_LINE");
  await app.press("mcp-editor-save");
  await app.waitUntil(() => app.shows("id:mcp-editor-error"));

  // Valid env + invalid timeout (-5) blocks save
  await app.clear("id:mcp-editor-env");
  await app.type("id:mcp-editor-env", "FOO=bar");
  await app.clear("id:mcp-editor-timeout");
  await app.type("id:mcp-editor-timeout", "-5");
  await app.press("mcp-editor-save");
  await app.waitUntil(() => app.shows("id:mcp-editor-error"));

  // Valid timeout saves cleanly and keeps quoted arg intact on reopen
  await app.clear("id:mcp-editor-timeout");
  await app.type("id:mcp-editor-timeout", "30");
  await app.press("mcp-editor-save");
  await app.waitUntil(() => !app.shows("id:mcp-editor-save"));

  await app.press("mcp-detail-edit");
  await app.waitUntil(() => app.shows("id:mcp-editor-args"));
  expect(app.value("id:mcp-editor-args")).toBe('"/tmp/my dir" --flag');
});

test("store blocks empty required input and avoids cloning already-installed 0-input item", async () => {
  const app = await open(HOST, { width: WINDOW_W, height: WINDOW_H });
  await app.waitUntil(() => app.shows("id:mcp-tab-store"));

  await app.press("mcp-tab-store");
  await app.waitUntil(() => app.shows("id:mcp-store-install-brave-search"));

  // Empty required input must stay in modal and display inline error
  await app.press("mcp-store-install-brave-search");
  await app.waitUntil(() => app.shows("id:mcp-store-input-BRAVE_API_KEY"));
  await app.press("mcp-store-install-confirm");
  await app.waitUntil(() => app.shows("id:mcp-store-install-error"));
  await app.press("mcp-store-install-close");
  await app.waitUntil(() => !app.shows("id:mcp-store-install-confirm"));

  // Install 0-input item once -> installed marker appears; clicking again opens existing editor instead of cloning `-2`
  await app.press("mcp-store-install-playwright");
  await app.waitUntil(() => app.shows("id:mcp-store-clone-playwright"));
  await app.press("mcp-store-install-playwright");
  await app.waitUntil(() => app.shows("id:mcp-editor-save"));
  expect(app.value("id:mcp-editor-name")).toBe("playwright");
  await app.press("mcp-editor-close");
  await app.waitUntil(() => !app.shows("id:mcp-editor-save"));
});

test("conflict dialog, batch delete confirmation, backups modal, unsupported tools notice, and project tab refresh", async () => {
  const app = await open(HOST, { width: WINDOW_W, height: WINDOW_H });
  await app.waitUntil(() => app.shows("id:mcp-card-filesystem"));

  // Same-name different-config found entry opens conflict modal instead of hardcoding keep
  await app.press("mcp-adopt-codex-filesystem");
  await app.waitUntil(() => app.shows("id:mcp-import-conflict-modal"));
  await app.press("mcp-conflict-replace");
  await app.waitUntil(() => !app.shows("id:mcp-import-conflict-modal"));

  // Enable antigravity (supportsToolsOff: false) and probe -> shows unsupported tools notice
  await app.press("mcp-agent-scope-all");
  await app.waitUntil(() => app.shows("id:mcp-drawer-agent-antigravity"));
  await app.press("mcp-drawer-agent-antigravity");
  await app.press("mcp-btn-probe");
  await app.waitUntil(() => app.shows("id:mcp-detail-tools-unsupported"));

  // Backups modal opens from PageBar and restore prompts confirmation
  await app.press("mcp-btn-backups");
  await app.waitUntil(() => app.shows("id:mcp-backups-modal"));
  await app.press("mcp-backup-restore-20261120-0001-.claude.json.bak");
  await app.waitUntil(() => app.shows("id:dialog-confirm-ok"));
  await app.press("dialog-confirm-ok");
  await app.press("mcp-backups-close");
  await app.waitUntil(() => !app.shows("id:mcp-backups-modal"));

  // Staying on project tab and adding a server keeps tab and refreshes snippets
  await app.press("mcp-tab-project");
  await app.waitUntil(() => app.shows("id:mcp-project-view"));
  await app.press("mcp-btn-new");
  await app.waitUntil(() => app.shows("id:mcp-editor-name"));
  await app.type("id:mcp-editor-name", "proj-extra");
  await app.type("id:mcp-editor-command", "uvx");
  await app.press("mcp-editor-save");
  await app.waitUntil(() => !app.shows("id:mcp-editor-save"));
  await app.waitUntil(() => app.shows("id:mcp-project-view"));
  await app.press("mcp-project-copy");
  await app.waitUntil(() => (app.clipboard() ?? "").includes("proj-extra"));

  // Batch delete requires confirmation before removing
  await app.press("mcp-tab-managed");
  await app.press("mcp-select-toggle");
  await app.waitUntil(() => app.shows("id:mcp-batch-bar"));
  await app.press("mcp-batch-select-all");
  await app.press("mcp-batch-delete");
  await app.waitUntil(() => app.shows("id:dialog-confirm-cancel"));
  await app.press("dialog-confirm-cancel");
  expect(app.shows("id:mcp-card-filesystem")).toBe(true);
});
