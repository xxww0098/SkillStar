# SkillStar — Code Framework

## Architecture

Tauri v2 desktop application for managing AI agent skills across multiple coding agents (Claude, Cursor, Windsurf, etc.). The frontend is a React SPA communicating with a Rust backend via Tauri IPC commands.

> **Web UI**: see [AGENTS_UI.md](./AGENTS_UI.md) for frontend framework and conventions.

- **Tauri IPC** — Frontend invokes Rust commands via `@tauri-apps/api/core` `invoke()`. All backend logic is exposed through `#[tauri::command]` functions in `src-tauri/src/commands.rs`.
- **Core modules** — Domain logic lives in `src-tauri/src/core/` with dedicated modules for skills, agents, projects, marketplace, git operations, etc.
- **Data storage** — JSON files in the system data directory (`~/Library/Application Support/skillstar/` on macOS). No database required.
- **Skill management** — Skills are Git repositories cloned into a central hub directory (`~/.agents/skills/`). Multi-skill repositories are cloned into a repo cache (`~/.agents/.repos/`) with individual skills symlinked from the hub. Symlinks connect skills to agent-specific configuration directories.
- **Project management** — Project configs are stored in SkillStar's data directory (`skillstar/projects/`). Project directories only receive symlinks — zero file pollution.

## Project Structure

