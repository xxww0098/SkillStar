# SkillStar — Web UI Framework

> This document governs the SkillStar desktop frontend. For backend rules, see [AGENTS.md](./AGENTS.md).

## Tech Stack

| Layer | Choice | Version |
|-------|--------|---------|
| **Runtime** | Node.js / Bun | latest |
| **Framework** | React + TypeScript | 18.x |
| **Build** | Vite | 5.x |
| **Styling** | TailwindCSS | 4.x |
| **Animation** | Framer Motion | 12.x |
| **Icons** | Lucide React | 0.436 |
| **Components** | Custom primitives + Radix UI | latest |
| **Desktop IPC** | @tauri-apps/api | 2.x |
| **Toasts** | Sonner | 2.x |

## Project Structure

```
SkillStar/
├── index.html                 # Entry point
├── package.json               # Frontend dependencies and scripts
├── vite.config.ts             # Vite configuration
├── tsconfig.json              # TypeScript configuration
└── src/
    ├── main.tsx               # App bootstrap + SkillsProvider wiring
    ├── App.tsx                # Root layout + routing + cross-page navigation context
    ├── index.css              # TailwindCSS theme tokens + base styles
    ├── vite-env.d.ts          # Vite global types
    ├── types/
    │   └── index.ts           # Shared TypeScript types (Skill, Project, Agent, etc.)
    ├── hooks/
    │   ├── useSkills.ts       # Installed skills CRUD + agent linking (global SkillsProvider state)
    │   ├── useAgentProfiles.ts # Agent profile listing + toggling
    │   ├── useProjectManifest.ts # Project registration + skill sync
    │   ├── useSkillGroups.ts  # Skill group CRUD + deploy
    │   ├── useMarketplace.ts  # skills.sh marketplace search
    │   ├── useAiConfig.ts     # AI provider config + translate/summarize
    │   └── useUpdater.ts      # Auto-update check/download/install hook
    ├── lib/
    │   ├── utils.ts           # Tailwind cn() helper
    │   ├── toast.ts           # Sonner toast wrapper
    │   ├── shareCode.ts       # Skill group share code encode/decode
    │   ├── backgroundStyle.ts # Global background style persistence + DOM apply
    │   ├── skillUpdateRefresh.ts # Pending-update refresh mode persistence + interval resolver
    │   └── marketplaceDescriptionHydration.ts # Marketplace description hydration helpers + patch merge
    ├── pages/
    │   ├── MySkills.tsx       # Global skill management + per-agent linking
    │   ├── Marketplace.tsx    # skills.sh marketplace browser
    │   ├── PublisherDetail.tsx # Publisher drill-down sub-page
    │   ├── SkillGroups.tsx    # Skill bundle management + deploy navigation
    │   ├── Projects.tsx       # Thin re-export wrapper
    │   └── projects-page/     # Projects page sections
    │       ├── index.tsx      # Projects page composition + state/handlers
    │       ├── DeployBanner.tsx
    │       ├── ProjectListPanel.tsx
    │       ├── ProjectDetailPanel.tsx
    │       ├── ScanImportBanner.tsx
    │       ├── AgentAccordion.tsx
    │       └── ApplyFooter.tsx
    │   ├── Settings.tsx       # Thin re-export wrapper
    │   └── settings-page/     # Settings page sections
    │       ├── index.tsx      # Settings page composition + state/handlers
    │       ├── AgentConnectionsSection.tsx
    │       ├── ProxySection.tsx
    │       ├── AiProviderSection.tsx
    │       ├── UpdateRefreshSection.tsx
    │       ├── AppearanceSection.tsx
    │       ├── LanguageSection.tsx
    │       ├── StorageSection.tsx
    │       └── AboutSection.tsx
    └── components/
        ├── ui/                # Reusable UI primitives
        │   ├── button.tsx
        │   ├── badge.tsx
        │   ├── card.tsx
        │   ├── input.tsx
        │   ├── EmptyState.tsx
        │   ├── Skeleton.tsx
        │   └── sonner.tsx
        ├── layout/
        │   ├── Sidebar.tsx    # Left navigation sidebar
        │   ├── Toolbar.tsx    # Page toolbar (search, sort, view mode)
        │   └── DetailPanel.tsx # Right-side skill detail panel
        ├── skills/
        │   ├── SkillCard.tsx  # Individual skill card with agent toggles
        │   ├── SkillGrid.tsx  # Grid/list layout for skill cards
        │   ├── SkillEditor.tsx # SKILL.md content editor
        │   ├── SkillSelectionBar.tsx # Batch selection toolbar
        │   ├── CreateGroupModal.tsx  # Create/edit deck
        │   ├── DeployToProjectModal.tsx # Quick deploy modal (used in MySkills)
        │   ├── ProjectDeployAgentDialog.tsx # Project deploy target picker with multi-agent SVG cards
        │   ├── ImportShareCodeModal.tsx  # Import deck from share code
        │   ├── ExportShareCodeModal.tsx  # Export deck as share code
        │   ├── UninstallConfirmDialog.tsx # Uninstall confirmation
        │   ├── GitHubImportModal.tsx  # GitHub repo scan + batch skill import
        │   └── RecommendedRow.tsx # Recommended skills row
        ├── marketplace/
        │   └── OfficialPublishers.tsx # Publisher cards grid
```

