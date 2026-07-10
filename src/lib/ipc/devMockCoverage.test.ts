/**
 * Coverage guard for the browser-dev IPC mock (see ./devMock.ts).
 *
 * Command names are declared as *TypeScript types only* (`interface FooCommands
 * { command_name: { args: ...; result: ... } }` in ./commands/*.ts) — they do
 * not exist as runtime values, so nothing at runtime can enumerate them. This
 * test instead reads the source files as text with node:fs and extracts
 * command names with a small brace-depth state machine (see
 * `extractDeclaredCommands` / `extractHandlerKeys` below). That makes the
 * test resilient to reordering/reformatting, but it does mean: if you
 * restructure a `*Commands` interface or the `HANDLERS` object in a way that
 * breaks the "2-space-indented `key: ...` at brace-depth 1" shape these
 * functions look for, update the extractors together with this test.
 *
 * Ratchet model (same spirit as scripts/internal/check_*.sh): as of writing,
 * src/lib/ipc/commands/*.ts declares more commands than devMock.ts's HANDLERS
 * table actually mocks. Known gaps are grandfathered in KNOWN_MISSING_MOCKS
 * below so this test passes today. Any NEWLY declared command that is not
 * mocked AND not in that allowlist fails the test by name — so the gap can
 * only shrink. When you add a mock for one of these, delete its entry from
 * KNOWN_MISSING_MOCKS (a second test below fails loudly if you forget and the
 * mock already covers it, so the list cannot go stale unnoticed).
 */
/// <reference types="node" />
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const COMMANDS_DIR = path.join(__dirname, "commands");
const DEV_MOCK_PATH = path.join(__dirname, "devMock.ts");

/**
 * Extract command names declared inside `export interface *Commands { ... }`
 * blocks across every file in src/lib/ipc/commands/. Uses a brace-depth state
 * machine so that sibling DTO interfaces in the same file (e.g. `SshHost`,
 * indented identically to the commands themselves) are never mistaken for
 * command names — only identifier keys at depth 1 *inside* a `*Commands`
 * block count.
 */
function extractDeclaredCommands(dir: string): Set<string> {
  const names = new Set<string>();
  const files = fs.readdirSync(dir).filter((f) => f.endsWith(".ts"));

  for (const file of files) {
    const text = fs.readFileSync(path.join(dir, file), "utf8");
    const lines = text.split("\n");
    let inBlock = false;
    let depth = 0;

    for (const line of lines) {
      if (!inBlock) {
        if (/^export interface \w+Commands\b/.test(line)) {
          inBlock = true;
          depth = 0;
        } else {
          continue;
        }
      }

      if (depth === 1) {
        const m = line.match(/^\s{2}([a-zA-Z_][a-zA-Z0-9_]*)\s*:/);
        if (m && !/^\s*\/\//.test(line)) {
          names.add(m[1]);
        }
      }

      const opens = (line.match(/\{/g) || []).length;
      const closes = (line.match(/\}/g) || []).length;
      depth += opens - closes;
      if (depth <= 0) {
        inBlock = false;
      }
    }
  }

  return names;
}

/**
 * Extract the keys of the `HANDLERS` lookup table in devMock.ts, using the
 * same brace-depth approach (only depth-1 keys inside the `const HANDLERS =
 * { ... }` block count, so nested return-value object literals never leak
 * their own keys into the result).
 */
function extractHandlerKeys(devMockPath: string): Set<string> {
  const text = fs.readFileSync(devMockPath, "utf8");
  const lines = text.split("\n");
  const names = new Set<string>();
  let inBlock = false;
  let depth = 0;

  for (const line of lines) {
    if (!inBlock) {
      if (/^const HANDLERS\s*:/.test(line)) {
        inBlock = true;
        depth = 0;
      } else {
        continue;
      }
    }

    if (depth === 1) {
      const m = line.match(/^\s{2}([a-zA-Z_][a-zA-Z0-9_]*)\s*:/);
      if (m && !/^\s*\/\//.test(line)) {
        names.add(m[1]);
      }
    }

    const opens = (line.match(/\{/g) || []).length;
    const closes = (line.match(/\}/g) || []).length;
    depth += opens - closes;
    if (depth <= 0 && inBlock) {
      break;
    }
  }

  return names;
}

/**
 * Commands declared in src/lib/ipc/commands/*.ts that devMock.ts's HANDLERS
 * table does not yet mock (as of the scan that generated this list — see the
 * class doc comment above). Unmocked commands still work in browser dev: they
 * fall through devInvoke's "unknown command" branch and resolve to `[]` (see
 * devMock.ts), so list-shaped reads render empty instead of crashing; only
 * object-shaped reads or mutations need an entry here removed by adding a
 * real mock.
 *
 * To fix one: add a handler for it in devMock.ts's HANDLERS table, then
 * delete its line here. Do NOT add new entries here for commands that don't
 * exist yet — this list only grandfathers a fixed, shrinking set of known
 * gaps; it is not a general-purpose exemption mechanism.
 */
