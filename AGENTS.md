# 和我讲中文

# SkillStar — Code Framework

## Architecture

SkillStar is a Tauri v2 desktop app with a React SPA frontend and Rust backend.

- Frontend calls backend via `invoke()` and Tauri events.
- Tauri commands are defined in `src-tauri/src/commands/mod.rs` (root handlers) with feature modules under `src-tauri/src/commands/`.
- Core domain logic lives in workspace crates under `crates/`; `src-tauri/src/core/` retains thin adapter stubs and Tauri-specific glue.
- Persistence is mixed storage under `~/.skillstar/`: JSON for config/project metadata plus SQLite for marketplace cache.
- Skill distribution is symlink-based to keep project directories clean.

> Frontend-specific conventions live in [AGENTS-UI.md](./AGENTS-UI.md).

## Key Paths


| Purpose               | Path                       |
| --------------------- | -------------------------- |
| Data root             | `~/.skillstar/`            |
| Config files          | `~/.skillstar/config/`     |
| SQLite databases      | `~/.skillstar/db/`         |
| Runtime logs          | `~/.skillstar/logs/`       |
| Runtime state         | `~/.skillstar/state/`      |
| Hub root              | `~/.skillstar/hub/`        |
| Hub skills            | `~/.skillstar/hub/skills/` |
| Local authored skills | `~/.skillstar/hub/local/`  |
| Repo cache            | `~/.skillstar/hub/repos/`  |
| SSH host config       | `~/.skillstar/config/ssh_hosts.toml` |
| SSH accepted host keys| `~/.skillstar/config/ssh_known_hosts.json` |


## Project Structure (Condensed)

```text
SkillStar/
├── src/                           # React app
│   ├── features/                  # domain slices (components + hooks)
│   │   ├── my-skills/             # SkillShell (unified UI), backends/ (local + remote SkillsBackend), cards, modals, install/export
│   │   ├── marketplace/           # marketplace browsing
│   │   ├── projects/              # project registration + agent config
│   │   └── settings/              # app settings sections
│   ├── hooks/                     # global hooks (useNavigation, useUpdater, useAiConfig)
│   ├── pages/                     # thin route-level shells
│   ├── components/                # ui/, layout/, shared/
│   ├── lib/                       # shared utilities
│   └── types/                     # shared TS types
├── src-tauri/
│   ├── src/
│   │   ├── commands/              # mod.rs: skills, bundles, shell, network; + marketplace, agents, projects, github, patrol, acp
│   │   │   └── ai/               # AI commands: summarize, skill pick
│   │   │   └── models_commands.rs # provider CRUD / health dashboard (split from models.rs)
│   │   │   └── oauth_commands.rs  # Codex/Gemini OAuth + account management
│   │   │   └── quota_commands.rs  # quota refresh / usage / speedtest
│   │   │   └── ssh_hosts.rs       # SSH remote host CRUD + connection test + remote skill push/list/delete
│   │   └── core/                  # Tauri-specific glue only (state handles, event emitters, window-bound wrappers)
│   │       ├── skills/            # thin adapters over skillstar-skills (install, update, bundle, local, group, discover)
│   │       ├── marketplace_snapshot/ # local-first marketplace DB (wraps Tauri State)
│   │       ├── marketplace.rs     # marketplace command helpers
│   │       ├── acp_client.rs      # ACP client (Agent Client Protocol)
│   │       ├── app_shell.rs       # app shell utilities
│   │       ├── lockfile.rs        # lockfile management
│   │       ├── path_env.rs        # PATH environment helpers
│   │       ├── patrol.rs          # patrol glue (event emitters)
│   │       ├── skill.rs           # skill type adapters
│   │       └── update_checker.rs  # update checker glue
│   ├── Cargo.toml
│   └── tauri.conf.json
├── crates/                        # workspace crates (domain logic)
│   ├── skillstar-core/            # shared types + infra (paths, fs_ops, db_pool, migration, error, util) + user config (proxy, github_mirror, ACP)
│   ├── skillstar-skills/          # skill lifecycle (install, update, bundle, local, repo_scanner, discovery) + git operations
│   ├── skillstar-marketplace/     # marketplace snapshot + FTS
│   ├── skillstar-models/          # model provider configuration: providers store + tool sync (Claude Code / Codex / OpenCode) + on-disk config I/O + latency + circuit breaker
│   ├── skillstar-ai/              # AI inference: chat completion (OpenAI / Anthropic / local), summarization, skill pick, scan params
│   ├── skillstar-projects/        # project management + agent profiles + patrol + terminal (Launch Deck)
│   ├── skillstar-ssh/             # SSH remote skill management: russh connect + SFTP push/list/delete + host config + keyring credentials
│   └── skillstar-app/             # Tauri-agnostic command helpers (shell, network, marketplace, ACP) + CLI entry point
├── docs/
│   ├── Error.md
│   ├── CHANGELOG.md
│   └── impeccable.md       # brand & visual baseline
├── scripts/
│   ├── release/
│   └── internal/            # maintenance-only scripts
├── AGENTS.md
└── AGENTS-UI.md
```