```
SkillStar/
├── package.json                   # React 18 + Vite + TailwindCSS 4
├── vite.config.ts                 # Vite configuration
├── tsconfig.json                  # TypeScript config
├── index.html                     # SPA entry point
├── src/                           # ━━ Frontend source ━━━━━━━━━━━━━
│   ├── main.tsx                   # App bootstrap
│   ├── App.tsx                    # Root layout + routing + cross-page navigation context
│   ├── index.css                  # TailwindCSS theme tokens + base styles
│   ├── vite-env.d.ts              # Vite global types
│   ├── types/
│   │   └── index.ts               # Shared TypeScript types (Skill, Project, Agent, etc.)
│   ├── hooks/
│   │   ├── useSkills.ts           # Installed skills CRUD + agent linking (global SkillsProvider state)
│   │   ├── useAgentProfiles.ts    # Agent profile listing + toggling
│   │   ├── useProjectManifest.ts  # Project registration + skill sync
│   │   ├── useSkillCards.ts       # Skill card deck CRUD + deploy
│   │   ├── useMarketplace.ts      # skills.sh marketplace search
│   │   ├── useAiConfig.ts         # AI provider config + translate/summarize
│   │   └── useUpdater.ts          # Auto-update check/download/install hook
│   ├── lib/
│   │   ├── utils.ts               # Tailwind cn() helper
│   │   ├── toast.ts               # Sonner toast wrapper
│   │   ├── shareCode.ts           # Skill group share code encode/decode
│   │   ├── backgroundStyle.ts     # Global background style persistence + DOM apply
│   │   ├── skillUpdateRefresh.ts  # Pending-update refresh mode persistence + interval resolver
│   │   └── marketplaceDescriptionHydration.ts # Marketplace description hydration helpers + patch merge
│   ├── pages/
│   │   ├── MySkills.tsx           # Global skill management + per-agent linking
│   │   ├── Marketplace.tsx        # skills.sh marketplace browser
│   │   ├── PublisherDetail.tsx     # Publisher drill-down sub-page
│   │   ├── SkillCards.tsx         # Skill card deck management + deploy navigation
│   │   ├── Projects.tsx           # Thin re-export wrapper
│   │   └── projects-page/         # Projects page sections
│   │       ├── index.tsx          # Projects page composition + state/handlers
│   │       ├── DeployBanner.tsx
│   │       ├── ProjectListPanel.tsx
│   │       ├── ProjectDetailPanel.tsx
│   │       ├── ScanImportBanner.tsx
│   │       ├── AgentAccordion.tsx
│   │       └── ApplyFooter.tsx
│   │   ├── Settings.tsx           # Thin re-export wrapper
│   │   └── settings-page/         # Settings page sections
│   │       ├── index.tsx          # Settings page composition + state/handlers
│   │       ├── AgentConnectionsSection.tsx
│   │       ├── ProxySection.tsx
│   │       ├── AiProviderSection.tsx
│   │       ├── UpdateRefreshSection.tsx
│   │       ├── AppearanceSection.tsx
│   │       ├── LanguageSection.tsx
│   │       ├── StorageSection.tsx
│   │       └── AboutSection.tsx
│   └── components/
│       ├── ui/                    # Reusable UI primitives
│       │   ├── button.tsx
│       │   ├── badge.tsx
│       │   ├── card.tsx
│       │   ├── input.tsx
│       │   ├── EmptyState.tsx
│       │   ├── Skeleton.tsx
│       │   └── sonner.tsx
│       ├── layout/
│       │   ├── Sidebar.tsx        # Left navigation sidebar
│       │   ├── Toolbar.tsx        # Page toolbar (search, sort, view mode)
│       │   └── DetailPanel.tsx    # Right-side skill detail panel
│       ├── skills/
│       │   ├── SkillCard.tsx      # Individual skill card with agent toggles
│       │   ├── SkillGrid.tsx      # Grid/list layout for skill cards
│       │   ├── SkillEditor.tsx    # SKILL.md content editor
│       │   ├── SkillSelectionBar.tsx # Batch selection toolbar
│       │   ├── CreateGroupModal.tsx  # Create/edit deck
│       │   ├── DeployToProjectModal.tsx # Quick deploy modal (used in MySkills)
│       │   ├── ProjectDeployAgentDialog.tsx # Project deploy target picker
│       │   ├── ImportShareCodeModal.tsx  # Import deck from share code
│       │   ├── ExportShareCodeModal.tsx  # Export deck as share code
│       │   ├── UninstallConfirmDialog.tsx # Uninstall confirmation
│       │   ├── GitHubImportModal.tsx  # GitHub repo scan + batch skill import
│       │   ├── PublishSkillModal.tsx   # Publish local skill to GitHub
│       │   ├── ImportBundleModal.tsx   # Import .agentskill bundle file
│       │   └── RecommendedRow.tsx # Recommended skills row
│       ├── marketplace/
│       │   └── OfficialPublishers.tsx # Publisher cards grid
├── public/                        # Static assets (agent icons)
├── scripts/
│   └── download_avatars.cjs       # Agent avatar download script
├── src-tauri/                     # ━━ Rust backend ━━━━━━━━━━━━━━━━
│   ├── Cargo.toml                 # Rust dependencies
│   ├── tauri.conf.json            # Tauri app configuration
│   ├── build.rs                   # Tauri build script
│   └── src/
│       ├── main.rs                # Tauri entry point
│       ├── lib.rs                 # Command registration + plugin setup
│       ├── cli.rs                 # CLI argument parsing
│       ├── commands.rs            # Tauri command root (skills + shared helpers)
│       ├── commands/              # Split command modules by domain
│       │   ├── marketplace.rs     # skills.sh search/leaderboard/publishers/description-hydration commands
│       │   ├── agents.rs          # Agent profile and per-agent link commands
│       │   ├── projects.rs        # Project registration/sync/scan/import commands
│       │   ├── github.rs          # GitHub status/publish/repo scanner/cache commands
│       │   └── ai.rs              # AI provider config/translate/summarize/test commands
│       └── core/                  # Domain logic modules
│           ├── mod.rs             # Module exports
│           ├── skill.rs           # Skill + OfficialPublisher types
│           ├── skill_group.rs     # SkillGroup CRUD (JSON persistence)
│           ├── installed_skill.rs # Installed skill discovery + update checks
│           ├── agent_profile.rs   # Agent profile detection + management
│           ├── ai_provider.rs     # AI config + OpenAI-compatible translate/summarize
│           ├── proxy.rs           # Proxy config load/save + schema
│           ├── project_manifest.rs # Project registration + skill list + sync
│           ├── marketplace.rs     # skills.sh API integration
│           ├── path_env.rs        # Cross-platform PATH enrichment for GUI binary discovery
│           ├── git_ops.rs         # Git clone/pull/hash via gitoxide (gix)
│           ├── sync.rs            # Symlink management (hub ↔ agent dirs)
│           ├── lockfile.rs        # Skill lockfile (install tracking + source_folder)
│           ├── repo_scanner.rs    # GitHub repo clone/scan/batch-install + update detection
│           ├── repo_history.rs    # Repo scan history persistence
│           ├── skill_bundle.rs    # .agentskill bundle export/import (tar.gz packaging)
│           └── gh_manager.rs      # GitHub CLI status check + skill publish to GitHub
└── dist/                          # Vite build output
```

## Frontend Dependencies

| Package | Version | Purpose |
|---------|---------|---------|
| `react` / `react-dom` | 18.x | UI framework |
| `vite` | 5.x | Build tool |
| `tailwindcss` | 4.x | Utility-first CSS |
| `@tauri-apps/api` | 2.x | Tauri IPC bridge |
| `@tauri-apps/plugin-dialog` | 2.x | Native file dialogs |
| `@tauri-apps/plugin-shell` | 2.x | Shell command execution |
| `framer-motion` | 12.x | Animation library |
| `lucide-react` | 0.436 | Icon library |
| `class-variance-authority` | 0.7 | Component variant styling |
| `sonner` | 2.x | Toast notifications |
| `react-markdown` | 10.x | Markdown rendering |
| `@radix-ui/*` | latest | Accessible UI primitives |

