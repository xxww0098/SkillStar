# Changelog

All notable changes to SkillStar are listed here.
Format follows [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

## [0.1.3] - 2026-04-02

## [0.1.2] - 2026-04-01

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