## Frontend Dependencies


| Package                  | Version | Purpose                      |
| ------------------------ | ------- | ---------------------------- |
| `react` / `react-dom`    | 19.x    | UI runtime                   |
| `vite`                   | 5.x     | Build tool                   |
| `tailwindcss`            | 4.x     | Styling                      |
| `@tauri-apps/api`        | 2.x     | IPC bridge                   |
| `framer-motion`          | 12.x    | Motion                       |
| `lucide-react`           | 0.436   | Icons                        |
| `sonner`                 | 2.x     | Toasts                       |
| `react-markdown`         | 10.x    | Markdown render              |
| `@radix-ui/*`            | latest  | Accessible primitives        |
| `@biomejs/biome`         | 2.x     | Lint + format (dev)          |
| `vitest`                 | 3.x     | Unit/component testing (dev) |
| `@testing-library/react` | 16.x    | React test utilities (dev)   |


## Backend Dependencies (Rust)


| Crate                   | Version   | Purpose                         |
| ----------------------- | --------- | ------------------------------- |
| `tauri`                 | 2         | Desktop framework               |
| `tokio`                 | 1         | Async runtime                   |
| `reqwest`               | 0.13      | HTTP                            |
| `gix`                   | 0.80      | Git operations                  |
| `serde` / `serde_json`  | 1         | Serialization                   |
| `anyhow`                | 1         | Error handling                  |
| `chrono`                | 0.4       | Time                            |
| `clap`                  | 4         | CLI parsing                     |
| `regex`                 | 1.12      | Pattern matching                |
| `toml`                  | 1.1       | TOML parsing                    |
| `flate2` + `tar`        | 1.1 / 0.4 | Bundle packing                  |
| `html2md`               | 0.2       | Marketplace detail conversion   |
| `sys-locale`            | 0.3       | Locale detection                |
| `agent-client-protocol` | 0.10      | ACP (Agent Client Protocol) SDK |
| `async-trait`           | 0.1       | Async trait support (for ACP)   |
| `tokio-util`            | 0.7       | Compat adapters (ACP stdio)     |
| `futures`               | 0.3       | Async streams                   |
| `skillstar-core`        | —         | Shared types + infra + user config |
| `skillstar-skills`      | —         | Skill lifecycle + git operations |
| `skillstar-marketplace` | —         | Marketplace snapshot + FTS      |
| `skillstar-models`      | —         | Model provider config + tool sync + latency + circuit breaker |
| `skillstar-ai`          | —         | AI inference (chat completion, summarize, skill pick) |
| `skillstar-projects`    | —         | Projects + patrol + terminal    |
| `skillstar-app`         | —         | Command helpers + CLI           |


## Backend Behavior Rules

### Skills and Sync

