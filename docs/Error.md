# Error Log

Significant bugs and fixes, kept in short form for faster lookup.
Windows-specific issues are tracked separately in `docs/Win.md`.

## Format

```markdown
### [Title] 鈥?YYYY-MM-DD
- Symptom: ...
- Root cause: ...
- Fix: ...
- Files: ...
```

---

### Windows Multi Mode Disabled by Policy — 2026-04-10
- Symptom: Users could still enter tmux multi-mode on Windows and hit runtime/CLI path compatibility failures.
- Root cause: Multi-mode remained enabled in Launch UI and deploy path on Windows even when runtime behavior was unstable.
- Fix: Enforced single-mode-only policy on Windows in both frontend (multi toggle disabled + auto-fallback to single) and backend deploy guard (reject multi requests).
- Files: src/features/launch/components/LaunchDeckSection.tsx, src/features/launch/components/ModeSwitch.tsx, src-tauri/src/core/terminal_backend.rs, AGENTS.md

### Windows Multi Launch Incorrectly Reported tmux Missing (MSYS2/Git Bash) — 2026-04-10
- Symptom: On Windows, users with tmux installed in MSYS2 (or non-default bash runtime) still saw multi-mode blocked with "tmux is not installed" / bash missing prompts.
- Root cause: Runtime detection only relied on PATH plus a narrow Git Bash fallback (C:\\Program Files\\Git\\bin\\bash.exe), so valid MSYS2 installs were not discovered.
- Fix: Added Windows runtime probing for common bash/tmux locations (Git Bash + MSYS2), plus bash-based "tmux -V" fallback validation. Multi-mode launcher now reuses this resolver, and the tmux launch script dynamically derives bash PATH from the current Windows process PATH (no hardcoded user directories) so user-scoped CLI installs are discoverable.
- Files: src-tauri/src/core/terminal_backend.rs, src/features/launch/components/TmuxPrompt.tsx, AGENTS.md

### Windows Single Launch Failed with `0x80070002` (`bash ... .sh`) 鈥?2026-04-10
- Symptom: Launch Deck `single` mode on Windows failed to open with terminal error `0x80070002`, showing `绯荤粺鎵句笉鍒版寚瀹氱殑鏂囦欢` while trying to start `bash <temp>/ss-launch-*.sh`.
- Root cause: Launch backend always generated `.sh` scripts and Windows terminal launcher always executed `bash`, which breaks on systems without Git Bash/WSL `bash` in PATH.
- Fix: Added platform-aware single-mode script generation on Windows using PowerShell (`.ps1`) and launcher routing to `pwsh`/`powershell`; kept multi-mode on bash/tmux with explicit missing-bash error guidance.
- Files: `src-tauri/src/core/terminal_backend.rs`, `src-tauri/src/cli.rs`, `AGENTS.md`

### Release Matrix Action Failing with `Resource not accessible by integration` 鈥?2026-04-09
- Symptom: `tauri-action` fails in the release workflow across multiple matrix jobs with `Couldn't find release with tag vX.Y.Z. Creating one. ##[error]Resource not accessible by integration`, resulting in no release being published.
- Root cause: Race condition. Multiple concurrent matrix jobs attempt to create the missing Draft Release via `POST /releases` at the exact same moment. Due to concurrency/rate-limit restrictions on `GITHUB_TOKEN`, this throws an HTTP 403.
- Fix: Bypass the job racing by manually creating the empty Draft Release (via `gh release create vX.Y.Z --draft`) using the local `gh` token *before* triggering the GitHub Action matrix.
- Files: `.github/workflows/release.yml`, `.agents/workflows/release.md`

### Skill Update Fails But Card Shows "Updated" 鈥?2026-04-05
- Symptom: `git fetch --depth 1 --quiet` fails with `fatal: shallow file has changed since we read it`, toast shows "鏇存柊澶辫触", but the skill card's update button changes to "宸插畨瑁? (no update badge).
- Root cause: `prefetch_unique_repos` silently discarded fetch errors (`let _ = ...`). The subsequent `check_repo_skill_update_local` compared stale local/remote refs and returned `false` (no update), which wrote `update_available: false` to `UPDATE_STATE_CACHE`. The periodic `updatesQuery` refetch merged this into the frontend cache before or after the explicit update attempt, overriding the correct "update available" state.
- Fix: (1) Backend: `prefetch_unique_repos` now returns a set of repo roots where fetch failed. `check_repo_skill_update_local` accepts this set and returns `None` for failed-fetch repos (not `false`). `refresh_skill_updates` preserves the previous cached state for `None` results instead of clearing the cache. Patrol follows the same pattern. (2) Frontend: `useSkills.updateSkill` restores `update_available: true` in the query cache on error as defense-in-depth.
- Files: `repo_scanner.rs`, `installed_skill.rs`, `patrol.rs`, `useSkills.ts`

