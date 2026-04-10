# Windows Error Log

Windows-specific bug investigations and fixes, extracted from `docs/Error.md`.

### Windows Launch Deck Single Mode Could Fail with `0x80070002` (`bash ... .sh`) - 2026-04-10
- Symptom: Launch Deck `single` mode failed on some Windows machines with `0x80070002` (`系统找不到指定的文件`) while trying to open `bash <temp>/ss-launch-*.sh`.
- Root cause: Single mode always generated a `.sh` script and Windows launcher always used `bash`, which breaks when Git Bash/WSL bash is unavailable.
- Fix: Added platform-aware single-mode launch path on Windows: generate `.ps1` script and launch with `pwsh`/`powershell`. Multi mode remains bash/tmux-based and now reports a clear missing-bash error.
- Files: `src-tauri/src/core/terminal_backend.rs`, `src-tauri/src/cli.rs`, `AGENTS.md`

### Windows External Link Click Could Trigger Double Browser Open - 2026-04-10
- Symptom: Clicking external links in detail/modals/pages could trigger duplicate browser/system open actions on Windows (user-visible as two open prompts/buttons in some environments).
- Root cause: External links were handled by two parallel frontend paths: (1) startup-level global document click interception and (2) scattered per-view `<a target="_blank">` behavior / custom handlers. Mixed handling created duplicate trigger opportunities.
- Fix: Removed startup global external-link interceptor and unified all external link openings through one shared path:
  - `ExternalAnchor` for anchor links
  - `openExternalUrl` / `handleExternalAnchorClick` for programmatic and anchor-click handling
  - Markdown link renderer now also routes through the same shared external-open flow
- Files: `src/lib/externalOpen.ts`, `src/components/ui/ExternalAnchor.tsx`, `src/main.tsx`, `src/lib/markdown.tsx`, `src/components/layout/DetailPanel.tsx`, `src/features/models/components/ClaudeConfigPanel.tsx`, `src/features/my-skills/components/PublishSkillModal.tsx`, `src/features/settings/components/AddCustomAgentDialog.tsx`, `src/pages/PublisherDetail.tsx`, `AGENTS-UI.md`

### Windows Models Background Probe Could Flash Console (`opencode`) - 2026-04-10
- Symptom: On Windows, opening the Models page could still trigger brief terminal flashes during OpenCode capability/provider probes.
- Root cause: `get_opencode_cli_models` and `get_opencode_auth_providers` used direct `std::process::Command::new("opencode")`, bypassing the shared Windows no-window spawn flags.
- Fix: Switched both probes to `core::path_env::command_with_path("opencode")`, which keeps PATH enrichment and applies `CREATE_NO_WINDOW` on Windows.
- Files: `src-tauri/src/commands/models.rs`

### Windows OAuth Button Could Open File Explorer Instead Of Browser - 2026-04-10
- Symptom: Clicking OAuth authorization buttons in Models could open a local File Explorer window instead of the browser authorization page.
- Root cause: `open_external_url` on Windows preferred `explorer <url>`. For some OAuth URLs with long query strings, Explorer can treat the argument like a path/navigation target instead of a web URL.
- Fix: Changed Windows external open order to prefer `rundll32 url.dll,FileProtocolHandler <url>` and keep `explorer` only as fallback.
- Files: `src-tauri/src/commands.rs`

### Windows Models Navigation Could Spawn Terminal (`~/.bun/bin/open`) 闁?2026-04-10
- Symptom: On Windows, opening external links from models/settings flows (and sometimes after navigation into Models page) could pop a terminal window titled like `C:\\Users\\<user>\\.bun\\bin\\open`.
- Root cause: Frontend external URL opens depended on `@tauri-apps/plugin-shell` `open(...)`, which can resolve to PATH-provided `open` executables in some environments. Also, global click interception was attached directly at startup without a global idempotent guard.
- Fix: Added backend-native `open_external_url` command and routed all frontend external URL opens through it. Windows now uses `explorer` (fallback `rundll32 url.dll,FileProtocolHandler`) so it no longer relies on PATH `open`. Added a guarded single-instance external-link interceptor and a short duplicate-open suppression window.
- Files: `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`, `src/lib/externalOpen.ts`, `src/main.tsx`, `BehaviorStrip.tsx`, `ModelsPanel.tsx`, `useCodexAccounts.ts`, `useGeminiOAuth.ts`, `AboutSection.tsx`

### Windows Force Delete Spinner Could Stall During Storage Refresh 閳?2026-04-04
- Symptom: On Windows, force-deleting installed skills or repo cache could keep showing a loading spinner for a long time (sometimes appearing stuck).
- Root cause: Settings force-delete flow awaited `get_storage_overview` before clearing the button loading state, and storage size walkers followed symlink/junction targets, which can trigger expensive or cyclic scans through repo-linked directories.
- Fix: Added slow-operation hints and UI timeout escape for force-delete (release spinner while backend continues, then report completion/failure), stopped blocking force-delete completion on storage refresh (`fetchStorageOverview` now runs in background), and made storage/cache size calculations skip symlink/junction targets. Also added Windows junction target fallback when resolving hub links to cache paths.
- Files: `AGENTS.md`, `src/pages/settings-page/index.tsx`, `src-tauri/src/commands/github.rs`, `src-tauri/src/core/repo_scanner.rs`

### Windows Agent Unlink Fully No-Op (All Unlink Paths) 閳?2026-04-04
- Symptom: On Windows, all unlink operations (`閸楁洑閲滈幐澶愭尦閸欐牗绉烽柧鐐复`, `鐠佸墽鐤嗘い闈涘絿濞戝牓鎽奸幒顧? `閸欐牗绉烽崗銊╁劥闁剧偓甯碻) could no-op, making SVG agent buttons appear permanently lit.
- Root cause: Sync unlink flow first gated deletion behind a strict managed-entry precheck (`is_link || has SKILL.md`). On some Windows junction/directory states this precheck returned false, so removal was skipped before `remove_link_or_copy` had a chance to run.
- Fix: Split sync deletion into two paths: strict overwrite guard for link-time replacement, and unlink-time removal that attempts `remove_link_or_copy` whenever a target entry exists (only missing targets are treated as no-op).
- Files: `src-tauri/src/core/sync.rs`, `AGENTS.md`

### Windows Agent Unlink Reverted After Delay / Batch Unlink Aborted Early 閳?2026-04-04
- Symptom: On Windows, agent SVG link state could turn off then flip back on after ~2 seconds; Settings "閸欐牗绉烽柧鐐复" felt ineffective.
- Root cause: (1) Skill card batch link/unlink used concurrent toggles; one delayed filesystem failure could roll back the whole optimistic snapshot. (2) Backend `unlink_all_skills_from_agent` aborted on the first removal error, so one bad entry could cancel the entire batch.
- Fix: Changed skill-card batch toggles to sequential execution with partial-failure reporting and scoped rollback per-skill (not full snapshot). Unified backend unlink removal through robust link/copy removal, continued batch unlink on per-entry failures, and invalidated sync profile cache after agent profile mutations.
- Files: `src/pages/SkillCards.tsx`, `src/features/my-skills/hooks/useSkills.ts`, `src-tauri/src/core/sync.rs`, `src-tauri/src/commands/agents.rs`