- Keep heavy logic in `core/*` or workspace crates, not in command wrappers.
- `src-tauri/src/core/skills/` is a thin adapter layer over `skillstar-skills`; domain logic lives in the crate.
- Installed-skill list should render fast from local snapshot first.
- Remote update checks run in bounded background work.
- Project sync is reconciliation: add selected skills per agent, remove stale entries, prune empty agent folders; zero-skill agent selections must be dropped instead of creating empty project folders or persisting as active project agents; deployment always tries symlink first — if symlink creation fails (e.g. Windows without Developer Mode), falls back to full directory copy automatically. The `deploy_modes` field in `skills-list.json` is retained for backward-compat but ignored.
- Global agent linking (`toggle_skill_for_agent`, `batch_link_skills_to_agent`) uses the same symlink → junction → copy ladder as project deploys; batch linking processes every skill and reports accumulated failures instead of aborting on the first one.
- `resync_existing_links` (after skill update) refreshes BOTH link and copy deployments via a staged swap: the fresh deploy is created under a staging name first, so a failed re-create never destroys the user's existing link. Per-agent failures are collected into `SkillUpdateOutcome.agent_link_failures` and surfaced as a UI warning toast — never silently swallowed.
- When a project is selected, `refresh_stale_project_copies` compares SHA-256 content hashes of copy-deployed skill directories against their hub sources; stale copies are re-deployed while intentionally deleted skills are not restored.
- Windows/global agent unlink must attempt `remove_link_or_copy` for any existing entry (link/junction/copy), and only treat missing targets as no-op.
- Repo scan/import defaults to **root-first**: when repo root has a valid `SKILL.md`, treat the root as the primary single skill by default.
- Repo scan/import may optionally use **full-depth** discovery to include nested `SKILL.md` skills in the same repository.
- Imported skill identity should prefer frontmatter `name`; fall back to directory name (and for root-level fallback use repo name when needed).
- CLI install mode is split: `skillstar install <url>` defaults to **project-level** linking for the current working directory, while `skillstar install --global <url>` performs **hub-only** installation.
- Project-level CLI install should prefer linking into already-detected agent project folders and fall back to `.agent/skills` when no project-level agent path exists yet.
- CLI install should accept explicit project targets via `--agent <id>` (repeatable or comma-separated), and when omitted in an interactive terminal should allow user input before falling back to auto-detect.
- CLI `install` / `add` accepts repo URL, `owner/repo` shorthand, local `.ags`/`.agd` bundle paths, and local directories containing `SKILL.md`; directory inputs are adopted as local-authored skills via `local_skill::create`.
- CLI `install` flags mirror the `npx skills add` baseline: `--list` scans without mutating, `--all` installs every discovered skill, `--yes` skips interactive prompts, `--copy` signals a preference for copy deployment (project sync still auto-falls-back to copy when symlinks fail).
- CLI `find [query]` / `search [query]` routes through `skillstar_marketplace::snapshot::search_local` so the local FTS snapshot powers CLI discovery; `--json` emits the raw `LocalFirstResult` for scripting.
- CLI `remove` / `rm` / `uninstall` uninstalls by name, accepts `--all` and `--yes`; it is the same codepath as the Tauri `uninstall_skill` command.
- CLI `init [name]` replaces the hard-coded `create`; the old name is kept as an alias.
- CLI install output annotates each project-level link with its actual deployed kind (`link`, `copy`, `broken-link`, `missing`) so users can see when a symlink silently degraded to a copy.

### Project Registration and Detection

- Adding a new Agent CLI: see the dedicated guide [ADDING-AN-AGENT.md](./ADDING-AN-AGENT.md) (builtin data table + icon is the whole core path; tool-sync and usage are independent optional axes).
- Project registration is explicit before scan/import/sync.
- `detect_project_agents` is data-driven by `builtin_profiles()`.
- Install detection strategy is driven by the builtin table's `binary` column: CLI agents (`claude`/`codex`/`gemini`/`zcode`/`opencode`) are installed iff their binary is reachable in the enriched PATH (Homebrew/cargo/snap dirs included so GUI-launched Tauri still finds user CLIs); when the binary is off PATH they fall back to directory presence but strictly on the skills dir itself (`global_skills_dir`), never its parent — this still lets ZCode (a GUI app with `~/.zcode/skills` but no `zcode` binary) and a broken Codex install (`~/.codex/skills` exists, npm global never symlinked into `bin`) be detected, while preserving the shared-home-root disambiguation (a stray `~/.gemini` from Antigravity has no `~/.gemini/skills`, so Gemini is not false-positived). IDE/global-only agents (`antigravity`/`cursor`/`qoder`/`trae`/`openclaw`/`hermes`, and all custom agents) fall back to directory presence (skills dir or its parent).
- Each agent has a unique `project_skills_rel`; disambiguation is sealed (always returns empty).
- OpenClaw is global-only: `project_skills_rel` stays empty.
- Custom agent `project_skills_rel` may be entered with Windows backslashes in UI/commands, but backend storage and detection normalize it to forward slashes.

### Model Accounts

- Completing Codex OAuth login only adds or refreshes the account record; it must not auto-switch the current active Codex account.

### Usage / Subscriptions (`skillstar-usage`)

