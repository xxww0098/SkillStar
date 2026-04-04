# Changelog

All notable changes to SkillStar are listed here.
Format follows [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

## [0.1.7] - 2026-04-04

### Added
- **`remove_link_or_copy()` utility** — inverse of `create_symlink_or_copy()` that safely removes symlinks, NTFS junction points, or directory copies (Windows fallback), with SKILL.md presence check to prevent accidental deletion of unrelated directories.

### Fixed
- **Windows unlink silently failing** — skills deployed as directory copies (when both symlinks and junctions failed) could not be unlinked, uninstalled, or toggled off; all sync/unlink paths (`toggle_skill_for_agent`, `remove_skill_from_all_agents`, `unlink_all_skills_from_agent`, `unlink_skill_from_agent`) now handle copy-based deployments.
- **Copy-based skills invisible in UI** — `list_linked_skills`, `detect_agent_links`, and `count_symlinks` now detect directory copies with SKILL.md, so Settings and My Skills pages correctly display agent link badges and synced counts on Windows.
- **Re-enabling a skill after copy deployment** — the enable path in `toggle_skill_for_agent` now removes a previous copy-based deployment before creating a proper symlink, instead of bailing with "Target cannot be overwritten".
- **Project-level unlink** — `remove_skill_from_all_projects` and `clear_project_symlinks` now also clean up copy-based skill deployments in project directories.

## [0.1.6] - 2026-04-04

### Added
- **Windows Developer Mode banner** — platform-aware guidance component that detects NTFS junction/symlink capability and guides users to enable Developer Mode for full functionality.
- **`check_developer_mode` command** — backend check for Windows Developer Mode status via registry query.
- **`is_link()` utility** — centralized cross-platform helper that detects both true symlinks and NTFS junction points, replacing scattered `is_symlink()` checks.
- **`create_symlink_or_copy()` fallback** — project-level deployment now falls back to directory copy when both symlinks and junctions fail (e.g. cross-drive on Windows without Developer Mode).
- **Mirror-aware updater** — `commands/updater.rs` with `check_app_update` / `download_app_update` / `install_app_update` Rust commands that dynamically inject GitHub mirror endpoints into the Tauri updater, bypassing network restrictions.
- **`gix` → Git CLI fallback** — `compute_tree_hash` now falls back to `git rev-parse` via CLI when `gix` fails on Windows (NTFS locking / shallow clone quirks).

### Changed
- **Symlink detection unified** — all filesystem-sensitive operations (`installed_skill`, `local_skill`, `sync`, `agent_profile`, `gh_manager`, `repo_scanner`, `skill_bundle`, `skill_pack`, `project_manifest`) now use `paths::is_link()` instead of raw `is_symlink()`.
- **Junction target resolution** — `read_link` calls in `local_skill` and `gh_manager` now fall back to `junction::get_target()` on Windows.
- **Updater hook refactored** — `useUpdater.ts` now calls Rust-side commands (`check_app_update` / `download_app_update` / `install_app_update`) instead of using the JS `@tauri-apps/plugin-updater` directly; supports mirror-aware endpoints and download progress events.
- **Update check timeout** increased from 12s to 20s to accommodate mirror latency.
- **SVG icon fix** — `AgentIcon` and `AntigravityIcon` interaction improvements.

### Fixed
- Windows symlink operations silently failing when Developer Mode is disabled — now detected and handled with junction fallback + user guidance.
- Updater unable to check for updates when GitHub mirror acceleration is enabled.
- `gix` panicking on Windows shallow clones due to NTFS file-locking.

## [0.1.5] - 2026-04-04

### Added
- **Command Palette** (`⌘K` / `Ctrl+K`) — fuzzy-searchable action launcher for quick page navigation and app-wide commands.
- **Keyboard Shortcuts** — `⌘1–6` page navigation, `⌘,` Settings, `⌘I` Import; extracted into `useKeyboardShortcuts` hook.
- **Git installation detection** (`check_git_status`) — cross-platform Git availability check with platform-specific install instructions (Xcode CLT / Homebrew / winget / apt / pacman) shown in Settings → About.
- **Settings sidebar navigation** — floating icon-only dock with IntersectionObserver-driven scroll highlighting and framer-motion entrance/selection animations.
- **Windows security scan patterns** — PowerShell encoded commands (Critical), `schtasks` scheduled task persistence (High), and registry Run/RunOnce auto-start persistence (High).
- **External link interception** — `<a href="https://…">` clicks now open in the system browser instead of navigating the WebView (fixes Windows WebView2 navigation).
- **Platform path display** — `isWindows()` / `formatPlatformPath()` utilities ensure backslash separators in Settings agent paths on Windows.
- **`useTauriSetup` hook** — extracted patrol sync, tray language init, and window-hidden handler from App.tsx into a dedicated lifecycle hook.

### Changed
- **AI command module refactored** — monolithic `commands/ai.rs` (2 400+ LOC) split into `commands/ai/{mod,translate,summarize,scan}.rs` submodules; public API unchanged.
- **Fonts bundled locally** — DM Sans and JetBrains Mono variable TTFs shipped in `public/fonts/`; removed Google Fonts CDN dependency and tightened CSP (`style-src` / `font-src` no longer allow external origins).
- **Font stack extended** — added `PingFang SC` and `Microsoft YaHei` fallbacks for CJK rendering.
- **Detail panel widened** — max width increased from `max-w-sm` to `max-w-md`; action buttons moved to a sticky bottom bar.
- **Detail panel Escape close** — pressing `Escape` dismisses the panel when not in edit/read mode.
- **Detail panel translation auto-trigger** — no longer requires AI config for short-text translation (MyMemory-only path works); skips auto-translate when `localized_description` is already hydrated.
- **ResizablePanel** — max width now computed from parent container width (not viewport), preventing panels from extending behind the sidebar; width re-clamped on window resize.
- **SHA-256 helper deduplicated** — moved inline hex-encoding into `core::util::sha256_hex()`; consumed by `installed_skill.rs` and `translation_cache.rs`.
- **GitHub mirror preset** — replaced defunct `GHP.ci` with `GHFast.top`.
- **Patrol mutex handling** — all `.lock().unwrap()` calls replaced with `.unwrap_or_else(|p| p.into_inner())` to recover from poisoned mutexes instead of panicking.
- **Directory copy** — `copy_dir_recursive` in `gh_manager` and `local_skill` now skips Windows system files (`Thumbs.db`, `desktop.ini`) alongside `.DS_Store`.
- **File deletion** — `remove_dir_all` replaced with `remove_dir_all_retry` (3 attempts, 200 ms delay) to handle Windows file-locking (antivirus / search indexer).
- **Dynamic sandbox** — security scan sandbox environment split into Unix and Windows branches for correct PATH / HOME / TEMP handling.
- **Updater hook** — `check()` now returns `{ found, version? }` for callers that need to act on the result.
- **Viewport units** — `100vh` supplemented with `100dvh` for the root element, fixing mobile/dynamic viewport height.

### Fixed
- Detail panel opening no longer triggers an unintended AI retranslation when MyMemory is the only configured provider.
- ResizablePanel width restored from localStorage could exceed the available parent area on smaller screens.
- Patrol crash-loops on mutex poisoning after a previous panic.
- Windows WebView2 bounds desync (content rendered in top half only) — forced resize on startup.

### Removed
- **Setup Hook system** — `setup_hook.rs`, `SetupHookPanel.tsx`, and all ACP build/rebuild commands (`acp_generate_setup_hook`, `acp_rebuild_skills`, `scan_rebuild_skills`, `apply_rebuild_skills`, `get_setup_hook`, `save_setup_hook`, `delete_setup_hook`, `run_setup_hook`).
- `setup-hooks/` path from hub directory tree and legacy migration.

### Added
- **Security Scan** — full-featured page with three scan modes (Static / Smart / Deep):
  - Static regex pattern matching (curl-pipe-sh, reverse shells, cron persistence, etc.).
  - AI-powered chunk-batched deep analysis with file-role classification.
  - Radar sweep animation, live file trail, concurrent chunk worker indicators.
  - Per-run timestamped reports (`~/.skillstar/security_scan_logs/`) plus rolling runtime log.
  - SQLite-backed scan cache (keyed by content hash + mode + scanner version).
  - Scan estimate preview (file count, API calls) and mode fallback warnings.
  - Clear cache action that wipes both SQLite entries and log files.
- **AI Translation & Summarization**:
  - Streaming SKILL.md translation (`ai://translate-stream`) with chunked fallback for long documents.
  - Streaming AI summary (`ai://summarize-stream`).
  - Short description translation with dual provider path (AI + MyMemory public API).
  - Provider priority setting (`ai_first` / `mymemory_first`); translation source exposed to UI.
  - SQLite translation cache (`~/.skillstar/translation_cache.db`) keyed by text hash + target language.
  - Per-content-hash concurrency control — parallel translation of different skills, serialized for identical content.
  - "Retranslate via AI" action that bypasses cache and forces AI-only refresh.
- **AI Skill Picker** — local pre-ranking → bounded candidate catalog → multi-round AI scoring → stable fallback ranking.
- **Full i18n** (`i18next`) with English and Simplified Chinese locales across all pages.
- **Antigravity** agent profile and `public/agents/antigravity.svg` icon.
- **Local skill lifecycle**: create / edit / delete / migrate / graduate (`~/.skillstar/.agents/skills-local/`).
- **Background patrol** — low-frequency, single-skill update checking with Settings and tray controls.
- **Treeless sparse clone** install flow for large repos (with fallback to full clone).
- **Skill bundles** — `.ags` (single) and `.agd` (deck) tar.gz archives with manifest and checksum.
- **Share code** improvements — inline embed, human-readable wrapper, backward-compatible import.
- **Marketplace** detail scraping and background description hydration cache (`marketplace_description_cache.json` with TTL + size pruning).
- **Project agent auto-detection** via `builtin_profiles()` + shared-path disambiguation dialog.
- **Reusable UI primitives**: `ResizablePanel`, `HScrollRow`, `AgentIcon`, `SearchInput`, `SelectAllButton`, `SplashScreen`, `LoadingLogo`, `SuccessCheckmark`, `ErrorBoundary`.
- **Import modal** refactored into multi-phase architecture (`InputURL → Loading → SelectSkills → ShareCodePreview → Completed → Error`).
- **Skill Reader** component for in-panel SKILL.md reading, translation, and summary.
- **Settings – Short Text Service** section extracted from AI provider config.
- **Settings – Background Run** section for patrol toggle.
- **API key encryption** with AES-256-GCM (machine-uid derived key).
- **Scan config auto-derivation** from context window K-tokens (chunk size, response tokens, concurrency).
- **AI concurrency** override exposed in Settings UI (default 4).

### Changed
- Rust backend upgraded to **Edition 2024** (`rust-version = "1.94.1"`).
- `reqwest` bumped to **0.13**; `gix` bumped to **0.80**.
- `rusqlite` added for durable translation and security scan caches.
- Agent icon system unified and moved under `public/agents/`.
- Settings page restructured into extracted section components (`AgentConnections`, `Proxy`, `AiProvider`, `ShortTextService`, `BackgroundRun`, `Appearance`, `Language`, `Storage`, `About`).
- Repo update flow standardized to fetch/reset-style deterministic sync.
- Update checks optimized with per-repo prefetch and bounded concurrency.
- Window close behavior changed to hide-to-background (explicit quit via tray).
- Shared HTTP client (`LazyLock`) for AI and marketplace requests — eliminates ~100-200 ms per request.
- Scan parameters auto-derived from `context_window_k`; manual overrides are power-user escape hatch.
- Visual hierarchy refined: removed unnecessary backdrop-blur from standard cards; glassmorphism reserved for modals and floating panels.

### Fixed
- CLI argument mismatch compilation issue.
- UTF-8 unsafe string slicing and AI response parsing robustness.
- Atomic cache writes for marketplace description cache.
- Publisher detail missing skills/repo data fallback behavior.
- Shallow clone update loop causing perpetual "Update Available".
- Broken symlink detection and uninstall cleanup reliability.
- Modal overflow behavior for long content.
- Cross-platform PATH enrichment for `git`/`gh` command discovery.
- AI translation first-token latency reduced by switching from global serial lock to per-content-hash concurrency.
- Security scan cache validation now hashes full file contents (not truncated snippets).
- Partial AI scan failures no longer cached as "Safe" — surfaced as incomplete results.
- Editing installed skill files in-app now invalidates related security scan cache.

### Removed
- Legacy `test_parse` artifact.
- Deprecated tracked scripts (`run-build.sh`, `run-dev.sh`) from version control.
- Large `public/demo.mp4` file.
- `DEVELOPMENT.md` (merged into `AGENTS.md`).
- `UpdateRefreshSection` settings component (replaced by `BackgroundRunSection`).

## [0.1.1] - 2026-03-30

### Added
- `get_publisher_repos` command with SSR-first parsing strategy.
- Repo parsing helper and unit tests for publisher extraction.
- Additional `.gitignore` entries for caches/artifacts.

### Changed
- Project rebranded from AgentHub to SkillStar.
- Concept renamed from "Skill Groups" to "Decks".
- CLI naming updated from `agenthub` to `skillstar`.

### Fixed
- Publisher detail repo list incompleteness.
- Marketplace tab state reset when navigating back.
- Cross-platform PATH enrichment for `git`/`gh` command discovery.

## [0.1.0] - 2026-03-29

### Added
- Core pages: My Skills, Marketplace, Decks, Publisher Detail, Settings.
- Local skill install/update/uninstall and batch operations.
- Project-level symlink sync across supported providers.
- CLI + GUI dual-mode runtime in one binary.
- Right-side detail panel, toasts, and base desktop UX skeleton.
