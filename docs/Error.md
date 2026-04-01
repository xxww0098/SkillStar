# Error Log

Significant bugs and fixes, kept in short form for faster lookup.

## Format

```markdown
### [Title] — YYYY-MM-DD
- Symptom: ...
- Root cause: ...
- Fix: ...
- Files: ...
```

---

### Project Import Stored Discovered Skills In Hub Instead Of Local Storage — 2026-04-01
- Symptom: Importing unmanaged skills discovered in project agent folders created real directories under `~/.skillstar/.agents/skills/` instead of treating them as local skills.
- Root cause: `import_scanned_skills` copied project-discovered skills straight into the hub and only replaced the project folder with a symlink afterwards, bypassing `skills-local/`.
- Fix: Add local-skill adoption flow that moves discovered project skills into `skills-local/`, recreates the hub entry as a symlink, and then points the project folder at that canonical hub link.
- Files: `src-tauri/src/core/local_skill.rs`, `src-tauri/src/core/project_manifest.rs`, `AGENTS.md`

### Marketplace Could Feel Slow And Re-fetch The Same Data On Every View — 2026-04-01
- Symptom: Marketplace tabs, publisher drill-down, and detail panels depended on direct remote fetches or client-side hydration, so first paint was slow and repeated navigation re-downloaded the same marketplace data.
- Root cause: Marketplace had no durable local snapshot model; descriptions were hydrated in the browser, search skipped any local index, and publisher/detail flows bypassed a shared cache.
- Fix: Add `marketplace_snapshot.rs` with SQLite snapshot tables + FTS, expose local-first Tauri commands with freshness status, move Marketplace/PublisherDetail/DetailPanel to snapshot reads plus explicit stale refresh, and seed remote search results back into the local corpus.
- Files: `src-tauri/src/core/marketplace_snapshot.rs`, `src-tauri/src/commands/marketplace.rs`, `src-tauri/src/lib.rs`, `src/hooks/useMarketplace.ts`, `src/pages/Marketplace.tsx`, `src/pages/PublisherDetail.tsx`, `src/components/layout/DetailPanel.tsx`, `src/types/index.ts`, `AGENTS.md`, `AGENTS-UI.md`

### Marketplace Stayed In Loading State Forever In React StrictMode — 2026-04-01
- Symptom: Marketplace page stayed on `正在从 skills.sh 加载技能...` and never rendered data.
- Root cause: `mountedRef` cleanup set `current = false`, but mount lifecycle never reset it to `true`; in `React.StrictMode` remount checks, async callbacks exited early and skipped `setLoading(false)`.
- Fix: Reset `mountedRef.current = true` on mount lifecycle, keep `false` on cleanup, and apply the same fix to shared async components using the same guard pattern.
- Files: `src/hooks/useMarketplace.ts`, `src/hooks/useAiStream.ts`, `src/components/layout/DetailPanel.tsx`

### Deck Card Could Hide Install Action For Missing Skills — 2026-04-01
- Symptom: Deck cards showed `No skills installed` without an install button, and existing decks could not recover missing skills.
- Root cause: Deck create/edit flow did not persist `skill_sources` from selected installed skills, and install actions were gated by pre-existing source metadata only.
- Fix: Persist `skill_sources` on deck create/edit, keep install entry visible when skills are missing, and add marketplace name-based source resolution fallback during install.
- Files: `src/pages/SkillCards.tsx`, `src/i18n/locales/en.json`, `src/i18n/locales/zh-CN.json`

### Export Analyzer Could Misclassify Uninstalled Simple Skills As Bundle — 2026-04-01
- Symptom: `分享技能` showed `压缩包` for simple skills that were only missing locally, instead of generating a normal share code.
- Root cause: Export analysis only trusted local `git_url` and fell back to reading local files; uninstalled skills failed local file reads and were marked as bundle.
- Fix: Add marketplace name-based `git_url` resolution fallback before local-file fallback so missing-but-remote skills stay in normal share-code export.
- Files: `src/components/skills/ExportShareCodeModal.tsx`