const KNOWN_MISSING_MOCKS = new Set([
  "add_custom_agent_profile",
  "adopt_local_folder",
  "ai_batch_process_skills",
  "ai_extract_search_keywords",
  "ai_pick_skills",
  "ai_search_marketplace_local",
  "ai_search_with_keywords",
  "ai_summarize_skill",
  "ai_summarize_skill_stream",
  "ai_translate_skill",
  "ai_translate_skill_stream",
  "app_quit",
  "batch_link_skills_to_agent",
  "batch_remove_skills_from_all_agents",
  "clean_broken_skills",
  "clean_repo_cache",
  "clear_all_caches",
  "create_local_skill",
  "create_local_skill_from_content",
  "create_mcp_server",
  "create_project_skills",
  "create_skill_group",
  "delete_local_skill",
  "delete_mcp_server",
  "delete_skill_group",
  "deploy_skill_group",
  "dismiss_new_skill",
  "dismiss_new_skills_batch",
  "download_and_install_update",
  "duplicate_skill_group",
  "export_multi_skill_bundle",
  "export_skill_bundle",
  "force_delete_app_config",
  "force_delete_installed_skills",
  "force_delete_repo_caches",
  "get_marketplace_skill_details",
  "get_official_publishers",
  "get_publisher_repo_skills",
  "get_publisher_repos",
  "get_publisher_repos_local",
  "get_repo_skills_local",
  "get_skill_detail_local",
  "get_skills_sh_leaderboard",
  "import_mcp_from_tool",
  "import_multi_skill_bundle",
  "import_project_skills",
  "import_skill_bundle",
  "install_from_scan",
  "install_from_share_code",
  "install_skill",
  "list_linked_skills",
  "list_mcp_publishers_local",
  "list_mcp_servers_by_publisher_local",
  "migrate_local_skills",
  "open_external_url",
  "open_folder",
  "preview_multi_skill_bundle",
  "preview_skill_bundle",
  "read_codex_env_from_zshrc",
  "read_text_file",
  "refresh_stale_project_copies",
  "register_project",
  "remove_custom_agent_profile",
  "remove_project",
  "reorder_mcp_servers",
  "resolve_skill_sources",
  "restart_after_update",
  "resync_tool",
  "save_acp_config",
  "save_ai_config",
  "save_and_sync_project",
  "save_github_mirror_config",
  "save_project_skills_list",
  "save_proxy_config",
  "scan_github_repo",
  "search_skills_sh",
  "set_dock_visible",
  "set_mcp_tool_enabled",
  "start_patrol",
  "stop_patrol",
  "sync_all_mcp",
  "sync_marketplace_scope",
  "sync_mcp_server",
  "test_github_mirror",
  "test_provider_latency",
  "toggle_skill_for_agent",
  "uninstall_skill",
  "unlink_all_skills_from_agent",
  "unlink_skill_from_agent",
  "update_mcp_server",
  "update_project_path",
  "update_skill",
  "update_skill_content",
  "update_skill_group",
  "update_tray_language",
  "write_codex_env_to_zshrc",
  "write_text_file",
]);

describe("devMock coverage", () => {
  it("mocks every declared command not explicitly grandfathered", () => {
    const declared = extractDeclaredCommands(COMMANDS_DIR);
    const handled = extractHandlerKeys(DEV_MOCK_PATH);

    // Sanity check: the extractors should not silently return nothing (that
    // would make this whole test vacuously pass).
    expect(declared.size).toBeGreaterThan(100);
    expect(handled.size).toBeGreaterThan(50);

    const uncovered = [...declared].filter((cmd) => !handled.has(cmd) && !KNOWN_MISSING_MOCKS.has(cmd));

    expect(
      uncovered,
      uncovered.length > 0
        ? `${uncovered.length} command(s) declared in src/lib/ipc/commands/*.ts have no devMock.ts handler and are not in KNOWN_MISSING_MOCKS: ${uncovered.sort().join(", ")}. Add a mock in devMock.ts's HANDLERS table, or if this is intentional debt, add the name to KNOWN_MISSING_MOCKS in this file.`
        : "",
    ).toEqual([]);
  });

  it("keeps KNOWN_MISSING_MOCKS free of stale entries", () => {
    const declared = extractDeclaredCommands(COMMANDS_DIR);
    const handled = extractHandlerKeys(DEV_MOCK_PATH);

    // An entry is stale if either: (a) it is already mocked (someone added a
    // handler and forgot to clean up the allowlist), or (b) it no longer
    // corresponds to any declared command (the command was renamed/removed).
    const alreadyMocked = [...KNOWN_MISSING_MOCKS].filter((cmd) => handled.has(cmd));
    const noLongerDeclared = [...KNOWN_MISSING_MOCKS].filter((cmd) => !declared.has(cmd));

    expect(
      alreadyMocked,
      alreadyMocked.length > 0
        ? `${alreadyMocked.length} command(s) in KNOWN_MISSING_MOCKS are now mocked in devMock.ts — remove from the allowlist: ${alreadyMocked.sort().join(", ")}.`
        : "",
    ).toEqual([]);

    expect(
      noLongerDeclared,
      noLongerDeclared.length > 0
        ? `${noLongerDeclared.length} command(s) in KNOWN_MISSING_MOCKS no longer exist in src/lib/ipc/commands/*.ts — remove from the allowlist: ${noLongerDeclared.sort().join(", ")}.`
        : "",
    ).toEqual([]);
  });
});