- Subscription + usage snapshots persist under `~/.skillstar/config/usage/`; Tauri commands live in `src-tauri/src/commands/usage_commands.rs`.
- Catalog is fixed; OAuth fetchers in `crates/skillstar-usage/src/fetchers/oauth/`; API-key fetchers in `fetchers/api_key/`. All HTTP uses `skillstar_core::infra::http_client::probe_http_client` (honours `config/proxy.json`).
- OAuth login returns `OAuthStartDto` (`auth_url`, `pending_id`, optional `user_code` for GitHub Copilot Device Flow). Frontend shows device code in `SubscriptionEditDialog`.
- Grok (`xai`) usage uses the xAI Grok CLI OAuth flow and reads `https://cli-chat-proxy.grok.com/v1/billing`; `monthlyLimit`, `used`, and `onDemandCap` are cents and should be rendered as monthly billing credits.
- **Local import** (`import_subscription_from_local`): default-install paths only — `codex` (`~/.codex/auth.json`), `antigravity` (IDE `state.vscdb` + protobuf oauth blob), `qoder` (IDE `state.vscdb` `secret://aicoding.auth.userInfo`). No multi-instance / `*_instance` binding.
- **Out of scope (do not implement):** per-provider multi-account lists, active-account switching UI, or cockpit-style `*_instance` / account picker flows. Usage page is one `Subscription` row per user-created entry; multiple rows for the same `catalog_id` are allowed but there is no “switch active account” concept.
- Google-family quota (Antigravity) lives in `crates/skillstar-usage/src/cloud_code.rs` (`loadCodeAssist`, `fetchAvailableModels`, token refresh).
- Qoder OpenAPI requests inject `Cosy-*` headers from `SharedClientCache/cache/machine_token.json` when present (`qoder_machine.rs`).
- **Do not modify** `fetchers/oauth/cursor.rs` unless explicitly requested; Cursor OAuth/usage is treated as complete.

### Launch Deck / Terminal

- Launch Deck is **single-pane only**; tmux-based multi-pane mode is removed across platforms.
- On Windows, **single mode** launch must run through a generated PowerShell script (`.ps1`) and must not require `bash`.

### AI Integration

- Model provider configuration (provider store + presets + external tool sync + latency + circuit breaker) lives in the `skillstar-models` crate; pure inference (chat completion, summarize, skill pick) lives in `skillstar-ai`. `skillstar-ai` depends on `skillstar-models` for provider resolution. Tauri commands in `commands/models_commands.rs` use `skillstar_models::*`; commands in `commands/ai/*` use `skillstar_ai::ai_provider`.
- External tool sync targets `claude-code` (`~/.claude/settings.json`), `codex` (`~/.codex/config.toml` + `auth.json`), `opencode` (`~/.config/opencode/opencode.json`, `provider.skillstar` block with `@ai-sdk/openai-compatible`), `gemini` (`~/.gemini/.env`), and `zcode` (`~/.zcode/v2/config.json`, OpenCode schema). OpenCode model blocks should prefer the provider's `meta.model_catalog` so `name`, `limit.context`, `limit.output`, and `cost` metadata are preserved when available. Tauri also exposes read/write/format/list for those on-disk configs plus `push_provider_to_tool_config` to re-apply the active provider for a tool.
- Provider endpoint probes (`test_endpoints_latency`, `fetch_provider_models`, `fetch_provider_model_catalog`, connection test) use `skillstar_core::infra::http_client::probe_http_client`, which honours `config/proxy.json`. Anthropic bases (`/anthropic` in URL) probe via `POST /messages`; OpenAI bases use `GET /models`. HTTP 401/403 are treated as reachable with auth failure, not hard errors. Model catalog enrichment belongs in `skillstar-models`; frontend code should consume normalized metadata instead of depending on a remote registry's raw JSON shape.
- AI provider config is backend-owned (`config/ai.json`); frontend never stores API keys.
- AI summary / quick read should prefer a Models provider reference (`provider_ref`) for Claude or Codex instead of duplicating URL/API key in `ai.json`; only `api_format=local` keeps manual base URL / model fields for Ollama-style local endpoints.
- Skill translation is English-to-Chinese by default and lives in `skillstar-ai::ai_provider::translate`; it must preserve Markdown structure through AST extraction + XML segment batching, skip only clearly already-target Chinese content, and reuse translations through a backend-owned SQLite cache under `~/.skillstar/db/`.
- AI skill pick must pre-rank installed skills locally before calling the model, keep the AI candidate catalog bounded, aggregate multi-round AI votes/scores into a stable ranking, and fall back to deterministic local ranking when AI output is partial or invalid.
- AI skill pick responses returned to the frontend must preserve relevance order and expose enough metadata (for example score/reason/fallback state) for the UI to explain why a skill was recommended.
- Streaming APIs emit:
  - `ai://summarize-stream` (`start`/`delta`/`complete`/`error`)

### Marketplace and Repo Flow

