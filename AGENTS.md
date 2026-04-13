# SkillStar — Code Framework

## Architecture

SkillStar is a Tauri v2 desktop app with a React SPA frontend and Rust backend.

- Frontend calls backend via `invoke()` and Tauri events.
- Tauri commands are defined in `src-tauri/src/commands/mod.rs` (root handlers) with feature modules under `src-tauri/src/commands/`.
- Core domain logic lives in `src-tauri/src/core/`.
- Persistence is mixed storage under `~/.skillstar/`: JSON for config/project metadata plus SQLite for marketplace and translation caches.
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
│   │   ├── security/              # security scanning
│   │   └── settings/              # app settings sections
│   ├── hooks/                     # global hooks (useNavigation, useUpdater, useAiConfig)
│   ├── pages/                     # thin route-level shells
│   ├── components/                # ui/, layout/, shared/
│   ├── lib/                       # shared utilities
│   └── types/                     # shared TS types
├── src-tauri/
│   ├── src/
│   │   ├── commands/              # mod.rs: skills, bundles, shell, network; + marketplace, agents, projects, github, patrol, acp
│   │   │   └── ai/               # AI commands split: translate, summarize, scan
│   │   └── core/                  # domain modules (domain-first layout)
│   │       ├── infra/             # cross-cutting infrastructure (paths, fs_ops, migration, error, db_pool, util)
│   │       ├── ai/               # AI domain facade (translation_cache, re-exports ai_provider)
│   │       ├── ai_provider/       # AI config, HTTP client, translation, summarization, skill pick
│   │       ├── config/            # user-editable config (proxy, github_mirror)
│   │       ├── git/              # git operations (ops, gh_manager, repo_history, source_resolver, dismissed_skills)
│   │       ├── projects/          # project management (agents, sync, manifest re-export)
│   │       ├── terminal/          # Launch Deck config + terminal_backend re-export
│   │       ├── skills/            # skill lifecycle (install, update, bundle, local, group, discover)
│   │       ├── security_scan/     # static/AI scanning, orchestrator, cache, logging
│   │       ├── marketplace_snapshot/ # local-first marketplace DB
│   │       ├── model_config/      # per-provider model config (codex, claude, gemini, opencode)
│   │       └── project_manifest/  # project manifest types, helpers, path resolution
│   ├── Cargo.toml
│   └── tauri.conf.json
├── docs/
│   ├── Error.md
│   ├── CHANGELOG.md
│   └── impeccable.md       # brand & visual baseline
├── scripts/
│   ├── release/
│   ├── security_scan/
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


## Backend Behavior Rules

### Skills and Sync

- Keep heavy logic in `core/*`, not in command wrappers.
- Installed-skill list should render fast from local snapshot first.
- Remote update checks run in bounded background work.
- Project sync is reconciliation: add selected skills per agent, remove stale entries, prune empty agent folders; deployment always tries symlink first — if symlink creation fails (e.g. Windows without Developer Mode), falls back to full directory copy automatically. The `deploy_modes` field in `skills-list.json` is retained for backward-compat but ignored.
- When a project is selected, `refresh_stale_project_copies` compares SHA-256 content hashes of copy-deployed skill directories against their hub sources; stale copies are re-deployed while intentionally deleted skills are not restored.
- Windows/global agent unlink must attempt `remove_link_or_copy` for any existing entry (link/junction/copy), and only treat missing targets as no-op.
- Repo scan/import defaults to **root-first**: when repo root has a valid `SKILL.md`, treat the root as the primary single skill by default.
- Repo scan/import may optionally use **full-depth** discovery to include nested `SKILL.md` skills in the same repository.
- Imported skill identity should prefer frontmatter `name`; fall back to directory name (and for root-level fallback use repo name when needed).
- CLI install mode is split: `skillstar install <url>` defaults to **project-level** linking for the current working directory, while `skillstar install --global <url>` performs **hub-only** installation.
- Project-level CLI install should prefer linking into already-detected agent project folders and fall back to `.agents/skills` when no project-level agent path exists yet.
- CLI install should accept explicit project targets via `--agent <id>` (repeatable or comma-separated), and when omitted in an interactive terminal should allow user input before falling back to auto-detect.

### Project Registration and Detection

- Project registration is explicit before scan/import/sync.
- `detect_project_agents` is data-driven by `builtin_profiles()`.
- Each agent has a unique `project_skills_rel`; disambiguation is sealed (always returns empty).
- OpenClaw is global-only: `project_skills_rel` stays empty.
- Custom agent `project_skills_rel` may be entered with Windows backslashes in UI/commands, but backend storage and detection normalize it to forward slashes.

### Model Accounts

- Completing Codex OAuth login only adds or refreshes the account record; it must not auto-switch the current active Codex account.