## Architecture

- SPA with Tauri IPC; all data flows through Rust `#[tauri::command]` handlers.
- Frontend state is managed via React hooks (`useState`, `useCallback`, `useMemo`). No external state management library.
- Skills state is centralized via `SkillsProvider` (wired in `main.tsx`) so all pages share one `useSkills` polling lifecycle.
- Pending-update refresh cadence is user-configurable from `Settings.tsx` and persisted in localStorage (`skillstar:skill-update-refresh`) with an `auto` mode.
- All data fetching uses Tauri `invoke()` calls, not HTTP requests.
- `SkillEditor.tsx` and `DetailPanel.tsx` translation preview support live streaming via Tauri events: invoke `ai_translate_skill_stream`, listen on `ai://translate-stream`, and progressively render translated markdown deltas.
- `DetailPanel.tsx` AI Quick Read summary supports live streaming via Tauri events: invoke `ai_summarize_skill_stream`, listen on `ai://summarize-stream`, and progressively render summary deltas.
- Marketplace page uses incremental description hydration: initial list render uses fast search/leaderboard payload, then missing descriptions are patched in background (first batch only) and merged into both list and DetailPanel state.
- Cross-page navigation context is managed in `App.tsx` via lifted state props (e.g., pre-selected skills for deploy, focus skill for detail panel).
- Simple client-side routing via `NavPage` union type + switch-case in `App.tsx`.

## Design System — Dark Glassmorphism

SkillStar uses a **Dark Glassmorphism** design language for a modern, layered visual experience.

### Color Palette

| Token | Value | Usage |
|-------|-------|-------|
| `background` | `#0a0a0f` | App background |
| `foreground` | `#f4f4f5` | Primary text |
| `primary` | `#3b82f6` | Primary actions, highlights |
| `primary-hover` | `#60a5fa` | Hover state |
| `card` | `rgba(255,255,255,0.05)` | Card backgrounds |
| `card-hover` | `rgba(255,255,255,0.08)` | Card hover |
| `border` | `rgba(255,255,255,0.1)` | Borders, dividers |
| `border-subtle` | `rgba(255,255,255,0.06)` | Subtle dividers |
| `muted` | `rgba(255,255,255,0.08)` | Muted backgrounds |
| `muted-foreground` | `#a1a1aa` | Secondary text |
| `accent` | `rgba(59,130,246,0.15)` | Selection, focus |
| `sidebar` | `rgba(255,255,255,0.03)` | Sidebar background |
| `sidebar-hover` | `rgba(255,255,255,0.06)` | Sidebar hover |
| `sidebar-active` | `rgba(59,130,246,0.15)` | Sidebar active |

### Glass Effects

```css
/* Base glass */
.glass {
  background: rgba(255, 255, 255, 0.05);
  backdrop-filter: blur(20px);
  border: 1px solid rgba(255, 255, 255, 0.1);
}

/* Ambient glow (modal header decoration) */
.ambient-glow::before {
  content: '';
  position: absolute;
  top: -40px;
  left: -40px;
  width: 120px;
  height: 120px;
  background: radial-gradient(circle, rgba(59, 130, 246, 0.2) 0%, transparent 70%);
  border-radius: 50%;
  filter: blur(40px);
  pointer-events: none;
}
```

### Component Patterns

**Modal/Dialog** — Full-screen overlay with centered card:
```tsx
// Backdrop
<div className="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm" />

// Card container
<div className="relative overflow-hidden rounded-[24px] border border-white/10 bg-card/95 shadow-[0_0_80px_-20px_rgba(0,0,0,0.5)] backdrop-blur-3xl ring-1 ring-white/5">
  // Ambient glows
  <div className="pointer-events-none absolute -left-20 -top-20 h-48 w-48 rounded-full bg-primary/20 blur-[60px] opacity-70" />
  <div className="pointer-events-none absolute -right-20 -top-20 h-48 w-48 rounded-full bg-blue-500/10 blur-[60px] opacity-70" />
</div>
```