### AI Smart Pick Could Feel Random And Return Fragile Results — 2026-04-01
- Symptom: `AI 智能选卡` recommendations could swing between runs, fail on loosely formatted model output, or surface alphabetically sorted results that looked unrelated to the user prompt.
- Root cause: The picker sent an unbounded YAML-like catalog directly to the model, accepted overly fragile array-only parsing, allowed one successful round to dominate, and discarded relevance information before returning results.
- Fix: Add deterministic local pre-ranking and bounded candidate catalogs, require structured AI recommendations with score/reason metadata, aggregate multi-round consensus into stable relevance ordering, and fall back to deterministic local ranking when AI output is partial or invalid.
- Files: `src-tauri/src/core/ai_provider.rs`, `src-tauri/src/commands/ai.rs`, `src-tauri/prompts/ai/pick_skills.md`, `src/components/skills/AiPickSkillsModal.tsx`, `src/types/index.ts`, `src/i18n/locales/en.json`, `src/i18n/locales/zh-CN.json`, `AGENTS.md`, `AGENTS-UI.md`

### Translation Cache Was Split Across Frontend State And Backend DB — 2026-04-01
- Symptom: Translation reuse behaved inconsistently across panels, `SKILL.md` streaming could run concurrently, and `AI 重译` for short text could still come back from MyMemory.
- Root cause: Frontend kept extra translation caches outside SQLite, `ai_translate_skill_stream` had no global session gate, and short-text retranslate only bypassed cache without forcing the AI provider path.
- Fix: Make SQLite the durable translation cache authority, serialize `SKILL.md` streaming translation sessions globally, and add an AI-only retranslate path for short text.
- Files: `src-tauri/src/commands/ai.rs`, `src/components/skills/SkillReader.tsx`, `src/components/layout/DetailPanel.tsx`, `AGENTS.md`, `AGENTS-UI.md`

### Tray Stop Patrol Could Be Silently Undone By Auto-Restart — 2026-04-01
- Symptom: Clicking `停止后台检查` in the tray appeared ineffective because background checks restarted after the window hid again.
- Root cause: Tray stop only halted the current Rust patrol loop; the frontend still auto-started patrol on `window-hidden` from its local background-run toggle.
- Fix: Persist patrol auto-start intent in backend status, gate hidden-window auto-start on that backend flag, and sync tray stop back into the shared frontend background-run state.
- Files: `src-tauri/src/core/patrol.rs`, `src-tauri/src/commands/patrol.rs`, `src-tauri/src/lib.rs`, `src/App.tsx`, `src/pages/settings-page/BackgroundRunSection.tsx`, `src/pages/settings-page/index.tsx`, `AGENTS.md`, `AGENTS-UI.md`

### Window Close Could Still Leave Tray Icon After Background Run Was Stopped — 2026-04-01
- Symptom: After disabling `后台运行`, clicking the window close button still left the app alive in the macOS menu bar/tray.
- Root cause: Native close handling always forced `prevent_close + hide`, regardless of whether background run was still enabled.
- Fix: Make close behavior conditional on patrol enablement: hide to tray only when background run is enabled, otherwise allow the close action to terminate the app; also sync frontend background-run preference into backend state at startup.
- Files: `src-tauri/src/lib.rs`, `src/App.tsx`, `AGENTS.md`

### Tray Background Action Stayed Stuck On `Stop` Instead Of Toggling — 2026-04-01
- Symptom: The tray menu always showed `停止后台检查`, even after background mode was already stopped.
- Root cause: Tray menu labels were static, and the tray action only implemented a one-way stop path instead of rebuilding itself from the current patrol enabled state.
- Fix: Add tray state-aware menu rebuilding, change the tray action into a real start/stop toggle, and broadcast shared patrol enabled state so Settings and tray labels stay aligned.
- Files: `src-tauri/src/lib.rs`, `src-tauri/src/commands/patrol.rs`, `src/App.tsx`, `AGENTS.md`, `AGENTS-UI.md`

