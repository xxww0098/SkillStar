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


## Project Structure (Condensed)

```text
SkillStar/
├── src/                           # React app
│   ├── features/                  # domain slices (components + hooks)
│   │   ├── my-skills/             # skill grid, cards, modals, install/export
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
- When a project is selected, `refresh_stale_project_copies` compares SHA-256 content hashes of copy-deployed skill directories against their hub sources; stale copies are re-deployed while intentionally deleted skills are not restored.
- Windows/global agent unlink must attempt `remove_link_or_copy` for any existing entry (link/junction/copy), and only treat missing targets as no-op.
- Repo scan/import defaults to **root-first**: when repo root has a valid `SKILL.md`, treat the root as the primary single skill by default.
- Repo scan/import may optionally use **full-depth** discovery to include nested `SKILL.md` skills in the same repository.
- Imported skill identity should prefer frontmatter `name`; fall back to directory name (and for root-level fallback use repo name when needed).
- CLI install mode is split: `skillstar install <url>` defaults to **project-level** linking for the current working directory, while `skillstar install --global <url>` performs **hub-only** installation.
- Project-level CLI install should prefer linking into already-detected agent project folders and fall back to `.agents/skills` when no project-level agent path exists yet.
- CLI install should accept explicit project targets via `--agent <id>` (repeatable or comma-separated), and when omitted in an interactive terminal should allow user input before falling back to auto-detect.
- CLI `install` / `add` accepts repo URL, `owner/repo` shorthand, local `.ags`/`.agd` bundle paths, and local directories containing `SKILL.md`; directory inputs are adopted as local-authored skills via `local_skill::create`.
- CLI `install` flags mirror the `npx skills add` baseline: `--list` scans without mutating, `--all` installs every discovered skill, `--yes` skips interactive prompts, `--copy` signals a preference for copy deployment (project sync still auto-falls-back to copy when symlinks fail).
- CLI `find [query]` / `search [query]` routes through `skillstar_marketplace::snapshot::search_local` so the local FTS snapshot powers CLI discovery; `--json` emits the raw `LocalFirstResult` for scripting.
- CLI `remove` / `rm` / `uninstall` uninstalls by name, accepts `--all` and `--yes`; it is the same codepath as the Tauri `uninstall_skill` command.
- CLI `init [name]` replaces the hard-coded `create`; the old name is kept as an alias.
- CLI install output annotates each project-level link with its actual deployed kind (`link`, `copy`, `broken-link`, `missing`) so users can see when a symlink silently degraded to a copy.

### Project Registration and Detection

- Project registration is explicit before scan/import/sync.
- `detect_project_agents` is data-driven by `builtin_profiles()`.
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
- External tool sync targets `claude-code` (`~/.claude/settings.json`), `codex` (`~/.codex/config.toml` + `auth.json`), and `opencode` (`~/.config/opencode/opencode.json`, `provider.skillstar` block with `@ai-sdk/openai-compatible`). OpenCode model blocks should prefer the provider's `meta.model_catalog` so `name`, `limit.context`, `limit.output`, and `cost` metadata are preserved when available. Tauri also exposes read/write/format/list for those on-disk configs plus `push_provider_to_tool_config` to re-apply the active provider for a tool.
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

### ACP Integration

- ACP Client implementation is in `core/acp_client.rs`; it implements the `acp::Client` trait with auto-approved permissions and text collection via `session_notification`.
- ACP config commands are in `commands/acp.rs`: `get_acp_config`, `save_acp_config`.
- The ACP client is retained for potential future agent integrations but has no active consumer in the current codebase.

## Page Responsibilities


| Page        | Scope   | Responsibility                                               |
| ----------- | ------- | ------------------------------------------------------------ |
| `My Skills` | Global  | Manage installed skills + per-agent links                    |
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
