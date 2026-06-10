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

## Models Provider UI
- The Models mode is a **single hub page** (`pages/Models.tsx` → `features/models/components/hub/ModelsHub.tsx`). The old four-subpage split (agent-connections / providers / health / tool-configs) has been removed; the `#models/<sub>` hashes redirect to `#models` and `ModelsNavPage` is a single literal `"hub"` kept for back-compat.
- Hub layout (top-down):
  1. **Hero header**: "模型工作台" title + `新增供应商` CTA.
  2. **`HealthBar`**: a sticky status strip with one chip per agent (`claude-code` / `codex` / `opencode`); each chip auto-probes via `tauriInvoke("test_provider_connection")` on mount and supports click-to-retest.
  3. **`AgentHeroCard` row** (3 cards): brand status pill, accent strip, Radix `Popover` provider picker + model picker, footer actions (resync / open drawer / disconnect).
  4. **Provider gallery**: search input + responsive `ProviderGalleryCard` grid (`sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4`) with hover-revealed `MoreHorizontal` menu (duplicate / delete) — replaces the old left-side `ModelsNav` list.
- **`ProviderDrawer`**: Radix `Dialog` slide-in from the right (`max-w-[560px]`, Framer Motion `[0.22,1,0.36,1]`). Single component handles both **create** and **edit** flows.
  - Create mode renders `PresetPicker` (category-tiled preset grid → fill API Key + Base URL → 创建并继续). After creation the drawer pivots into edit mode without unmounting.
  - Edit mode renders `ProviderDrawerForm` — an `Accordion` of `ConfigCollapseSection`s: 连接 / Agent 同步 / Codex 默认参数 / Claude 模型映射 / 运行参数 / 连接诊断与余额 / 磁盘配置文件 / 附加信息. Only one section is expanded at a time.
  - `ProviderSaveState` now lives in `providerForm/useProviderFormState.ts` (used to live in `ProviderConfigForm`). The drawer header shows a save badge and the footer shows the live status.