## Backend Dependencies (Rust)

| Crate | Version | Purpose |
|-------|---------|---------|
| `tauri` | 2 | Desktop app framework |
| `serde` / `serde_json` | 1 | Serialization |
| `tokio` | 1 | Async runtime |
| `reqwest` | 0.12 | HTTP client (marketplace + AI API) |
| `gix` | 0.68 | Git operations (gitoxide) |
| `chrono` | 0.4 | Timestamp handling |
| `dirs` | 5 | System directory paths |
| `anyhow` | 1 | Error handling |
| `clap` | 4 | CLI argument parsing |
| `serde_yaml` | 0.9 | YAML frontmatter parsing |
| `regex` | 1.12 | Pattern matching |
| `toml` | 1.1 | TOML config parsing |
| `flate2` | 1.1 | Gzip compression/decompression |
| `tar` | 0.4 | Tar archive packing/unpacking |

## SkillStar Desktop Backend Addendum

- Tauri desktop commands live in `src-tauri/src/commands.rs`; avoid adding more heavy logic there when a core module can own it instead.
- Installed-skill discovery should prefer a fast local snapshot for first paint, while remote Git update checks run as bounded background work.
- Per-skill Git and filesystem probes must avoid fully serial execution; use Rust async orchestration with controlled blocking-task concurrency for repository metadata collection.
- Project-level sync is a reconciliation pass, not append-only behavior: applying project config must remove stale symlinks for agents that were deselected and prune any now-empty agent config directories such as `.agents/` or `.claude/`.
- Project registration is an explicit step: the Projects page should register the selected path in `projects.json` before loading details, scanning unmanaged skills, or importing project-local skills.
- AI integration uses a pluggable OpenAI-compatible provider configured in `ai_config.json`. All AI API calls go through the Rust backend (`ai_provider.rs`) using `reqwest`, inheriting the user's proxy settings. The frontend never handles API keys directly.
- Full-document skill translation is resilient to long Markdown files: `ai_provider.rs` should fallback to bounded chunk-by-line translation when a single-pass translation returns an empty AI payload, then reassemble chunks in original order.
- Skill translation supports streaming display: `ai_translate_skill_stream` emits `ai://translate-stream` events (`start`/`delta`/`complete`/`error`) while translating so the editor can render live output.
- AI quick-read summary supports streaming display: `ai_summarize_skill_stream` emits `ai://summarize-stream` events (`start`/`delta`/`complete`/`error`) so DetailPanel can render incremental summary output.
- Marketplace skills use a hybrid description strategy: leaderboard/search returns quickly, then missing `Skill.description` values are hydrated in bounded background batches via `hydrate_marketplace_descriptions`. Descriptions are extracted from skills.sh skill pages (`Summary` block) and cached in `marketplace_description_cache.json` under the SkillStar data directory with TTL + size pruning.
- GitHub repo import uses a two-phase flow: `scan_github_repo` clones/fetches into `~/.agents/.repos/` and scans for SKILL.md files, then `install_from_scan` creates symlinks from `~/.agents/skills/` into the cached repo. Update checking and `git pull` operate on the cached repo, automatically updating all skills from the same source.
- Local skill publishing uses `gh_manager.rs` to initialize git, create a GitHub repository via `gh` CLI, push code, and update the lockfile with the new `git_url`. The Share Code system supports inline embedding of small SKILL.md content for skills without a git remote.
- Skill bundles (`.agentskill` files) provide a third sharing tier: `skill_bundle.rs` packs a skill directory into a tar.gz archive with a `manifest.json` (metadata + SHA-256 checksum). Users export via a save dialog and import via the `ImportBundleModal`, which previews manifest info and handles name conflicts. This mechanism is for multi-file local skills that are too large for Share Code inline embedding but don't need GitHub publishing.

### Design Philosophy — Page Responsibilities

| Page | Scope | Responsibility |
|------|-------|----------------|
| **My Skills** | 全局 | 管理 Hub 中所有已安装 skill，per-agent 链接（全局 symlink） |
| **Projects** | 项目级 | 管理项目注册、项目内 agent 配置、项目级 skill 部署和同步 |
| **Decks** | 技能包 | 打包/导入/导出 skill 组合，一键 Deploy 到 Projects |

- Cross-page navigation: Decks → Deploy → Projects (with pre-selected skills); Projects → click skill → My Skills (auto-open DetailPanel).

## Auto-Update System

The desktop app supports automatic updates via `tauri-plugin-updater`. No custom backend module or Tauri commands are needed — the plugin handles everything.

### Architecture