### Security Scan Clear Cache Left Log Files Behind — 2026-03-31
- Symptom: Clicking `清理缓存` in Security Scan cleared cached results but old scan logs remained on disk.
- Root cause: `clear_security_scan_cache` only deleted DB cache rows and legacy JSON cache, not `security_scan.log` or `security_scan_logs/`.
- Fix: Add explicit log cleanup (`runtime log + per-run log directory`) and wire it into `clear_security_scan_cache`.
- Files: `src-tauri/src/core/security_scan.rs`, `src-tauri/src/commands/ai.rs`, `AGENTS.md`

### Security Scan Could Reuse Wrong Cache And Hide AI Failures — 2026-03-31
- Symptom: Static scans could satisfy later AI scans; large-file tail edits could miss re-scan; broken AI responses could still end up cached as safe.
- Root cause: Cache key only used `skill_name + tree_hash` from truncated content, scan mode was not separated, and AI parse/aggregation failures were downgraded instead of treated as incomplete.
- Fix: Scope cache by scan mode and scanner version, hash full file contents, surface incomplete AI analysis without caching it, and invalidate scan cache after in-app `SKILL.md` edits.
- Files: `src-tauri/src/core/security_scan.rs`, `src-tauri/src/commands/ai.rs`, `src-tauri/src/commands.rs`, `AGENTS.md`

### Security Scan Rollout Left CLI Build Broken — 2026-03-31
- Symptom: `cargo check` failed after security scan CLI changes.
- Root cause: CLI still called old `scan_single_skill()` signature.
- Fix: Pass new args explicitly (`run_ai = true`, `on_progress = None`).
- Files: `src-tauri/src/cli.rs`

### Project Import Could Replace Non-Skill Folders And Drift Shared-Path Ownership — 2026-03-31
- Symptom: `Import All` could import invalid folders and mis-assign owner on shared paths.
- Root cause: Non-symlink directories were treated as valid without `SKILL.md`; backend trusted unstable `agent_id` on shared paths.
- Fix: Require `SKILL.md`, skip invalid dirs, preserve persisted owner, and re-canonicalize frontend state.
- Files: `src-tauri/src/core/project_manifest.rs`, `src/pages/projects-page/index.tsx`

### Agent Toggle Default State And Security Scan Cache Could Both Look Stale — 2026-03-31
- Symptom: First disable click looked ineffective; security badges could show stale data.
- Root cause: Toggle fallback state mismatched UI default; scan cache was merged without strict pruning.
- Fix: Align toggle fallback with rendered state; reset/prune scan state and cache to installed skills.
- Files: `src-tauri/src/core/agent_profile.rs`, `src-tauri/src/commands/ai.rs`, `src/hooks/useSecurityScan.ts`

### Importing Project Skills Triggered Dev-Mode Page Reload And Route Jump — 2026-03-31
- Symptom: `Import All` caused dev-mode page reload/route jump.
- Root cause: Vite watcher reacted to writes inside project agent skill folders.
- Fix: Ignore `.agents`, `.claude`, `.gemini`, `.opencode` in Vite watch config.
- Files: `vite.config.ts`

### Unmanaged Skill Scan Showed Duplicate Names For Shared Paths — 2026-03-31
- Symptom: Same unmanaged skill could appear multiple times.
- Root cause: No dedupe by real project path for shared directories.
- Fix: Deduplicate by `(project_skills_rel, skill name)` and add import-target dedupe guard.
- Files: `src/pages/projects-page/index.tsx`

### Shared-Path Scan Hydration Could Auto-Enable Multiple Agents — 2026-03-31
- Symptom: Multiple agents on same shared path could auto-enable together.
- Root cause: Scan results were merged by `agent_id` without shared-path ownership canonicalization.
- Fix: Enforce one owner per shared path across initial load, disambiguation, deploy, and manual toggles.
- Files: `src/pages/projects-page/index.tsx`