### Settings / GitHub Mirror Panel Appeared Clipped (Blank Lower Area) 鈥?2026-04-04
- Symptom: In Settings, clicking `GitHub 鍔犻€焋 could show a partially rendered mirror panel with a large blank area below.
- Root cause: Nested flex containers in settings page missed `min-h-0`, and the scroll root used `h-full`; in WebView this can break overflow sizing and clip tall sections.
- Fix: Added `min-h-0` to the settings root and main flex containers, and switched the scroll root from `h-full` to `min-h-0` to restore correct vertical scrolling/layout.
- Files: `src/pages/settings-page/index.tsx`

### CLI Install Mode Missing `--global` / `--agent` / Interactive Selection 鈥?2026-04-02
- Symptom: `skillstar install --global <url>` was unsupported, and default CLI install did not provide one-step project-level linking or explicit per-agent targeting.
- Root cause: `cli.rs` install subcommand only accepted `url` and always executed a hub clone path; no mode switch existed for project-level vs global install, and no terminal selection flow existed when agent targets were omitted.
- Fix: Extended CLI install with `--global`, `--project`, `--agent`, and `--name`; install now reuses `core::skill_install`, defaults to project-level linking in current directory, supports interactive terminal agent selection when `--agent` is omitted, and falls back to `.agents/skills` when no agent is chosen.
- Files: `src-tauri/src/cli.rs`, `src-tauri/src/main.rs`, `AGENTS.md`, `README.md`

### SkillSelectionBar Overlaps Info Sidebar (DetailPanel) 鈥?2026-04-01
- Symptom: When the side Info Panel (`DetailPanel`) slided out to show skill details, the `SkillSelectionBar` (batch actions bar) rendered on top of it, overlapping the header text and close buttons.
- Root cause: `SkillSelectionBar` was hardcoded to `z-[60]`, which is higher than the `DetailPanel`'s `z-50` overlay relative to the main container.
- Fix: Reduced the `SkillSelectionBar` `z-index` from `z-[60]` to `z-40`, allowing it to sit safely beneath the sliding DetailPanel while still maintaining its hover layer above regular content.
- Files: `SkillSelectionBar.tsx`

### Agent Batch Link / Unlink Commands Did Not Invalidate Skill Cache 鈥?2026-04-01
- Symptom: "閾炬帴鍒版櫤鑳戒綋" (Link to Agent) dropdown in selection bar appeared to have no effect; agent icons on cards didn't update after batch linking.
- Root cause: `batch_link_skills_to_agent`, `unlink_all_skills_from_agent`, and `unlink_skill_from_agent` in `commands/agents.rs` successfully created/removed symlinks but did not call `installed_skill::invalidate_cache()`. The subsequent `list_skills` refresh returned stale cached `agent_links` data, making it look like nothing happened.
- Fix: Added `installed_skill::invalidate_cache()` calls after each symlink mutation, matching the pattern already established by `toggle_skill_for_agent` in `commands.rs`.
- Files: `commands/agents.rs`

### OfficialPublishers Layout Toggle Broken 鈥?2026-04-01
- Symptom: Grid/list view toggle buttons on the "瀹樻柟鍙戝竷鑰? tab had no visible effect; layout stayed single-column regardless.
- Root cause: `OfficialPublishers` set a CSS variable `--ss-card-min` inline, but `ss-cards-grid` class has no `grid-template-columns` rule consuming it. Grid defaulted to single implicit column, making grid 鈮?list.
- Fix: Replaced unused CSS variable with explicit `gridTemplateColumns: "repeat(auto-fill, minmax(280px, 1fr))"` inline style, matching `SkillGrid`'s approach. Removed unused `CSSProperties` import.
- Files: `OfficialPublishers.tsx`

### Marketplace Grid Vertical Gaps & Tab-Switch Artifacts 鈥?2026-04-01
- Symptom: Switching marketplace tabs caused card vertical gaps to jump; spacing inconsistent with Skills page.
- Root cause: `@tanstack/react-virtual` rendered each grid row as an absolutely-positioned container; CSS `gap` only worked horizontally. Stale virtualizer measurements persisted across tab changes.
- Fix: Replaced virtualization with progressive infinite-scroll (initial 60, +30 per scroll). Skip `AnimatePresence`/layout animations for >100 items. Unified scroll containers (`ss-page-scroll`).
- Files: `SkillGrid.tsx`, `Marketplace.tsx`, `PublisherDetail.tsx`

### SkillGrid ResizeObserver Never Attached 鈥?Stuck At 1 Column 鈥?2026-04-01
- Symptom: Grid showed single-column full-width strips; view toggle had no effect. Recurred across two separate fixes (post-virtualization removal, then post-virtualization re-add).
- Root cause: `useLayoutEffect([])` ran on mount when `skills` was empty 鈫?`containerRef.current` was null 鈫?observer never attached 鈫?`containerWidth` stayed 0 鈫?column count locked at 1.
- Fix: Added data-readiness flag (`gridRendered` / `shouldVirtualize`) to effect dependency array so observer re-attaches when the grid div appears.
- Files: `SkillGrid.tsx`

### Grid `auto-fit` Stretched Solitary Items To Full Width 鈥?2026-04-01
- Symptom: Single items in grid view stretched full-width, making grid indistinguishable from list.
- Root cause: `auto-fit` forces standalone items to fill remaining tracks.
- Fix: Replaced all `auto-fit` with `auto-fill`; standardized `columnStrategy="auto-fill"`.
- Files: `index.css`, `SkillCards.tsx`, `Marketplace.tsx`, `PublisherDetail.tsx`, `SkillGrid.tsx`

### Skill Card Entry Animation Made Text Look Bold 鈥?2026-04-01
- Symptom: Card title/body text looked thicker during entry animation.
- Root cause: `scale` transforms caused font rasterization artifacts during interpolation.
- Fix: Removed `scale` from list item variants (kept `opacity + y`); changed to `layout="position"`.
- Files: `SkillGrid.tsx`

### Project Import Stored Skills In Hub Instead Of Local 鈥?2026-04-01
- Symptom: Importing project-discovered skills created real dirs under `skills/` instead of `skills-local/`.
- Root cause: `import_scanned_skills` bypassed local-skill adoption flow.
- Fix: Move discovered skills to `skills-local/` first, then create hub symlink.
- Files: `local_skill.rs`, `project_manifest.rs`

### Marketplace Local-First Snapshot Missing 鈥?2026-04-01
- Symptom: Marketplace tabs felt slow; repeated navigation re-downloaded data.
- Root cause: No durable local snapshot model; descriptions hydrated in browser; no shared cache.
- Fix: Added `marketplace_snapshot.rs` with SQLite + FTS; local-first reads with freshness status; seed remote search into local corpus.
- Files: `marketplace_snapshot.rs`, `commands/marketplace.rs`, `lib.rs`, `useMarketplace.ts`, `Marketplace.tsx`, `PublisherDetail.tsx`, `DetailPanel.tsx`

### Marketplace Stuck Loading In React StrictMode 鈥?2026-04-01
- Symptom: Marketplace stayed on loading spinner forever.
- Root cause: `mountedRef` cleanup set `false` but mount never reset to `true`; StrictMode remount skipped `setLoading(false)`.
- Fix: Reset `mountedRef.current = true` on mount.
- Files: `useMarketplace.ts`, `useAiStream.ts`, `DetailPanel.tsx`

### Deck Card Hid Install Action For Missing Skills 鈥?2026-04-01
- Symptom: Deck cards showed `No skills installed` without install button.
- Root cause: `skill_sources` not persisted on create/edit; install gated by pre-existing source metadata.
- Fix: Persist `skill_sources`; keep install visible; add marketplace name-based source fallback.
- Files: `SkillCards.tsx`, `en.json`, `zh-CN.json`

### Export Analyzer Misclassified Uninstalled Simple Skills As Bundle 鈥?2026-04-01
- Symptom: Share showed `鍘嬬缉鍖卄 for simple skills only missing locally.
- Root cause: Export only trusted local `git_url`; uninstalled skills failed file reads 鈫?marked as bundle.
- Fix: Add marketplace name-based `git_url` resolution fallback before local-file fallback.
- Files: `ExportShareCodeModal.tsx`

