# SkillStar — Web UI Framework

> Frontend guide for SkillStar desktop. Backend rules are in [AGENTS.md](./AGENTS.md).

## Tech Stack
| Layer | Choice | Version |
|---|---|---|
| Runtime | Node.js / Bun | latest |
| Framework | React + TypeScript | 18.x |
| Build | Vite | 5.x |
| Styling | TailwindCSS | 4.x |
| Animation | Framer Motion | 12.x |
| Icons | Lucide React | 0.436 |
| Components | Custom primitives + Radix UI | latest |
| Desktop IPC | `@tauri-apps/api` | 2.x |
| Toasts | Sonner | 2.x |

## Project Structure (Condensed)
```text
src/
├── main.tsx                      # app bootstrap + provider wiring
├── App.tsx                       # layout + routing + cross-page state
├── hooks/                        # useSkills/useProjectManifest/useMarketplace/useAiConfig/useUpdater...
├── pages/                        # MySkills, Marketplace, PublisherDetail, SkillCards, Projects, Settings
│   ├── projects-page/            # project page sections
│   └── settings-page/            # settings page sections
├── components/
│   ├── ui/                       # shared primitives
│   ├── layout/                   # Sidebar/Toolbar/DetailPanel
│   ├── skills/                   # cards, editor, dialogs, import/export/deploy
│   └── marketplace/              # OfficialPublishers
├── lib/                          # utils, toast, share code, hydration helpers
└── types/                        # shared TS types
```

## Architecture Rules
- Frontend data must flow through Tauri `invoke()` commands and Tauri events.
- State is hook-driven (`useState` / `useMemo` / `useCallback`) with shared skill state from `SkillsProvider`.
- No external state-management library unless explicitly justified.
- Keep cross-page deploy/detail navigation state centralized in `App.tsx`.
- Marketplace description hydration must reuse `src/lib/marketplaceDescriptionHydration.ts`.
- Settings storage/location UI must use backend-resolved paths instead of frontend path reconstruction.

## Streaming UX Rules
- Skill translation: invoke `ai_translate_skill_stream`, listen to `ai://translate-stream` events.
- Quick summary: invoke `ai_summarize_skill_stream`, listen to `ai://summarize-stream` events.
- Event phases are `start` / `delta` / `complete` / `error`; UI should render incrementally and handle interruption safely.
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

## Maintenance Rules
- Frontend structure/convention changes must update this file first.
- Backend architecture changes must update `AGENTS.md` first.
- Keep structure sections aligned with real filesystem layout.

## Do NOT
- Do not use CSS modules or styled-components.
- Do not bypass backend commands with direct network fetches for app data flows.
