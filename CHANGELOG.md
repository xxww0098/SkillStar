# Changelog

All notable changes to SkillStar will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

## [0.1.1] - 2026-03-30

### Fixed
- **Publisher Detail missing repos**: parse complete repo list from `skills.sh/official` SSR payload instead of the per-publisher page, which omits low-traffic repos (e.g. GitHub showing 5 repos but only listing 3)
- **Marketplace tab lost on back**: preserve the active Marketplace category tab (Hot/Trending/Official etc.) when returning from Publisher Detail
- **Cross-platform git/gh discovery**: extract PATH enrichment into a shared `path_env.rs` module with platform-specific paths (macOS Homebrew, Linux snap/local, Windows Program Files/Scoop); apply to all 12 `Command::new("git"/"gh")` call sites across `gh_manager.rs`, `git_ops.rs`, and `repo_scanner.rs`

### Changed
- Rebranded project from AgentHub to `SkillStar` across all documentation and project config
- Renamed "Skill Groups" to "Decks" in conceptual models to align with card-based UI
- Updated `agenthub` CLI references to `skillstar`
- Appended standard Commit conventions and definitions to `AGENTS.md`

### Added
- `get_publisher_repos` backend command with two-phase strategy: official SSR payload first, per-publisher page scraping as fallback
- `parse_publisher_repos_from_official_payload` for extracting publisher repos from Next.js SSR JSON data
- `format_installs_label` helper to format numeric install counts as human-readable labels (`2.4M`, `100.0K`)
- Unit test for publisher repos SSR payload parsing
- Missing `.gitignore` entries for local cache (`.agents/`) and testing artifacts (`proofshot-artifacts/`)

## [0.1.0] - 2026-03-29

### Added

- **My Skills** page: install, update, uninstall local skills with tree-hash change detection
- **Marketplace** page: browse skills from GitHub, search, filter by All/Trending/Hot/Official
- **Decks**: create and manage decks for quick project setup
- **Publisher Detail**: drill-down view for official publisher skill collections
- **Settings**: agent connection management, proxy config, dependency status
- **Batch operations**: select multiple skills for batch install/uninstall
- **Detail panel**: right slide-out with skill info, README preview, install action
- **CLI + GUI dual mode**: same binary works as terminal commands and desktop app
- **Provider support**: Claude Code, Codex CLI, Gemini CLI, Cursor, Windsurf, Aider
- **Symlink sync**: share skills across providers via OS symlinks
- **Marketplace**: GitHub API integration with stars-based ranking and categorization
- **Zero system dependency**: pure Rust Git (gix/gitoxide), no `git` CLI required
- **Toast notifications**: install/uninstall feedback with animated status indicators
- **Slogan**: "less is more" displayed in sidebar branding