```
GitHub Release (tag v*)
  └── latest.json (CI auto-generated, per-platform signatures + download URLs)
       └── tauri-plugin-updater (frontend API via @tauri-apps/plugin-updater)
            └── useUpdater hook → Sidebar update notification bar
```

- **Backend**: Only plugin registration in `lib.rs` (3 lines). `tauri-plugin-updater` + `tauri-plugin-process` registered under `#[cfg(desktop)]`.
- **Frontend**: `src/hooks/useUpdater.ts` manages all lifecycle — check, download, install, relaunch — via dynamic imports. State (skipped version, last check time) stored in `localStorage`, no backend commands.
- **UI**: Update notification bar in `src/components/layout/Sidebar.tsx`, below the logo area. States: `available` → `downloading` → `ready` → `error`. No modal dialogs.

### Release Pipeline

The release is triggered by pushing a version tag:

```bash
# 1. Bump version in both package.json AND src-tauri/Cargo.toml (must match)
# 2. Commit the version bump
# 3. Tag and push
git tag v0.2.0 && git push origin v0.2.0
```

CI workflow (`.github/workflows/release.yml`) executes 3 jobs:

| Job | Action |
|-----|--------|
| `release` | Matrix build (macOS arm64/x64, Linux x64, Windows x64) via `tauri-apps/tauri-action@v0`. Creates a **draft** release with signed artifacts. |
| `rebuild-latest-json` | Downloads all assets, runs `scripts/release/build_merged_latest_json.cjs` to generate `latest.json` with per-platform download URLs and signatures, uploads it to the release. |
| `publish-release` | Publishes the draft release as the latest release. |

### Version Bumping

When releasing a new version, update **both** files:

1. `package.json` — `"version": "0.2.0"`
2. `src-tauri/Cargo.toml` — `version = "0.2.0"`

The CI validates that the git tag matches `package.json` version. Frontend version display in `Sidebar.tsx` footer should also be updated.

### Signing Key

- Generated with: `npx @tauri-apps/cli signer generate -w ~/.tauri/skillstar.key`
- Public key: stored in `src-tauri/tauri.conf.json` → `plugins.updater.pubkey`
- Private key: stored as GitHub Secret `TAURI_SIGNING_PRIVATE_KEY`
- `tauri.conf.json` must have `"bundle": { "createUpdaterArtifacts": true }` to generate `.sig` files during build

### latest.json Manifest

Generated by `scripts/release/build_merged_latest_json.cjs`. Structure:

```json
{
  "version": "0.2.0",
  "notes": "release notes...",
  "pub_date": "2026-...",
  "platforms": {
    "darwin-aarch64": { "signature": "...", "url": "https://github.com/..." },
    "darwin-x86_64":  { ... },
    "windows-x86_64": { ... },
    "linux-x86_64":   { ... },
    ...
  }
}
```

The updater endpoint in `tauri.conf.json` points to:
```
https://github.com/<OWNER>/<REPO>/releases/latest/download/latest.json
```

### Files Involved

| File | Role |
|------|------|
| `src-tauri/Cargo.toml` | `tauri-plugin-updater` + `tauri-plugin-process` (desktop conditional) |
| `src-tauri/tauri.conf.json` | Updater endpoint, pubkey, `createUpdaterArtifacts` |
| `src-tauri/src/lib.rs` | Plugin registration under `#[cfg(desktop)]` |
| `src/hooks/useUpdater.ts` | Update check/download/install lifecycle hook |
| `src/components/layout/Sidebar.tsx` | Update notification bar UI |
| `src/App.tsx` | Wires `useUpdater()` → Sidebar props |
| `.github/workflows/release.yml` | CI build + latest.json + publish |
| `scripts/release/build_merged_latest_json.cjs` | Manifest builder |

## Maintenance Rules

- **Backend Document-First**: Update `AGENTS.md` with new architectures, flows, or structural changes before writing backend code.
- **Frontend Document-First**: Update `AGENTS_UI.md` with new components, pages, or structural changes before writing frontend code.
- **Directory Sync**: The `Project Structure` tree must strictly reflect the actual project. Update it when adding or moving modules.
- **Dependency Sync**: New Rust crates must be added via `cargo add` and documented in the `Backend Dependencies` table.

### Commit 规范
遵循 Conventional Commits：`type(scope): description`
- **type**：`feat` / `fix` / `docs` / `style` / `refactor` / `perf` / `test` / `chore`
- **scope**：`layout` / `chat` / `vnc` / `event-store` / `debug-panel` / `skills` / `projects` / `agents` 等
- 允许使用中文 commit message。

## Do NOT

- **Do NOT** manually edit `Cargo.toml` to add dependencies — always use `cargo add`.