### AI Smart Pick Fragile And Random 鈥?2026-04-01
- Symptom: Recommendations swung between runs; loosely formatted output parsed unreliably.
- Root cause: Unbounded catalog, fragile parsing, single-round dominance, relevance info discarded.
- Fix: Deterministic local pre-ranking, bounded catalogs, structured AI output with score/reason, multi-round consensus, fallback to local ranking.
- Files: `ai_provider.rs`, `commands/ai.rs`, `pick_skills.md`, `AiPickSkillsModal.tsx`

### Translation Cache Split Across Frontend And Backend 鈥?2026-04-01
- Symptom: Translation reuse inconsistent; `SKILL.md` streaming ran concurrently; retranslate still hit MyMemory.
- Root cause: Frontend kept caches outside SQLite; no global session gate; retranslate only bypassed cache without forcing AI.
- Fix: SQLite as sole durable cache; serialize streaming sessions globally; AI-only retranslate path.
- Files: `commands/ai.rs`, `SkillReader.tsx`, `DetailPanel.tsx`

### Tray Patrol Stop/Toggle And Window Close 鈥?2026-04-01
- Symptom: Three related issues: (1) tray `鍋滄鍚庡彴妫€鏌 ineffective because frontend auto-restarted patrol; (2) window close left tray icon after background run disabled; (3) tray label stuck on `Stop` instead of toggling.
- Root cause: Tray stop didn't sync to frontend state; close always force-hid regardless of background-run setting; tray menu was static one-way.
- Fix: Persist patrol intent in backend; gate auto-start on backend flag; make close conditional on patrol enablement; add state-aware tray menu rebuilding with real start/stop toggle.
- Files: `patrol.rs`, `commands/patrol.rs`, `lib.rs`, `App.tsx`, `BackgroundRunSection.tsx`, `settings-page/index.tsx`

### Security Scan Cache Bugs And Log Cleanup 鈥?2026-03-31
- Symptom: (1) Static scans satisfied AI scans; tail edits missed re-scan; broken AI cached as safe. (2) `娓呯悊缂撳瓨` left log files behind.
- Root cause: Cache key lacked scan mode/version; content hash was truncated; AI failures downgraded; log cleanup missing from clear command.
- Fix: Scope cache by mode + version; hash full contents; surface incomplete AI without caching; invalidate on in-app edits; add log cleanup to `clear_security_scan_cache`.
- Files: `security_scan.rs`, `commands/ai.rs`, `commands.rs`

### Security Scan CLI Build Broken 鈥?2026-03-31
- Symptom: `cargo check` failed after security scan CLI changes.
- Root cause: CLI called old `scan_single_skill()` signature.
- Fix: Pass new args (`run_ai = true`, `on_progress = None`).
- Files: `cli.rs`

### Project Import / Shared-Path / Disambiguation Cluster 鈥?2026-03-31
- Symptom: Cluster of related issues: (1) Import replaced non-skill folders and drifted ownership. (2) Duplicate unmanaged skills for shared paths. (3) Multiple agents auto-enabled on same shared path. (4) Disambiguation destroyed symlinks, allowed multi-select, didn't persist or hydrate skills. (5) Hydration happened too late. (6) OpenClaw incorrectly participated in disambiguation.
- Root cause: No `SKILL.md` validation; no dedupe by real path; no shared-path ownership canonicalization; disambiguation used checkbox instead of radio; confirm set UI state without saving/syncing; agent enable didn't trigger filesystem hydration; OpenClaw had non-empty `project_skills_rel`.
- Fix: Require `SKILL.md`; dedupe by `(project_skills_rel, name)`; enforce one owner per shared path; switch to single-select; auto-apply on confirm; re-scan symlinked skills after confirm; set OpenClaw `project_skills_rel` to empty; add `save_project_skills_list` and `rebuild_project_skills_from_disk`.
- Files: `project_manifest.rs`, `commands/projects.rs`, `lib.rs`, `agent_profile.rs`, `AgentDisambiguationDialog.tsx`, `projects-page/index.tsx`, `useProjectManifest.ts`

### Agent Toggle Default Mismatched UI 鈥?2026-03-31
- Symptom: First disable click looked ineffective; security badges showed stale data.
- Root cause: Toggle fallback state mismatched UI default; scan cache merged without pruning.
- Fix: Align toggle fallback; reset/prune scan state to installed skills.
- Files: `agent_profile.rs`, `commands/ai.rs`, `useSecurityScan.ts`

### Vite Dev Reload On Project Skill Writes 鈥?2026-03-31
- Symptom: `Import All` caused dev-mode page reload/route jump.
- Root cause: Vite watcher reacted to writes inside project agent folders.
- Fix: Ignore `.agents`, `.claude`, `.gemini`, `.opencode` in Vite watch config.
- Files: `vite.config.ts`

### Deck Deployment Duplicate Conflict Flow 鈥?2026-03-31
- Symptom: Two dialogs appeared; conflicting shared-path agents selectable together.
- Root cause: Conflict logic split across dialogs; no shared-path exclusion in picker.
- Fix: Keep flow in deploy picker; enforce mutual exclusion inside shared-path groups.
- Files: `ProjectDeployAgentDialog.tsx`, `projects-page/index.tsx`

### Repo-Cached Update Badge Persists After Update 鈥?2026-03-31
- Symptom: Update badge reappeared after manual update.
- Root cause: Shallow/full repos used different update strategies 鈫?inconsistent HEAD alignment.
- Fix: Unify to `fetch` + `reset --hard origin/HEAD` (plus sparse rules re-apply).
- Files: `repo_scanner.rs`

### Shallow Clone `git pull` Fails 鈥?Skills Stuck on "Update Available" 鈥?2026-03-31
- Symptom: Skills in endless update-available state.
- Root cause: `git pull` failed on shallow repos with divergent grafted history.
- Fix: Detect shallow repos; use `fetch --depth 1` + `reset --hard origin/HEAD`.
- Files: `repo_scanner.rs`

### Uninstalling Skill Left Stale Project Metadata And Broken Symlinks 鈥?2026-03-31
- Symptom: Uninstalled skills still appeared in projects with stale links.
- Root cause: Uninstall only removed hub install, not project metadata/symlinks.
- Fix: Add `remove_skill_from_all_projects()` cleanup in both uninstall paths.
- Files: `project_manifest.rs`, `local_skill.rs`, `commands.rs`

### Repo Import Could Overwrite Unrelated Skills 鈥?2026-03-31
- Symptom: Repo import overwrote local or foreign-remote skills with same name.
- Root cause: Reinstall accepted name collision without source ownership validation.
- Fix: Allow replacement only when existing install resolves to same remote.
- Files: `repo_scanner.rs`

### Framer Motion Layout Stuttering 鈥?2026-03-30
- Symptom: Toolbar flyovers, sidebar snap, abrupt collapse clipping.
- Root cause: Global `layoutId` collisions, width vs `min-w` conflicts, height animations with padding/margin leaks.
- Fix: Scope `layoutId` via `useId()`, remove conflicting `min-w`, add `overflow-hidden`, move spacing to inner wrappers.
- Files: `Toolbar.tsx`, `Sidebar.tsx`, `SkillSelectionBar.tsx`, `CreateGroupModal.tsx`

### Bundle Identifier `.app` Suffix macOS Conflict 鈥?2026-03-30
- Symptom: macOS bundle id warning and release instability.
- Fix: Change identifier to `com.skillstar.desktop`.
- Files: `tauri.conf.json`

### Local DMG Packaging Fails 鈥?2026-03-30
- Symptom: Build succeeded but DMG failed locally.
- Root cause: `create-dmg` AppleScript lacked macOS permissions.
- Fix: Use `--bundles app` locally; keep DMG in CI.
- Files: `run-build.sh`

### CI `latest.json` Fails After Re-tag 鈥?2026-03-30
- Symptom: `latest.json` job missed updater artifact after deleting draft and re-tagging.
- Root cause: Release asset availability race during matrix uploads.
- Fix: Re-run workflow; avoid deleting draft release between retries.
- Files: `release.yml`, `build_merged_latest_json.cjs`

### Safari/WebKit SVG Filter Rendering Bug 鈥?2026-03-30
- Symptom: Complex SVG invisible in WebKit via `<img src>`.
- Root cause: WebKit sandbox dropped masked groups with complex Figma filters.
- Fix: Inline SVG as React component for DOM rendering path.
- Files: `antigravity.svg`, `AntigravityIcon.tsx`, `AgentIcon.tsx`, `Toolbar.tsx`, `SkillCard.tsx`, `AgentConnectionsSection.tsx`, `AgentAccordion.tsx`, `ProjectDeployAgentDialog.tsx`