- Marketplace data is local-first via `~/.skillstar/db/marketplace.db`; UI reads snapshots first and only syncs remote scopes on-demand/background refresh.
- `marketplace.db` owns marketplace list/search/publisher/repo/detail snapshots plus FTS; schema changes must be handled through `PRAGMA user_version` migrations.
- MCP marketplace recommendations are maintained in `marketplace.db` (`mcp_curated_server` + FTS) and merged ahead of the GitHub MCP Registry snapshot; remote registry refreshes must not delete or overwrite curated MCP rows.
- MCP marketplace is organised around **official publishers** (mirrors the skill official-publishers two-level structure): the Marketplace top MCP tab is a single "官方" entry that opens a publisher card grid; clicking a card drills into a publisher detail page (`McpPublisherDetail`) listing that publisher's servers.
- MCP marketplace publishers: **AdsPower**, **BigModel**, **Anthropic**, **Microsoft**, **SaaS** (Notion/Figma/Stripe), **Dev Tools** (Context7/Firecrawl), **Cloudflare**, **Brave**, **Google**, **Supabase**, and **Vercel** (agent-browser + ai-sdk-5-migration) are curated rows partitioned by the `source` column (`"adspower"` / `"bigmodel"` / `"anthropic"` / `"microsoft"` / `"saas"` / `"cn-ai"` / `"cloudflare"` / `"brave"` / `"google"` / `"supabase"` / `"vercel"` — the value doubles as the publisher id); **GitHub** maps to the full `mcp_registry_server` table (`source` is NULL). `list_mcp_publishers_local` aggregates all twelve; `list_mcp_servers_by_publisher_local({ publisherId })` scopes cards to one publisher. Each curated publisher's `raw_server_json` follows the GitHub registry shape so the existing `registry_to_entry` install path works unchanged. Curated seeds live in `mcp_snapshot.rs` (`default_curated_mcp_servers` + per-publisher factory functions like `anthropic_curated_servers`); the grid order is fixed by the `CURATED_ORDER` constant in `load_publishers`. The Vercel bucket only includes installable servers — `agent-browser` (stdio, launched via `package_arguments: ["mcp"]` so the final command is `npx -y agent-browser mcp`) and `ai-sdk-5-migration-mcp-server` (remote http, no auth); the clone-to-deploy templates (`mcp-on-vercel`, `mcp-for-next.js`, `express-mcp`, `mcp-apps-nextjs-starter`, archived `mcp-on-vercel-with-stripe`), the `mcp-to-ai-sdk` CLI codegen tool, and the OAuth-only `mcp.vercel.com` platform server are intentionally omitted.
- Marketplace snapshot DB access should prefer short-lived WAL connections per operation so local read paths can run concurrently; avoid process-wide single-connection locking.
- Marketplace remote HTTP calls must use `skillstar_core::infra::http_client::probe_http_client` so `config/proxy.json` is honoured.
- Local marketplace search must prefer the snapshot/FTS corpus and only do explicit remote seeding when the user asks or the scope has never been synced.
- GitHub repo import is two-phase: `scan_github_repo` then `install_from_scan`.
- Repo-level updates and checks operate on `~/.skillstar/hub/repos/` cached repositories.

### Local Skills and Graduation

- Local skills live in `~/.skillstar/hub/local/<name>` and are symlinked into hub.
- Skills discovered from project-level agent folders and imported into SkillStar must be adopted into `skills-local/` first, then exposed through the hub symlink in `skills/`.
- `local_skill.rs` owns create/delete/migrate/graduate operations.
- Publishing local skills to GitHub should graduate them into hub-backed repo installs.

### Bundles and Share

- `.ags` and `.agd` bundles are tar.gz archives with manifest + checksum.
- Export flow can route between share code, inline embed, and bundle download by skill complexity.
- Share-code install is centralized in the Tauri command `install_from_share_code`: it accepts an already-parsed `ShareCodeSkill[]` payload, runs the "already-installed → git-backed → embedded → skip" decision per entry, and returns a `ShareCodeInstallSummary`. Both `ImportModal` and `ImportShareCodeModal` must call this command; do not re-implement the per-entry loop in TypeScript.
- Local folder adoption is centralized in the Tauri command `adopt_local_folder`: it scans `SKILL.md` via the standard discovery pipeline and turns each selected skill into a local-authored skill (same semantics as CLI `skillstar install <local-dir>`).
- `get_skill_deploy_status(skill_name)` returns per-agent deploy kind (`link` / `copy` / `missing` / `unknown`) for a given skill so the UI can surface when a symlink has silently degraded to a copy.

