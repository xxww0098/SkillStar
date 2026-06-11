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
│   ├── models/                   # provider config + agent activation hub
│   │   ├── index.ts              # public API — cross-feature imports go through here only
│   │   ├── types.ts              # domain types (form values, agent status, drawer mode)
│   │   ├── api/                  # the ONLY IPC surface (TanStack Query wrappers + modelsKeys factory)
│   │   ├── lib/                  # pure functions (agentRegistry, agentStatus, providerPatch, launchCommand, …)
│   │   ├── hooks/                # React composition (useProviderForm, useAutosave, useAgentActivation, useAgentHealth)
│   │   └── components/
│   │       ├── hub/              # ModelsHub (thin container), ProviderGallery, gallery cards
│   │       ├── agents/           # agent cards + AgentSettingsDialog + per-agent forms
│   │       ├── provider/         # ProviderEditorDrawer + tabs/ + PresetPicker
│   │       ├── diagnostics/      # ConnectionStatusPanel, EndpointSpeedPanel, ConflictWarnings
│   │       └── shared/           # DrawerShell, brand icons, SaveBadge, Provider/Model select popovers
│   ├── mcp/                      # MCP server management + marketplace browsing
│   │   ├── components/           # McpManager, McpServerCard, McpServerForm, McpMarketBrowser
│   │   └── hooks/                # useMcpServers, useMcpPresets, useMcpMarketplace
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

The Models mode follows a strict "职责分离" IA: **agent activation has exactly one entry point (the agent cards row)**; the provider drawer manages ONLY provider data. Tool-specific settings shown on the agent side (Claude tier mapping, Codex wire_api/auth_mode) still persist on the provider record via `update_provider_flat` — only their presentation moved.

- The Models mode is a **single hub page** (`pages/Models.tsx` → `features/models/components/hub/ModelsHub.tsx`). The old four-subpage split has been removed; `#models/<sub>` hashes redirect to `#models`.
- Hub layout (top-down):
  1. **Hero header**: "模型工作台" title + `新增供应商` CTA.
  2. **Agent cards section** ("AGENT 接入") — the ONLY place to activate/deactivate/re-sync agents. One `AgentHeroCard` per provider agent from `lib/agentRegistry.ts` (`claude-code` / `codex` / `opencode` / `gemini`), plus `ClaudeDesktopCard` (MCP config, no provider binding) and `AppAiCard` (in-app AI binding, replaces the old `AppAiProviderInline` inside the drawer). The section header shows the aggregate "x/y 已接入" summary — there is no separate HealthBar strip; connection probes live in `hooks/useAgentHealth.ts` (probe once per `(toolId, providerId)` pair, click the status pill to retest) and statuses come from `lib/agentStatus.ts` (the canonical status model; latency colors via `lib/latencyColor.ts`).
  3. **Provider gallery**: search input + responsive `ProviderGalleryCard` grid with hover menu (duplicate / delete-with-confirmation).