### Disambiguation Auto-Sync Could Destructively Drop Shared-Path Symlinks — 2026-03-31
- Symptom: Shared-path skills disappeared after disambiguation confirm.
- Root cause: Confirm path called full sync before complete in-memory hydration.
- Fix: Add non-destructive `save_project_skills_list`; add `rebuild_project_skills_from_disk` repair flow.
- Files: `src-tauri/src/core/project_manifest.rs`, `src-tauri/src/commands/projects.rs`, `src-tauri/src/lib.rs`, `src/hooks/useProjectManifest.ts`, `src/pages/projects-page/index.tsx`, `AGENTS.md`

### Conflict Resolution Could Miss Shared-Path Skills After Agent Choice — 2026-03-31
- Symptom: Chosen owner agent still showed `No skills assigned`.
- Root cause: Hydration only used selected `agent_id`, not all ids in conflict group.
- Fix: Hydrate from full conflict-group union and seed initial state from scan index.
- Files: `src/pages/projects-page/index.tsx`

### Deck Deployment Showed Duplicate Conflict Flow and Allowed Conflicting Picks — 2026-03-31
- Symptom: Two dialogs appeared; conflicting shared-path agents could be selected together.
- Root cause: Conflict logic split across dialogs; picker had no shared-path exclusion.
- Fix: Keep flow in deploy picker and enforce mutual exclusion inside shared-path groups.
- Files: `src/components/skills/ProjectDeployAgentDialog.tsx`, `src/pages/projects-page/index.tsx`

### Project Skill Hydration Happened Too Late in Selection Flow — 2026-03-31
- Symptom: Existing symlinked project skills appeared late or empty on first load.
- Root cause: Early selection flow did not index symlink skills from initial scan.
- Fix: Scan and merge symlink skill index during first `handleSelectProject` phase.
- Files: `src/pages/projects-page/index.tsx`

### Disambiguation Confirm Did Not Persist Until Manual Apply — 2026-03-31
- Symptom: Owner choice stayed local until user pressed Apply.
- Root cause: Confirm handler set UI state but did not call save/sync.
- Fix: Auto-apply on confirm with filtered agents and clear dirty state on success.
- Files: `src/pages/projects-page/index.tsx`

### Disambiguation Enables Agent But Doesn't Hydrate Existing Project Skills — 2026-03-31
- Symptom: Agent enabled after disambiguation, but skill list remained empty.
- Root cause: Handler only flipped enabled state, no immediate filesystem hydration.
- Fix: Re-scan and merge existing symlinked skills for selected agent right after confirm.
- Files: `src/pages/projects-page/index.tsx`

### Agent Disambiguation Dialog Allowed Multi-Select — 2026-03-31
- Symptom: Shared-path ownership dialog allowed selecting multiple agents.
- Root cause: Dialog used checkbox array state instead of single-choice semantics.
- Fix: Switch to radio-like single selection (`selectedId`) and single-id payload.
- Files: `src/components/skills/AgentDisambiguationDialog.tsx`, `src/pages/projects-page/index.tsx`

### OpenClaw Incorrectly Participates in Project Path Disambiguation — 2026-03-31
- Symptom: OpenClaw appeared in project shared-path disambiguation.
- Root cause: `openclaw.project_skills_rel` was set to `.agents/skills`.
- Fix: Set OpenClaw `project_skills_rel` to empty string and add regression test.
- Files: `src-tauri/src/core/agent_profile.rs`, `AGENTS.md`

### Repo-Cached Update Badge Persists After Manual Update — 2026-03-31
- Symptom: Update badge reappeared right after manual update.
- Root cause: Shallow/full repos used different update strategies, leading to inconsistent HEAD alignment.
- Fix: Unify to `fetch` + `reset --hard origin/HEAD` (plus sparse rules re-apply).
- Files: `src-tauri/src/core/repo_scanner.rs`

