# SkillStar — Code Framework

## Architecture
SkillStar is a Tauri v2 desktop app with a React SPA frontend and Rust backend.

- Frontend calls backend via `invoke()` and Tauri events.
- Tauri commands are defined in `src-tauri/src/commands.rs` and split modules under `src-tauri/src/commands/`.
- Core domain logic lives in `src-tauri/src/core/`.
- Persistence is JSON-file based under `~/.skillstar/` (config, projects, and skill hub).
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

## Project Structure (Condensed)
```text
SkillStar/
├── src/                           # React app
│   ├── hooks/                     # Data hooks (skills, projects, marketplace, AI, updater)
│   ├── pages/                     # MySkills, Marketplace, PublisherDetail, SkillCards, Projects, Settings
│   ├── components/                # ui/, layout/, skills/, marketplace/
│   ├── lib/                       # shared utilities
│   └── types/                     # shared TS types
├── src-tauri/
│   ├── src/
│   │   ├── commands.rs            # Tauri command root
│   │   ├── commands/              # marketplace, agents, projects, github, ai, patrol
│   │   └── core/                  # domain modules (skills, sync, repo, local, bundles, paths...)
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
- Marketplace list uses fast payload first; hydrate missing descriptions in bounded batches.
- Description cache: `marketplace_description_cache.json` with TTL and size pruning.
- GitHub repo import is two-phase: `scan_github_repo` then `install_from_scan`.
- Repo-level updates and checks operate on `~/.skillstar/.agents/.repos/` cached repositories.

### Local Skills and Graduation
- Local skills live in `~/.skillstar/.agents/skills-local/<name>` and are symlinked into hub.
- `local_skill.rs` owns create/delete/migrate/graduate operations.
- Publishing local skills to GitHub should graduate them into hub-backed repo installs.

### Bundles and Share
- `.ags` and `.agd` bundles are tar.gz archives with manifest + checksum.
- Export flow can route between share code, inline embed, and bundle download by skill complexity.

### Patrol and Runtime
- Patrol checks one skill at a time with low overhead delay.
- Closing the window hides app to background; quit is explicit via tray menu.
- Patrol state persists in `~/.skillstar/patrol.json`.

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
