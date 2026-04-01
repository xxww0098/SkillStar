# SkillStar — Web UI Framework

> Frontend guide for SkillStar desktop. Backend rules are in [AGENTS.md](./AGENTS.md).

## Tech Stack
| Layer | Choice | Version |
|---|---|---|
| Runtime | Node.js / Bun | latest |
| Framework | React + TypeScript | 19.x |
| Build | Vite | 5.x |
| Styling | TailwindCSS, shadcn/ui | 4.x |
| Animation | Framer Motion | 12.x |
| Icons | Lucide React | 0.436 |
| Components | Custom primitives + Radix UI | latest |
| Desktop IPC | `@tauri-apps/api` | 2.x |
| Data/State | TanStack Query | 5.x |
| Toasts | Sonner | 2.x |

## Project Structure (Condensed)
```text
src/
├── main.tsx                      # app bootstrap + provider wiring
├── App.tsx                       # layout + routing + cross-page state
├── features/                     # domain slices (components + hooks)
│   ├── my-skills/                # skill grid, cards, modals, install/export
│   │   ├── components/           # SkillGrid, SkillCard, ImportModal, …
│   │   └── hooks/                # useSkills, useSkillCards
│   ├── marketplace/              # marketplace browsing
│   │   ├── components/           # OfficialPublishers
│   │   └── hooks/                # useMarketplace
│   ├── projects/                 # project registration + agent config
│   │   ├── components/           # AgentAccordion, ProjectDetailPanel, …
│   │   └── hooks/                # useProjectManifest, useProjectSkills, …
│   ├── security/                 # security scanning
│   │   ├── components/           # RadarSweep, ScanFilePanel
│   │   └── hooks/                # useSecurityScan
│   └── settings/                 # app settings
│       └── sections/             # AboutSection, AiProviderSection, …
├── pages/                        # thin route-level shells (lazy-loaded)
│   ├── projects-page/            # Projects page shell
│   └── settings-page/            # Settings page shell
├── hooks/                        # global-only hooks (useNavigation, useUpdater, useAiConfig)
├── components/
│   ├── ui/                       # shared atomic primitives
│   ├── layout/                   # Sidebar/Toolbar/DetailPanel
│   └── shared/                   # cross-feature: SkillEditor, SkillReader
├── lib/                          # utils, toast, share code
└── types/                        # shared TS types
```

## Architecture Rules
- Frontend data must flow through Tauri `invoke()` commands and Tauri events.
- State is hook-driven (`useState` / `useMemo` / `useCallback`) with shared skill state from `SkillsProvider`.
- No external state-management library unless explicitly justified.
- Keep cross-page deploy/detail navigation state centralized in `App.tsx`.
- Marketplace pages must read local-first snapshot commands from Tauri and treat remote sync as an explicit follow-up refresh, not a direct page data source.
- Marketplace UI should surface snapshot freshness/seeding state when relevant instead of hydrating missing descriptions in the browser.
- Marketplace drill-down screens (`PublisherDetail`, `DetailPanel`) should reuse the same local-first snapshot flow as the main marketplace page.
- Settings storage/location UI must use backend-resolved paths instead of frontend path reconstruction.

## Streaming UX Rules
- Skill translation: invoke `ai_translate_skill_stream`, listen to `ai://translate-stream` events.
- Quick summary: invoke `ai_summarize_skill_stream`, listen to `ai://summarize-stream` events.
- Event phases are `start` / `delta` / `complete` / `error`; UI should render incrementally and handle interruption safely.
- Durable translation reuse is backend-owned via SQLite cache; frontend translation state may only cache the active panel/session, not replace backend cache decisions.
- `Retranslate via AI` UI must request an AI-only refresh, not the generic priority/fallback path.
- AI skill pick UI should render backend-provided relevance order as-is and surface lightweight explanation metadata (for example score or reason) rather than re-sorting or hiding why a skill was recommended.
- Security scan progress must distinguish file-prep progress from AI chunk progress; concurrent worker state should be visible in the scanning UI rather than collapsed to a single active skill.

## Visual System (Dark Glassmorphism)
### Core Tokens
| Token | Value | Usage |
|---|---|---|
| `background` | `#0a0a0f` | app background |
| `foreground` | `#f4f4f5` | main text |
| `primary` | `#3b82f6` | primary action |
| `card` | `rgba(255,255,255,0.05)` | glass card bg |
| `border` | `rgba(255,255,255,0.1)` | borders/dividers |
| `muted-foreground` | `#a1a1aa` | secondary text |

### Component Direction
- Cards and dialogs use translucent layers + backdrop blur + subtle border.
- Large surfaces prefer `rounded-3xl` / `rounded-[24px]`; compact controls use smaller radius scale.
- Motion should be purposeful: entry, exit, and hover transitions only.
- Respect `prefers-reduced-motion` and keep AA contrast.

## Conventions
- Styling: TailwindCSS utilities only.
- Components: prefer `components/ui/*` primitives; use Radix for accessibility-heavy patterns.
- Types: shared types in `src/types/index.ts`.
- Icons: Lucide React.
- Avoid inline style unless dynamic value cannot be expressed with utility classes.

## Desktop UX Conventions
- Pages include `MySkills`, `Marketplace`, `PublisherDetail`, `SkillCards`, `Projects`, `Settings`.
- `Projects` is master-detail and must reconcile removals as well as additions.
- Only globally enabled agents should appear in project deploy target pickers.
- Shared project-path conflicts must be single-owner at selection time.
- Destructive skill actions use explicit confirmation components, not browser `confirm()`.
- New skills default to not linked to any agent until user toggles.
- Background-run preference must flow through shared helpers/events so tray actions and Settings switches render the same patrol state.
- Tray background actions should use stateful labels (`Start` / `Stop`) instead of a static one-way action label.

## Maintenance Rules
- Frontend structure/convention changes must update this file first.
- Backend architecture changes must update `AGENTS.md` first.
- Keep structure sections aligned with real filesystem layout.

## Do NOT
- Do not use CSS modules or styled-components.
- Do not bypass backend commands with direct network fetches for app data flows.