- Built-in provider presets are loaded via `get_provider_presets_flat` (Rust `get_all_presets_flat()`); do not duplicate preset lists in TypeScript.
- `AppAiProviderInline` + `set_app_ai_provider_ref` bind flat-store providers to `ai.json` for in-app AI (summarize / translate / skill pick).
- Settings `AiProviderSection` toggles **Models 供应商** vs **本地 Ollama** via `AppAiModelsPicker`.
- Provider `meta.timeout` is applied to AI HTTP clients at resolve time (not stored in `ai.json`).
- `update_provider_flat` returns `tool_sync_results`; the UI toasts when re-sync fails.
- Endpoint probe: OpenAI bases use `GET /models`; URLs containing `/anthropic` use `POST /messages` (avoids false 404 on DeepSeek Anthropic gateway). Same logic for **端点测速** and **深度连接测试** (empty model).
- Endpoint probe lives under the drawer's **附加信息** section (`EndpointSpeedPanel`); connection test/balance lives in the **连接诊断与余额** section (`ConnectionStatusPanel`).
- `ToolActivationPanel` supports `claude-code`, `codex`, and `opencode`.
  - `claude-code` writes `~/.claude/settings.json` (Claude Code CLI only — Anthropic's standalone Mac/Windows app stores config elsewhere and is not synced).
  - `codex` writes `~/.codex/config.toml` + `~/.codex/auth.json`. Per the [OpenAI Codex repo](https://github.com/openai/codex), **the same `~/.codex/` directory is read by the Codex CLI, the `codex app` desktop experience, and the official VS Code / Cursor / Windsurf IDE extensions** — so a single Codex binding here covers every Codex form-factor. The historical "cli" badge on `AgentToolIcon` has been removed to stop falsely implying CLI-only support.
  - `opencode` writes `~/.opencode/opencode.json`.
- In-app AI binding (`AppAiProviderInline` **OpenAI** button) is separate from Codex tool sync.
- **Sidebar in Models mode** renders the minimal `ModelsSidebar` (NOT the old `ModelsNav`): a "MODELS 工作台" intro card with `新增供应商` button + a "最近" rail of up to 6 recently sorted providers. Clicking a recent provider opens the drawer in edit mode via `selectedProviderId`.
- **CommandPalette** in Models mode exposes a single `Models 工作台` action (jumps into the hub) — the old `Providers / Health / Tool Configs / Models Settings` palette entries are removed.

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
- Translation UI should treat routing as backend-owned: display route mode, provider, and fallback state from payload metadata instead of inferring engine choice on the client.
- Durable translation reuse is backend-owned via SQLite cache; frontend translation state may only cache the active panel/session, not replace backend cache decisions.
- `Retranslate via AI` UI must request an AI-only refresh, not the generic priority/fallback path.
- Translation controls in `SkillEditor` and `SkillReader` should separate cached/toggle behavior from explicit retranslation: the main translate button may show or hide an existing result, while the refresh control sends `forceRefresh`.
- Translation completion UI should display backend-reported model throughput metrics when available; do not estimate TPS in TypeScript when provider usage is absent.
- Settings should expose translation as a single Translation Center with readiness states and simple mode choices; detailed provider credentials and diagnostics belong in advanced drawers, not in the primary flow.
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

### Configuration InfoTips
- UI fields for model and core behaviors must include an `InfoTip` (`[?]` hover) to explain the configuration.
- For options-based inputs like `SegmentPill`, descriptions must use a newline-separated dictionary format. Example: `"Description summary:\n\nOption1: explanation\nOption2: explanation"`.
- The `InfoTip` renderer automatically parses this `Label: ` syntax to apply highlighted typography (`font-bold text-foreground`) to the option names, creating a consistent structural visual hierarchy without requiring inline JSX.

## Conventions
- Styling: TailwindCSS utilities only.
- Components: prefer `components/ui/*` primitives; use Radix for accessibility-heavy patterns.
- Types: shared types in `src/types/index.ts`.
- Icons: Lucide React.
- Avoid inline style unless dynamic value cannot be expressed with utility classes.
- External navigation must use `components/ui/ExternalAnchor` for link elements and `openExternalUrl` for buttons/programmatic flows; avoid raw `<a target="_blank">` in app views.

## Desktop UX Conventions
- Pages include `MySkills`, `Marketplace`, `PublisherDetail`, `SkillCards`, `Projects`, `Mcp`, `Settings`.
- Marketplace is the unified discovery surface, but skill discovery and MCP discovery stay visually separated inside the same left-aligned category rail: skill tabs (`all` / `trending` / `hot` / `official`) stay grouped under `Skill`, and the GitHub MCP registry entry stays grouped under `MCP`. MCP marketplace cards should follow the same card template, 320px grid column baseline, grid/list layout toggle, and toolbar layout controls as skill cards. The `Mcp` page is for installed MCP server management, recommended presets, and tool sync only; do not embed a separate MCP marketplace inside it.
- `Projects` is master-detail and must reconcile removals as well as additions.
- Only globally enabled agents should appear in project deploy target pickers.
- Shared project-path conflicts must be single-owner at selection time.
- Destructive skill actions use explicit confirmation components, not browser `confirm()`.
- New skills default to not linked to any agent until user toggles.
- Background-run preference must flow through shared helpers/events so tray actions and Settings switches render the same patrol state.
- Tray background actions should use stateful labels (`Start` / `Stop`) instead of a static one-way action label.
- Agent path fields in Settings should display platform-native separators for editing, while `project_skills_rel` remains backend-canonicalized with forward slashes.

## Maintenance Rules
- Frontend structure/convention changes must update this file first.
- Backend architecture changes must update `AGENTS.md` first.
- Keep structure sections aligned with real filesystem layout.

## Do NOT
- Do not use CSS modules or styled-components.
- Do not bypass backend commands with direct network fetches for app data flows.