### Framer Motion Layout Stuttering and Jittering — 2026-03-30
- Symptom: Toolbar flyovers, sidebar snap, and abrupt collapse clipping.
- Root cause: Global `layoutId` collisions, width vs `min-w` conflicts, and height animations with padding/margin leaks.
- Fix: Scope `layoutId` via `useId()`, remove conflicting `min-w`, add `overflow-hidden` and move spacing to inner wrappers.
- Files: `src/components/layout/Toolbar.tsx`, `src/components/layout/Sidebar.tsx`, `src/components/skills/SkillSelectionBar.tsx`, `src/components/skills/CreateGroupModal.tsx`

### Bundle identifier `.app` suffix causes macOS conflict — 2026-03-30
- Symptom: macOS bundle id warning and release instability.
- Root cause: `identifier` ending with `.app` conflicts with macOS bundle extension.
- Fix: Change identifier to `com.skillstar.desktop`.
- Files: `src-tauri/tauri.conf.json`

### Local DMG packaging fails with `bundle_dmg.sh` error — 2026-03-30
- Symptom: Build succeeded but DMG packaging failed locally.
- Root cause: `create-dmg` AppleScript step lacked required macOS permissions.
- Fix: Use app-only local build (`--bundles app`); keep DMG packaging in CI.
- Files: `run-build.sh`

### CI `Build latest.json` fails after deleting draft release and re-tagging — 2026-03-30
- Symptom: `latest.json` job missed required updater artifact after re-tag.
- Root cause: Release asset availability race while matrix jobs were still finalizing uploads.
- Fix: Re-run workflow; avoid deleting draft release between retries.
- Files: `.github/workflows/release.yml`, `scripts/release/build_merged_latest_json.cjs`

### Safari/WebKit SVG Filter Rendering Bug — 2026-03-30
- Symptom: Complex SVG rendered incorrectly or invisible in WebKit via `<img src>`.
- Root cause: WebKit sandbox/filter handling dropped masked groups with complex Figma filters.
- Fix: Inline the SVG as a React component to use DOM SVG rendering path.
- Files: `public/agents/antigravity.svg`, `src/components/ui/icons/AntigravityIcon.tsx`, `src/components/ui/AgentIcon.tsx`, `src/components/layout/Toolbar.tsx`, `src/components/skills/SkillCard.tsx`, `src/pages/settings-page/AgentConnectionsSection.tsx`, `src/pages/projects-page/AgentAccordion.tsx`, `src/components/skills/ProjectDeployAgentDialog.tsx`

### Shallow Clone `git pull` Fails — All Skills Stuck on "Update Available" — 2026-03-31
- Symptom: Skills stayed in endless update-available state.
- Root cause: `git pull` failed on shallow repos with divergent grafted history.
- Fix: Detect shallow repos and use `fetch --depth 1` + `reset --hard origin/HEAD`.
- Files: `src-tauri/src/core/repo_scanner.rs`

### Uninstalling A Skill Left Stale Project Metadata And Broken Symlinks — 2026-03-31
- Symptom: Uninstalled skills still appeared in projects and left stale links.
- Root cause: Uninstall flow removed hub install only, not project metadata/symlinks.
- Fix: Add `remove_skill_from_all_projects()` cleanup in both uninstall paths.
- Files: `src-tauri/src/core/project_manifest.rs`, `src-tauri/src/core/local_skill.rs`, `src-tauri/src/commands.rs`

### Repo Import Reinstall Could Overwrite Local Or Unrelated Skills — 2026-03-31
- Symptom: Repo import could overwrite local or foreign-remote skills with same name.
- Root cause: Reinstall path accepted name collision without source ownership validation.
- Fix: Allow replacement only when existing install resolves to same remote; otherwise reject.
- Files: `src-tauri/src/core/repo_scanner.rs`