**Card** — Glass card with hover lift:
```tsx
<div className="rounded-[24px] border border-white/10 bg-card/50 backdrop-blur-sm shadow-[0_4px_20px_-8px_rgba(0,0,0,0.3)] hover:bg-card-hover/60" />
```

**Button Variants** — Glass-styled buttons:
```tsx
// Default: primary glass
"bg-primary/80 backdrop-blur-sm rounded-2xl"

// Outline: subtle border
"border-white/10 bg-white/5 hover:bg-white/10 rounded-2xl"

// Ghost: transparent
"hover:bg-white/10 rounded-2xl"
```

**Input** — Glass input:
```tsx
<input className="bg-white/5 border-white/10 backdrop-blur-sm rounded-xl focus:border-primary" />
```

### Radius Scale

| Token | Value |
|-------|-------|
| `radius-sm` | `6px` |
| `radius-md` | `8px` |
| `radius-lg` | `12px` |
| `radius-xl` | `16px` |

Large components (cards, modals) use `rounded-[24px]` or `rounded-3xl`.

### Animations

- **Modal entrance**: spring bounce `scale: 0.96→1, y: 12→0, duration: 0.35`
- **Modal backdrop**: fade `opacity: 0→1, duration: 0.15`
- **Panel slide**: spring `x: 100%→0, duration: 0.3`
- **Hover transitions**: `transition-all duration-200`

## Conventions

- **Components**: Use custom UI primitives from `components/ui/` first; Radix UI for accessible interactions (dialog, tooltip, select).
- **Styling**: TailwindCSS 4 utility classes. No inline styles. No CSS modules.
- **State**: Local React state via hooks. No external state library.
- **Types**: TypeScript. Shared types in `src/types/index.ts`.
- **Icons**: Lucide React exclusively.
- **Animations**: Framer Motion for transitions and micro-interactions.

## SkillStar Desktop Conventions

- Desktop routes live under `src/pages/` and include `MySkills.tsx`, `Marketplace.tsx`, `PublisherDetail.tsx`, `SkillGroups.tsx`, `Projects.tsx`, and `Settings.tsx`.
- `PublisherDetail.tsx` uses a two-step drill-down: first show repository list for the publisher, then show skill cards after selecting a repository.
- `Marketplace.tsx` and `PublisherDetail.tsx` must share the same description hydration utilities (`src/lib/marketplaceDescriptionHydration.ts`) to keep key normalization and patch merge behavior consistent.
- Agent connection management (enable/disable agents) lives in `Settings.tsx` under "Agent Connections". AI provider configuration (API key, model, base URL) lives in `Settings.tsx` under "AI Provider". Per-skill-per-agent linking is controlled from skill cards in `My Skills`.
- New skills default to NOT being linked to any agent. Users explicitly toggle per-agent links from skill cards.
- Skill-management overlays live under `src/components/skills/`. Destructive flows should use dedicated confirmation components rather than native browser `confirm()`.
- `My Skills` uninstall interactions must require explicit user confirmation before deleting local skills.
- `My Skills` should render from a fast local skill snapshot first; remote update badges may hydrate asynchronously after initial content is visible.
- `Projects.tsx` is a top-level page with master-detail two-panel layout for project skill management. The left panel shows a searchable project list; the right panel shows per-project agent configuration and skill assignment.
- `Projects.tsx` Apply must reconcile removals as well as additions. Turning an agent off for a project should delete that agent's old project-level symlinks on disk and remove empty agent config folders that were created just for sync output.
- When `Projects.tsx` is opened from Decks deploy with pending skills, clicking a project should open a multi-select agent deploy dialog that shows each target agent's SVG icon and project skill path before staging the skills into that project's config.
- `Projects.tsx` and project deploy agent pickers must only render globally enabled agents (`Settings.tsx` toggle on). Disabled agents should be hidden from project configuration and deploy target selection.
- Cross-page navigation context is managed in `App.tsx`: SkillGroups "Deploy" navigates to Projects with pre-selected skills; clicking a skill in Projects navigates to MySkills and auto-opens its DetailPanel.
- The old `ProjectSkillManager` modal is no longer used in SkillGroups; project management is handled exclusively by the Projects page.

## Maintenance Rules

- **Frontend Document-First**: Update `AGENTS_UI.md` with new components, pages, or structural changes before writing frontend code.
- **Backend Document-First**: Update `AGENTS.md` with new architectures, flows, or structural changes before writing backend code.
- **Directory Sync**: Keep the `Project Structure` tree in sync with actual project state.

## Do NOT

- **Do NOT** use CSS modules or styled-components — use TailwindCSS exclusively.