### Patrol and Runtime

- Patrol runs in per-cycle batches: prefetch unique repos once, then checks each skill locally with a tiny inter-skill delay; `interval_secs` is the cycle-to-cycle gap.
- Closing the window hides app to background only when background run is enabled; otherwise window close should quit the app and remove the tray icon.
- Patrol state persists in `~/.skillstar/state/patrol.json`.
- Tray background control must behave as a true start/stop toggle, and its label must stay in sync with the current background-run state shown in Settings.

### Storage APIs

- Storage overview APIs must return resolved real filesystem paths.
- Storage size scans for hub/cache/config must not follow symlink/junction targets; treat links as metadata-only entries to avoid recursive loops and Windows UI hangs.
- Respect `SKILLSTAR_DATA_DIR` and `SKILLSTAR_HUB_DIR` overrides.

### GitHub Mirror Acceleration

- GitHub mirror config is persisted in `~/.skillstar/config/github_mirror.json`; frontend auto-saves like proxy config.
- `core/config/github_mirror.rs` owns preset definitions, config load/save, URL rewriting, and connectivity testing.
- All git subprocess invocations (clone, fetch, pull, sparse checkout) inject mirror URL rewriting via `git -c url.*.insteadOf=...` per-command; the user's global `.gitconfig` is never modified.
- Mirror rewriting only affects `https://github.com/` URLs; non-GitHub remotes pass through unchanged.
- Built-in presets include commonly used community mirrors (ghproxy.vip, gh-proxy.com, github.akams.cn, gh.llkk.cc, ghfast.top); all use the URL-prefix proxy pattern.
- Users can specify a custom mirror URL; it must start with `https://` or `http://` and is normalized to end with `/`.
- Tauri commands: `get_github_mirror_config`, `save_github_mirror_config`, `get_github_mirror_presets`, `test_github_mirror`.
- `test_github_mirror` sends an HTTP HEAD request to verify reachability and returns latency in milliseconds.

### Local global agent skill discovery

- **Scope**: scans **installed** agent profiles' `global_skills_dir` (e.g. `~/.zcode/skills`) for subdirectories containing `SKILL.md`, classifying each as `hub_managed` (entry resolves into `~/.skillstar/hub`) or `standalone` (copy-only under the agent dir). Logic lives in `skillstar-projects::agents::global_scan` (`discover_global_agent_skills`).
- **Not Patrol**: hub patrol cycles do **not** scan agent folders; discovery runs when the **local** My Skills backend mounts (react-query on `SkillsProvider` load) and when the user clicks **My Skills refresh** (invalidates the discovery query alongside skills/updates).
- **Adoption**: `adopt_standalone_from_agent_entry` (`skillstar-skills::local_skill`) copies standalone trees into `hub/local/<name>`, symlinks into hub, removes the agent copy, and re-links via `toggle_skill_for_agent`. Tauri: `discover_global_agent_skills`, `adopt_standalone_agent_skill`.
- **UI**: local `SkillsBackend` exposes `globalAgentStandalone` + `adoptStandaloneAgentSkill`; `SkillShell` shows a banner and `LocalGlobalAgentAdoptDialog` when `capabilities.hasGlobalAgentDiscovery` and standalone count > 0 (remote backend sets the flag to `false`).

### SSH Remote Skill Management

