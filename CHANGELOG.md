# Changelog

All notable changes to SkillStar are listed here.
Format follows [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

## [0.1.2] - 2026-03-31

### Added
- Security Scan page and backend pipeline (static + AI deep scan) with risk badges.
- Full i18n (`i18next`) with English and Simplified Chinese locales.
- Antigravity agent profile and icon support.
- Local skill lifecycle (`create/edit/delete/migrate/graduate`) backed by `~/.agents/skills-local/`.
- Background patrol task for update checks plus Settings/tray controls.
- Treeless sparse clone install flow for large repos (with fallback).
- Skill bundle import/export (`.ags`, `.agd`) with manifest and checksum.
- Share code improvements (inline embed + human-readable wrapper + backward-compatible import).
- Marketplace detail scraping and background description hydration cache.
- Project agent auto-detection + shared-path disambiguation flow.
- Streaming AI summary (`ai_summarize_skill_stream`) and reusable UI primitives (ResizablePanel, HScrollRow, AgentIcon, etc.).

### Changed
- Rust backend upgraded to Edition 2024.
- Agent icon system unified and moved under `public/agents/`.
- Settings restructured (background running controls, dynamic version display).
- Repo update flow standardized to fetch/reset-style deterministic sync.
- Update checks optimized with per-repo prefetch and bounded concurrency.
- Window close behavior changed to hide-to-background.

### Fixed
- CLI argument mismatch compilation issue.
- UTF-8 unsafe string slicing and AI response parsing robustness.
- Atomic cache writes for marketplace description cache.
- Publisher detail missing skills/repo data fallback behavior.
- Shallow clone update loop causing perpetual "Update Available".
- Broken symlink detection and uninstall cleanup reliability.
- Modal overflow behavior for long content.

### Removed
- Legacy `test_parse` artifact.
- Deprecated tracked scripts (`run-build.sh`, `run-dev.sh`) from version control.
- Large `public/demo.mp4` file.
- `DEVELOPMENT.md` (merged into `AGENTS.md`).

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
