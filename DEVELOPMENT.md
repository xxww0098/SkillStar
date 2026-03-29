# Development

## Prerequisites

| Tool | Version | Purpose |
|------|---------|---------|
| [Bun](https://bun.sh) | latest | Package manager + dev server |
| [Rust](https://rustup.rs) | 1.85+ | Tauri backend |
| [Node.js](https://nodejs.org) | 18+ | TypeScript toolchain |

Optional:
- [gh CLI](https://cli.github.com) — needed for `skillstar publish`

## Setup

```bash
git clone https://github.com/your-org/skillstar.git
cd skillstar
bun install
```

## Commands

| Command | Description |
|---------|-------------|
| `bun run dev` | Frontend-only dev server (localhost:1420) |
| `bun run tauri dev` | Full desktop app (frontend + Rust backend) |
| `bun run build` | Typecheck + build frontend to `dist/` |
| `bun run tauri build` | Production build → `.dmg` / `.exe` / `.AppImage` |
| `bun run check` | Typecheck + lint |

### Tauri CLI

```bash
bun run tauri dev          # Start dev
bun run tauri build        # Production bundle
bun run tauri build --debug  # Debug build (faster, larger binary)
```

## Project Structure

```
skillstar/
├── src/                        # React frontend
│   ├── components/             # UI components
│   │   ├── ui/                 # shadcn/ui primitives
│   │   ├── layout/             # Sidebar, Toolbar, DetailPanel
│   │   ├── skills/             # SkillCard, SkillGrid, RecommendedRow
│   │   └── providers/          # ProviderList
│   ├── hooks/                  # React hooks
│   ├── pages/                  # Page components
│   └── lib/                    # Utilities
├── src-tauri/
│   ├── src/
│   │   ├── main.rs             # CLI + GUI entry
│   │   ├── commands.rs         # Tauri invoke commands
│   │   ├── cli.rs              # clap CLI subcommands
│   │   └── core/               # Business logic
│   │       ├── skill.rs
│   │       ├── git_ops.rs      # gix: tree-hash, clone, pull
│   │       ├── provider.rs     # Provider management
│   │       ├── sync.rs         # Skill ↔ Agent symlink sync
│   │       └── marketplace.rs  # GitHub API marketplace
│   ├── tauri.conf.json
│   └── Cargo.toml
├── package.json
└── index.html
```

## Debugging

### Frontend

- Open DevTools: right-click → Inspect Element
- React DevTools: install browser extension
- Logs: `console.log` appears in DevTools

### Rust Backend

```bash
# Enable debug logging
RUST_LOG=debug bun run tauri dev

# Trace-level for specific module
RUST_LOG=skillstar::core::git_ops=trace bun run tauri dev
```

### Tauri IPC

- Check browser console for `[TAURI]` prefixed messages
- Use `tauri-plugin-log` for structured backend logging

## Key Architecture Notes

- **Skill detection**: Uses Git tree-hash (via gix/gitoxide) to detect updates, not file timestamps
- **Zero system deps**: All Git operations via pure Rust gix, no `git` CLI required
- **Symlink sync**: Skills shared across providers via OS symlinks (not copies)
- **CLI + GUI dual mode**: Same binary, clap for CLI args, Tauri for GUI

## Adding a New Provider

1. Add variant to `Provider` enum in `src-tauri/src/core/provider.rs`
2. Implement config path detection
3. Add to `Settings.tsx` provider list
4. Update `AGENTS.md` provider table

## Adding a New UI Component

1. Check if shadcn/ui has it: `bunx shadcn@latest add <component>`
2. Place custom components in `src/components/`
3. Follow existing patterns (see `AGENTS_UI.md`)
4. Use `cn()` from `src/lib/utils` for className merging

## Performance Profiling

### Frontend

```bash
# Build with source maps, then use Chrome DevTools Performance tab
bun run dev
```

### Rust

```bash
# Build with debug symbols
bun run tauri build --debug

# Use flamegraph or cargo-instruments
cargo install flamegraph
cargo flamegraph --bin skillstar
```