- SSH remote skill management lives in the `skillstar-ssh` crate (pure Rust, Tauri-agnostic): `russh` (`ring` + `rsa` crypto backends) for connection/auth, `russh-sftp` for file transfer, `ssh-key` for key parsing, and the `keyring` crate (OS keychain on macOS / Credential Manager on Windows / Secret Service on Linux) for credential storage. Tauri commands in `commands/ssh_hosts.rs` are a thin forwarder over the crate.
- Host metadata persists in `~/.skillstar/config/ssh_hosts.toml` (non-sensitive fields only: display name, host, port, username, auth method, key *path*, default remote dir). Passphrases and passwords **never** touch the TOML — they live in the system keyring under service `skillstar-ssh`, keyed by host `id`. Deleting a host also clears its keyring entry (best-effort).
- The My Skills **remote (SSH)** scope surfaces hosts discovered from the user's `~/.ssh/config` (`system_config::parse_system_hosts`). These are **read-only** (parsed live each call, never written to `ssh_hosts.toml`) and shown in a separate "system" section with an "import" button that copies one into the managed store. The parser is dependency-free (a ~120-line state machine handling `Host`/`HostName`/`User`/`Port`/`IdentityFile`, `Include` recursion with glob support, and wildcard-alias filtering). System hosts connect directly using their `~/.ssh/config` identity file — no keyring involved — via the `system:<alias>` synthetic host id handled in `with_session`.
- Host-key verification is TOFU (trust-on-first-use): the first connection reports the server SHA-256 fingerprint to the UI (`test_ssh_connection` returns `unverified` + fingerprint); the user confirms via `accept_ssh_host_key`, which persists it to `ssh_known_hosts.json`. Subsequent mismatches hard-fail with `HOST_KEY_MISMATCH` (MITM protection). Verified hosts connect silently.
- Authentication supports both private-key files (path + optional passphrase) and password login. The crate resolves hub skill content dirs the same way `skill_content` does (follows the `skills/<name>` symlink, falls back to nested `<name>/SKILL.md`).
- **Push uses the remote `.skillstar` hub layout** (`hub::push_skill_via_hub`, invoked by `push_skill_to_remote`): upload the local hub skill tree to `~/.skillstar/hub/content/<name>` over SFTP (atomic `.skillstar.tmp` → `rename`, skipping `.git`), then run `ln -sfn` over SSH so `~/.<agent>/skills/<name>` symlinks to that content — mirroring local hub + agent link semantics on the VPS.
- Direct copy into an agent folder (no hub symlink) remains available via `sftp::push_skill` for internal/tests; the UI path always goes through hub push.
- **Discovery classifies hub layout**: each skill gets `layout` (`hub_managed` | `standalone`) via remote `readlink` + hub `SKILL.md` probe; `DiscoveryResult.needs_migration_count` counts standalone entries (self-downloaded / copy-only under agent dirs).
- **`migrate_remote_skill_to_hub`** (`hub::migrate_remote_skill_to_hub`): `mv` standalone tree to `~/.skillstar/hub/content/<name>`, then `ln -sfn` into the agent `skills/` dir; UI shows a migration banner and per-card migrate action on standalone skills.
- **Connection console**: every SSH command streams progress to the frontend via the `ssh://connect-stream` Tauri event. The crate emits structured `SshProgressEvent`s (phase = dial/handshake/host_key/auth/sftp/done/error, status = start/ok/warn/fail/pending) through a `ProgressSink` trait (`progress.rs`) — kept Tauri-agnostic. The command layer injects a `TauriProgressSink` that forwards to `app.emit`. Each invocation gets a unique `session_id` so the UI can filter events. The frontend `useConnectStream` hook subscribes and renders a terminal-style console in `RemoteSkillPanel`; when a host-key check is `pending` (first connection), the console pauses with a fingerprint + "trust" button instead of a throwaway toast. Errors also land in the console (phase=error, status=fail) so the user sees why a connection failed, not just a silent retry.
- `list_remote_skills` walks a remote dir and only reports subdirectories that contain `SKILL.md` (genuine skills, not stray folders). `delete_remote_skill` is recursive and idempotent.
- **Remote SSH UI**: My Skills no longer renders a separate `RemoteSkillPanel`; the same **`SkillShell`** drives both scopes, consuming a `SkillsBackend` injected via `<SkillsProvider backend={...}>` (`src/features/my-skills/backends/`). The remote backend (`useRemoteSkillsBackend`) adapts discovery + the remote content/lifecycle commands into the unified `SkillsBackend` shape; `remoteSkillToSkill` maps `RemoteSkill` → `Skill` and `SkillCapabilities` flags hide features the remote backend cannot serve (publish, deploy-to-project, AI batch, source/repo filters). The legacy `RemoteSkillPanel`/`RemoteSkillDrawer` are retained only as fallback render arms for read-only remote skills; the host picker, push dialog, and connection console are passed into `SkillShell` via the `filtersLead`/`actionsLead` slots. `RemoteSkillsContent` still exists as an export shim that delegates to `SkillShell`.
- **Remote skill discovery is scan-based, not table-based**: `discover_remote_skills` lists the remote `$HOME/.*` directories (skipping a blacklist of large/irrelevant ones like `.cache`/`.npm`/`.config`/`.ssh`/`.pm2`/`.kube`), and for each looks for a `skills/` subdir whose entries containing `SKILL.md` (dirs without it are ignored). Scan/build logic is split into pure helpers (`should_skip_home_dir`, `build_discovery_result`, `filter_remote_skill_list`) so VPS layouts (e.g. `system:vps-yy` hosts) are unit-testable without a live dial. This finds **any** agent — known (claude/codex/gemini) or unknown (grok, `.agents`, future ones) — without a hardcoded path table. It returns `DiscoveryResult { agents, skills, needs_migration_count }` (each skill carries its `agent` and `layout` so the UI can group/filter and prompt migration). `KNOWN_AGENT_SKILL_DIRS` is kept only as a fallback seed when the scan finds nothing (fresh server). `$HOME` is resolved via SFTP `canonicalize(".")`. `list_remote_skills(remote_dir)` remains for targeted listing of one directory.
- **Unified UI remote capability parity (Phase 2)**: the remote backend now surfaces read/edit content, update/pull, install-from-URL, per-agent link toggle, batch link, and update-availability checks via six new Tauri commands — `read_remote_skill_content`, `write_remote_skill_content`, `pull_remote_skill`, `install_remote_skill`, `toggle_remote_agent_link`, `check_remote_skill_updates` — all thin forwarders over `skillstar-ssh::hub` (atomic `.skillstar.tmp`→`rename` writes for content; `git -C <content> pull --ff-only` / `git clone --depth 1` for lifecycle; `ln -sfn`/`rm -f` for agent symlinks). `SkillCapabilities` gates which UI affordances appear per backend (remote = no publish / no deploy-to-project / no AI batch / no source/repo filters; everything else enabled).

