# SkillStar — Code Framework

## Architecture
SkillStar is a Tauri v2 desktop app with a React SPA frontend and Rust backend.

- Frontend calls backend via `invoke()` and Tauri events.
- Tauri commands are defined in `src-tauri/src/commands.rs` and split modules under `src-tauri/src/commands/`.
- Core domain logic lives in `src-tauri/src/core/`.
- Persistence is mixed storage under `~/.skillstar/`: JSON for config/project metadata plus SQLite for marketplace and translation caches.
- Skill distribution is symlink-based to keep project directories clean.

> Frontend-specific conventions live in [AGENTS-UI.md](./AGENTS-UI.md).

## Key Paths
| Purpose | Path |
|---|---|
| Hub skills | `~/.skillstar/.agents/skills/` |
| Local authored skills | `~/.skillstar/.agents/skills-local/` |
| Repo cache | `~/.skillstar/.agents/.repos/` |
| SkillStar data root | `~/.skillstar/` |
| Projects metadata | `~/.skillstar/projects/` |
| Marketplace snapshot DB | `~/.skillstar/marketplace.db` |
| Translation cache DB | `~/.skillstar/translation_cache.db` |

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
│   │   ├── commands.rs            # Tauri command root
│   │   ├── commands/              # marketplace, agents, projects, github, ai, patrol
│   │   └── core/                  # domain modules
│   │       ├── ai_provider/       # AI config, translation, summarization, skill pick
│   │       ├── security_scan/     # static/AI scanning, cache, logging
│   │       ├── marketplace_snapshot/ # local-first marketplace DB
│   │       └── ...                # skills, sync, repo, local, bundles, paths
│   ├── Cargo.toml
│   └── tauri.conf.json
├── docs/Error.md
├── AGENTS.md
└── AGENTS-UI.md
```

## Frontend Dependencies
| Package | Version | Purpose |
|---|---|---|
| `react` / `react-dom` | 18.x | UI runtime |
| `vite` | 5.x | Build tool |
| `tailwindcss` | 4.x | Styling |
| `@tauri-apps/api` | 2.x | IPC bridge |
| `framer-motion` | 12.x | Motion |
| `lucide-react` | 0.436 | Icons |
| `sonner` | 2.x | Toasts |
| `react-markdown` | 10.x | Markdown render |
| `@radix-ui/*` | latest | Accessible primitives |

## Backend Dependencies (Rust)
| Crate | Version | Purpose |
|---|---|---|
| `tauri` | 2 | Desktop framework |
| `tokio` | 1 | Async runtime |
| `reqwest` | 0.12 | HTTP |
| `gix` | 0.68 | Git operations |
| `serde` / `serde_json` | 1 | Serialization |
| `anyhow` | 1 | Error handling |
| `chrono` | 0.4 | Time |
| `clap` | 4 | CLI parsing |
| `regex` | 1.12 | Pattern matching |
| `toml` | 1.1 | TOML parsing |
| `flate2` + `tar` | 1.1 / 0.4 | Bundle packing |
| `html2md` | 0.2 | Marketplace detail conversion |
| `sys-locale` | 0.3 | Locale detection |

## Backend Behavior Rules

### Skills and Sync
- Keep heavy logic in `core/*`, not in command wrappers.
- Installed-skill list should render fast from local snapshot first.
- Remote update checks run in bounded background work.
- Project sync is reconciliation: add selected symlinks, remove stale ones, prune empty agent folders.

### Project Registration and Detection
- Project registration is explicit before scan/import/sync.
- `detect_project_agents` is data-driven by `builtin_profiles()`.
- Each agent has a unique `project_skills_rel`; disambiguation is sealed (always returns empty).
- OpenClaw is global-only: `project_skills_rel` stays empty.

### AI Integration
- AI provider config is backend-owned (`ai_config.json`); frontend never stores API keys.
- Long translation should fallback to chunked translation when full pass returns empty content.
- Short description translation always includes MyMemory (public API) path; users choose priority (`ai_first` or `mymemory_first`) but no longer toggle MyMemory on/off.
- Short description translation source must be exposed to frontend (`ai` or `mymemory`) so UI can show where the result came from.
- AI skill pick must pre-rank installed skills locally before calling the model, keep the AI candidate catalog bounded, aggregate multi-round AI votes/scores into a stable ranking, and fall back to deterministic local ranking when AI output is partial or invalid.
- AI skill pick responses returned to the frontend must preserve relevance order and expose enough metadata (for example score/reason/fallback state) for the UI to explain why a skill was recommended.
- All translation entry points must use SQLite (`~/.skillstar/translation_cache.db`) as the durable cache keyed by text hash + target language; frontend may keep transient display state but must not become the source of truth for translation reuse.
- Short description translation and SKILL.md translation are persisted in SQLite (`~/.skillstar/translation_cache.db`) keyed by text hash + target language; normal translate uses cache and explicit "retranslate" bypasses then overwrites cache.
- `Retranslate via AI` must mean AI-only refresh (not just cache bypass with provider fallback).
- `ai_translate_skill_stream` must run as a single global SKILL.md translation session; concurrent requests should serialize so one API-key session is active at a time.
- Security scan prompts must explicitly enforce `target_language` for model-generated natural language fields (`summary`, `description`, `recommendation`) while keeping schema enums stable.
- Security scan writes both rolling runtime logs (`~/.skillstar/security_scan.log`) and per-run timestamped reports (`~/.skillstar/security_scan_logs/scan-<timestamp>-<request>.log`).
- Streaming APIs emit:
  - `ai://translate-stream` (`start`/`delta`/`complete`/`error`)
  - `ai://summarize-stream` (`start`/`delta`/`complete`/`error`)

### Security Scan
- Security scan cache keys must distinguish scan mode (`static` vs `ai`) and scanner version.
- Cache validation must hash full file contents, not truncated AI snippets.
- Partial AI analysis failures must not be cached as `Safe`; surface them as incomplete results.
- Editing installed skill files in-app must invalidate the related security scan cache entry.
- `clear_security_scan_cache` must clear both SQLite cache tables and security scan log files (runtime + per-run reports).
- Skill-level fan-out can stay bounded, but AI concurrency must be enforced globally across chunk analysis and final aggregation.
- Chunk scheduling should favor throughput for large skills while still guaranteeing progress for every active skill.
- File-level scan cache writes should be batched per skill after chunk work completes to avoid SQLite contention during concurrent scans.

### Marketplace and Repo Flow
- Marketplace data is local-first via `~/.skillstar/marketplace.db`; UI reads snapshots first and only syncs remote scopes on-demand/background refresh.
- `marketplace.db` owns marketplace list/search/publisher/repo/detail snapshots plus FTS; schema changes must be handled through `PRAGMA user_version` migrations.
- Marketplace snapshot DB access should prefer short-lived WAL connections per operation so local read paths can run concurrently; avoid process-wide single-connection locking.
- Local marketplace search must prefer the snapshot/FTS corpus and only do explicit remote seeding when the user asks or the scope has never been synced.
- GitHub repo import is two-phase: `scan_github_repo` then `install_from_scan`.
- Repo-level updates and checks operate on `~/.skillstar/.agents/.repos/` cached repositories.

### Local Skills and Graduation
- Local skills live in `~/.skillstar/.agents/skills-local/<name>` and are symlinked into hub.
- Skills discovered from project-level agent folders and imported into SkillStar must be adopted into `skills-local/` first, then exposed through the hub symlink in `skills/`.
- `local_skill.rs` owns create/delete/migrate/graduate operations.
- Publishing local skills to GitHub should graduate them into hub-backed repo installs.

### Bundles and Share
- `.ags` and `.agd` bundles are tar.gz archives with manifest + checksum.
- Export flow can route between share code, inline embed, and bundle download by skill complexity.

### Patrol and Runtime
- Patrol runs in per-cycle batches: prefetch unique repos once, then checks each skill locally with a tiny inter-skill delay; `interval_secs` is the cycle-to-cycle gap.
- Closing the window hides app to background only when background run is enabled; otherwise window close should quit the app and remove the tray icon.
- Patrol state persists in `~/.skillstar/patrol.json`.
- Tray background control must behave as a true start/stop toggle, and its label must stay in sync with the current background-run state shown in Settings.

### Storage APIs
- Storage overview APIs must return resolved real filesystem paths.
- Respect `SKILLSTAR_DATA_DIR` and `SKILLSTAR_HUB_DIR` overrides.

## Page Responsibilities
| Page | Scope | Responsibility |
|---|---|---|
| `My Skills` | Global | Manage installed skills + per-agent links |
| `Projects` | Project | Register project, configure project-level agent skills, sync |
| `Decks` | Bundle | Package/import/export skill sets and deploy to projects |

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
See `.impeccable.md` for brand and visual direction (`Precise. Unified. Effortless.`, dark glassmorphism, accessibility baseline).

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