- **Agent settings dialog** (`agents/AgentSettingsDialog.tsx`, ModalShell 640px): per-agent deep configuration — provider/model binding, agent-conditional params (`ClaudeModelMapping` for claude-code; `CodexSettingsForm` for codex), Claude launch command (`lib/launchCommand.ts`), on-disk config file editor (`AgentConfigFiles`, single-tool mode), conflict warnings (tool-filtered), last-sync time + re-sync, deactivate. Activation flows through `hooks/useAgentActivation.ts` — the single activation path shared with the cards.
- **`ProviderEditorDrawer`** (`provider/ProviderEditorDrawer.tsx`, DrawerShell `max-w-[640px]`): edit mode is a TABBED form — 连接 (name / API key / dual base URLs / models_url) · 模型 (fetch + model list management + default model) · 高级 (runtime params / notes) · 诊断 (`ConnectionStatusPanel` + `EndpointSpeedPanel` + disk config). The drawer owns the autosave state machine: `hooks/useProviderForm.ts` (one reducer-managed values object; pure conversions in `lib/providerPatch.ts`) + `hooks/useAutosave.ts` (600ms debounce, validation-aware re-arm, **flush-on-close** so pending edits are never silently dropped). The save badge (`shared/SaveBadge`) renders ONLY here — the hub no longer mirrors save state.
- Create mode renders `PresetPicker` (category-tiled preset grid → API key + base URL → 创建并继续) inside a plain `DrawerShell`, then pivots to the editor drawer with the created provider showing a one-time `PostCreateGuide` banner (添加供应商 → 测试连接 → 接入 Agent step trail; auto-fetches the model catalog once when credentials allow). Provider deletion always confirms via `DeleteProviderDialog`, listing the agents that would disconnect.
- **Models state management**: all IPC goes through `api/` — `modelsKeys` factory (`api/keys.ts`), providers query + CRUD mutations (`api/providers.ts`), activation map + mutations (`api/activations.ts`, selected from the providers-flat cache — `tool_activations` is NEVER fetched separately), install detection query (`api/install.ts`, 5min stale). Mutations follow one convention: optimistic onMutate → rollback + toast onError → invalidate onSettled; `create` seeds the cache from the returned entity. devMock covers all models write commands (stateful `FLAT_PROVIDERS`), so the full create → edit → activate → delete flow works in plain-Vite browser dev.
- Built-in provider presets are loaded via `get_provider_presets_flat`; do not duplicate preset lists in TypeScript.
- Settings `AiProviderSection` toggles **Models 供应商** vs **本地 Ollama** via `AppAiModelsPicker`; the hub's `AppAiCard` is a shortcut for the Models-provider source only and defers to Settings when Ollama is active.
- Provider `meta.timeout` is applied to AI HTTP clients at resolve time (not stored in `ai.json`).
- `update_provider_flat` returns `tool_sync_results`; the api layer toasts when re-sync fails.
- Endpoint probe: OpenAI bases use `GET /models`; URLs containing `/anthropic` use `POST /messages` (avoids false 404 on DeepSeek Anthropic gateway). Same logic for **端点测速** and **深度连接测试** (empty model).
- Agent registry facts (`lib/agentRegistry.ts` — extend THERE when adding an agent, see ADDING-AN-AGENT.md):
  - `claude-code` writes `~/.claude/settings.json` (Claude Code CLI only — Anthropic's standalone desktop app stores config elsewhere and is not synced).
  - `codex` writes `~/.codex/config.toml` + `~/.codex/auth.json` — the same `~/.codex/` directory is read by the Codex CLI, the `codex app` desktop experience, and the official VS Code / Cursor / Windsurf IDE extensions, so a single Codex binding covers every form-factor.
  - `opencode` writes `~/.config/opencode/opencode.json`; `gemini` writes `~/.gemini/.env`.
- **Sidebar in Models mode** renders the minimal `ModelsSidebar`: intro card with `新增供应商` + a "最近" rail of up to 6 providers. Clicking a recent provider requests the edit drawer via the `modelsDrawerRequest` navigation event (request-nonce pattern, like `usageCreateRequest`).
- **CommandPalette** in Models mode exposes a single `Models 工作台` action.

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
- Centered glassmorphism modals must use `components/ui/ModalShell` (`ModalShell` + `ModalHeader` + `ModalCloseButton`) instead of hand-rolling AnimatePresence/backdrop/`modal-surface` scaffolding. Exceptions: Radix `AlertDialog`-based dialogs (keep Radix focus/Escape semantics) and dialogs with intentionally custom surfaces.
- Tauri event subscriptions tied to component lifetime must use `hooks/useTauriEvent` (handles the `listen()` promise/cleanup race); only imperative per-request streams (`useAiStream`, `useAiTranslate`) manage listeners manually.
- "Global-only agent" checks must be data-driven via `lib/agentProfiles.supportsProjectDeploy` (empty `project_skills_rel`), never hard-coded agent ids.
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