### Launch Deck / Terminal

- On Windows, **single mode** launch must run through a generated PowerShell script (`.ps1`) and must not require `bash`.
- On Windows, **multi mode (tmux)** is disabled; Launch Deck must force/keep single mode and reject multi deploy attempts with a clear message.

### AI Integration

- AI provider config is backend-owned (`config/ai.json`); frontend never stores API keys.
- Long translation should fallback to chunked translation when full pass returns empty content.
- Short description translation always includes MyMemory (public API) path; users choose priority (`ai_first` or `mymemory_first`) but no longer toggle MyMemory on/off.
- Short description translation source must be exposed to frontend (`ai` or `mymemory`) so UI can show where the result came from.
- AI skill pick must pre-rank installed skills locally before calling the model, keep the AI candidate catalog bounded, aggregate multi-round AI votes/scores into a stable ranking, and fall back to deterministic local ranking when AI output is partial or invalid.
- AI skill pick responses returned to the frontend must preserve relevance order and expose enough metadata (for example score/reason/fallback state) for the UI to explain why a skill was recommended.
- All translation entry points must use SQLite (`~/.skillstar/db/translation.db`) as the durable cache keyed by text hash + target language; frontend may keep transient display state but must not become the source of truth for translation reuse.
- Short description translation and SKILL.md translation are persisted in SQLite (`~/.skillstar/db/translation.db`) keyed by text hash + target language; normal translate uses cache and explicit "retranslate" bypasses then overwrites cache.
- `Retranslate via AI` must mean AI-only refresh (not just cache bypass with provider fallback).
- `ai_translate_skill_stream` must run as a single global SKILL.md translation session; concurrent requests should serialize so one API-key session is active at a time.
- SKILL.md translation prefers a **single full-document** API call when the file fits a budget derived from `context_window_k` (heading-based section fan-out only above that size), to cut round trips and reduce network failure rates.
- Security scan prompts must explicitly enforce `target_language` for model-generated natural language fields (`summary`, `description`, `recommendation`) while keeping schema enums stable.
- Security scan AI analysis should fallback to the configured local OpenAI-compatible endpoint (for example Ollama) when primary provider calls fail, while preserving the same response schema.
- Security scan writes both rolling runtime logs (`~/.skillstar/logs/security.log`) and per-run timestamped reports (`~/.skillstar/logs/scans/scan-<timestamp>-<request>.log`).
- Streaming APIs emit:
  - `ai://translate-stream` (`start`/`delta`/`complete`/`error`)
  - `ai://summarize-stream` (`start`/`delta`/`complete`/`error`)

### Security Scan

- Security scan cache keys must distinguish scan mode (`static` vs `ai`) and scanner version.
- Static scanning is orchestrator-driven (`security_scan/orchestrator.rs`) with pluggable analyzers; default registry includes `pattern`, `doc_consistency` (SKILL.md behavior consistency), `secrets`, `semantic` (call-graph/flow), and optional `dynamic`/`semgrep`/`trivy`/`osv`/`grype`/`gitleaks`/`shellcheck`/`bandit`/`sbom`/`virustotal`.
- Policy file (`~/.skillstar/config/scan_policy.yaml`) controls preset, severity threshold, ignore/override rules, and `enabled_analyzers` selection.
- Security scan results and exported reports should include per-analyzer execution telemetry (`id`/`status`/`findings`/`error`) so unavailable tools and degraded runs are visible without opening raw logs.
- Smart triage is rule-engine driven from `~/.skillstar/state/scan_smart_rules.yaml` (fallback: bundled default YAML), and rule updates should not require Rust code changes.
- Cache validation must hash full file contents, not truncated AI snippets.
- Partial AI analysis failures must not be cached as `Safe`; surface them as incomplete results.
- Editing installed skill files in-app must invalidate the related security scan cache entry.
- `clear_security_scan_cache` must clear both SQLite cache tables and security scan log files (runtime + per-run reports).
- Skill-level fan-out can stay bounded, but AI concurrency must be enforced globally across chunk analysis and final aggregation.
- Chunk scheduling should favor throughput for large skills while still guaranteeing progress for every active skill.
- File-level scan cache writes should be batched per skill after chunk work completes to avoid SQLite contention during concurrent scans.

### Marketplace and Repo Flow

- Marketplace data is local-first via `~/.skillstar/db/marketplace.db`; UI reads snapshots first and only syncs remote scopes on-demand/background refresh.
- `marketplace.db` owns marketplace list/search/publisher/repo/detail snapshots plus FTS; schema changes must be handled through `PRAGMA user_version` migrations.
- Marketplace snapshot DB access should prefer short-lived WAL connections per operation so local read paths can run concurrently; avoid process-wide single-connection locking.
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

