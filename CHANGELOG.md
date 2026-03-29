# Changelog

All notable changes to SkillStar will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

### Changed
- Rebranded project from AgentHub to `SkillStar` across all documentation and project config
- Renamed "Skill Groups" to "Decks" in conceptual models to align with card-based UI
- Updated `agenthub` CLI references to `skillstar`
- Appended standard Commit conventions and definitions to `AGENTS.md`

### Added
- Added missing `.gitignore` entries for local cache (`.agents/`) and testing artifacts (`proofshot-artifacts/`)
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