### ACP Integration

- ACP Client implementation is in `core/acp_client.rs`; it implements the `acp::Client` trait with auto-approved permissions and text collection via `session_notification`.
- ACP config commands are in `commands/acp.rs`: `get_acp_config`, `save_acp_config`.
- The ACP client is retained for potential future agent integrations but has no active consumer in the current codebase.

## Page Responsibilities


| Page        | Scope   | Responsibility                                               |
| ----------- | ------- | ------------------------------------------------------------ |
| `My Skills` | Global + remote (SSH) | Unified `SkillShell` driven by an injected `SkillsBackend` (local hub/filesystem or remote SSH). Local: installed skills + per-agent links + publish/deploy/AI. Remote: VPS SSH hosts, discovery, push/migrate/delete + remote read/edit/pull/install/toggle (toolbar scope switch swaps the provider's backend). |
| `Projects`  | Project | Register project, configure project-level agent skills, sync |
| `Decks`     | Bundle  | Package/import/export skill sets and deploy to projects      |


Cross-page flow: Decks deploy enters Projects with pre-selected skills; skill chips in Projects open the in-page detail panel.

## Auto-Update

SkillStar uses `tauri-plugin-updater` (no custom command layer required).

- Plugin setup in `src-tauri/src/lib.rs`
- Hook logic in `src/hooks/useUpdater.ts`
- Sidebar update UI in `src/components/layout/Sidebar.tsx`
- CI release pipeline in `.github/workflows/release.yml`

Release checklist:

1. Bump version in `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`.
2. Commit version bump.
3. Tag and push: `git tag vX.Y.Z && git push origin vX.Y.Z`.

`latest.json` is generated by `scripts/release/build_merged_latest_json.cjs` and published to latest release assets.

## Design Context

See `docs/impeccable.md` for brand and visual direction (`Precise. Unified. Effortless.`, dark glassmorphism, accessibility baseline).

## Maintenance Rules

- Document-first:
  - backend or architecture change → update `AGENTS.md` first
  - frontend structure/convention change → update `AGENTS-UI.md` first
- Keep directory/dependency sections in docs synced with reality.
- Log significant bug investigations and fixes in `docs/Error.md`.

### Commit Guidelines

Use Conventional Commits: `type(scope): description`

- `type`: `feat` / `fix` / `docs` / `style` / `refactor` / `perf` / `test` / `chore`
- `scope`: feature area such as `skills`, `projects`, `agents`, `layout`
- Commit messages must be English.

## Do NOT

- Do not manually add Rust dependencies by editing `Cargo.toml`; use `cargo add`.

## Quality & CI

- Lint + format: `bun run lint` / `bun run lint:fix` / `bun run format` (Biome).
- Frontend tests: `bun run test` / `bun run test:watch` (Vitest + jsdom).
- Test files live alongside source or under `src/test/` with `*.test.ts(x)` / `*.spec.ts(x)` naming.
- Tauri IPC is auto-mocked in test setup (`src/test/setup.ts`).
- CI supply chain audit: `cargo-deny` checks advisories, licenses, and sources against `src-tauri/deny.toml`.
- CI lockfile: all `bun install` steps use `--frozen-lockfile` for reproducibility.
