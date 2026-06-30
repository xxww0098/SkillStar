# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Language

Respond to the user in Chinese (per AGENTS.md). Commit messages must be English, using Conventional Commits: `type(scope): description` (e.g. `feat(skills): ...`, scopes like `skills`, `projects`, `agents`, `layout`, `usage`, `models`).

## Detailed docs (read these for the area you're touching)

- **AGENTS.md** — structure single source of truth: tech stack, project tree, workspace-crate map, doc index. Structure/dependency/crate changes must update AGENTS.md first.
- **docs/backend.md** — backend behavior rules per subsystem (skills sync, project detection, model/usage, AI, fingerprint, marketplace, SSH, S3, patrol, storage, github mirror, ACP, auto-update). Backend behavior changes must update this first.
- **AGENTS-UI.md** — frontend structure, Models hub UI, streaming UX rules, visual system. Frontend convention changes must update AGENTS-UI.md first.
- **ADDING-AN-AGENT.md** — step-by-step guide for adding a new Agent CLI (builtin data table, optional tool-sync axis, optional usage axis).
- Significant bug investigations get logged in `docs/Error.md`.

## Commands

```bash
bun install                 # deps (Bun is the package manager; bun.lock)
bun run tauri dev           # full desktop app in dev mode
bun run dev                 # Vite frontend only (no Tauri backend)
bun run tauri build         # package the app

bun run lint                # Biome check (lint + format check)
bun run lint:fix            # Biome auto-fix
bun run test                # all frontend tests (Vitest + jsdom)
bunx vitest run src/path/to/File.test.tsx     # single test file
bunx vitest run -t "test name"                # single test by name

cargo test                              # all Rust tests (workspace root)
cargo test -p skillstar-models          # one crate
cargo test -p skillstar-models test_fn  # one test
cargo check                             # fast compile check
```

- **Rust test safety:** tool-sync code resolves `~/.claude`, `~/.codex`, etc. Tests must never write to the real `$HOME` — integration tests must set `SKILLSTAR_TOOL_SYNC_HOME` to a temp dir (unit tests inside `skillstar-models` auto-sandbox via a `cfg(test)` fallback). This was a real bug: tests once clobbered the developer's live `~/.codex/config.toml`.
- Add Rust dependencies with `cargo add`, never by hand-editing `Cargo.toml`.
- Tauri IPC is auto-mocked in frontend tests via `src/test/setup.ts`.

## Architecture

Tauri v2 desktop app (plus a CLI in the same `skillstar` binary — entry in `src-tauri/src/cli.rs`). React 19 SPA frontend, Rust backend. The sidebar mode switcher exposes three product modes: **Skills** (skill management/distribution), **Usage** (AI subscription/quota dashboard), **Models** (provider config + tool sync).

### Layering (where code goes)

```
src/                    React frontend — talks to backend ONLY via invoke() + Tauri events
src-tauri/src/commands/ Tauri command wrappers — thin, no heavy logic
src-tauri/src/core/     Tauri-specific glue (State handles, event emitters, adapters)
crates/                 Workspace crates — ALL domain logic lives here
```

Heavy logic belongs in `crates/*`, not in command wrappers. Commands are registered in `src-tauri/src/commands/mod.rs`.

### Workspace crates

- `skillstar-core` — shared types + infra (paths, fs_ops, db_pool, migrations, `http_client::probe_http_client` which honours `config/proxy.json` — all remote HTTP must use it)
- `skillstar-skills` — skill lifecycle (install/update/bundle/local authoring/repo scan) + git via `gix` and subprocess
- `skillstar-marketplace` — local-first marketplace snapshot + SQLite FTS
- `skillstar-models` — provider store, presets, external tool sync (Claude Code / Codex / OpenCode on-disk configs), latency, circuit breaker
- `skillstar-ai` — inference (chat completion, streaming summarize/translate, skill pick); depends on `skillstar-models` for provider resolution
- `skillstar-providers` — zero-dependency leaf crate: single source of truth for provider metadata (balance endpoints, auth schemes); both usage fetchers and models presets derive from it
- `skillstar-usage` — subscriptions/quota: fixed catalog, OAuth + API-key fetchers, AES-256-GCM encrypted token storage
- `skillstar-fingerprint` — TLS-fingerprint-aware HTTP client (JA3/JA4/H2 emulation via optional `wreq` feature)
- `skillstar-projects` — project registration, agent profiles, patrol, Launch Deck terminal
- `skillstar-app` — Tauri-agnostic command helpers + CLI entry point

### Frontend structure

`src/features/<domain>/` slices (components + hooks per domain: my-skills, marketplace, projects, models, usage, mcp, fingerprints, settings); `src/pages/` are thin lazy-loaded shells; cross-page navigation state is centralized in `App.tsx`. State is hook-driven with TanStack Query — no external state library. Styling is Tailwind v4 utilities only (no CSS modules / styled-components), Radix primitives, dark glassmorphism design (brand/visual direction in `PRODUCT.md`). i18n via i18next (`src/i18n/locales/en.json` + `zh-CN.json` — keep both in sync).

Streaming AI flows use Tauri events (`ai://summarize-stream`, `ai://translate-stream`) with `start`/`delta`/`complete`/`error` phases.

### Storage

Everything persists under `~/.skillstar/` (overridable via `SKILLSTAR_DATA_DIR` / `SKILLSTAR_HUB_DIR`): JSON for config (`config/`), SQLite for marketplace cache + translation cache (`db/`), skills hub (`hub/skills/`, local-authored in `hub/local/`, repo cache in `hub/repos/`). Skill distribution into projects is symlink-first with automatic copy fallback (Windows without Developer Mode). AI provider config is backend-owned (`config/ai.json`); the frontend never stores API keys.

## Code rules

- **Files must stay under ~1000 lines.** Any source file that crosses 1000 lines gets decoupled into smaller modules; keep new/edited files well under that.
- Before refactors that move or delete files, make sure uncommitted work is committed or stashed first — the working tree often carries large in-flight changes.
- Marketplace/UI data is local-first: read snapshot commands first; remote sync is an explicit follow-up, never the direct page data source.
- Do not modify `crates/skillstar-usage/src/fetchers/oauth/cursor.rs` unless explicitly requested.
