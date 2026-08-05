# Error Log

状态：active

## 2026-08-05 - 已有仓库扫描把稀疏工作树误当完整远端树，并跟随 Skill symlink

- Symptom: 已有私有仓库的注册预览可能漏掉 Skill 目录外的嵌套文件、漏掉根 Skill 之外的嵌套 Skill，且恶意仓库可用 symlink 让发现器读取 checkout 外的 `SKILL.md`。
- Root cause: 通用 Skill 安装缓存为了性能使用 shallow sparse checkout；注册预览却遍历物化后的工作树来代表完整远端库存。与此同时，递归发现使用会跟随 symlink 的 `Path::is_dir` / `read_to_string`，没有文件类型和大小边界。
- Fix: 注册预览以 `git ls-tree` 的 tracked tree 作为文件清单，忽略 cache untracked 文件，再按 tree 中的全部 `SKILL.md` 目录物化并重新发现；发现器只接受不超过 1 MiB 的普通 `SKILL.md` 文件，递归 entry 与优先目录的每一级父路径都用 `symlink_metadata` 拒绝 symlink。
- Files: `crates/skillstar-skills/src/{shared_channels/existing.rs,discovery.rs}`。
- Self-check: sparse fixture 必须同时看见 `.github/workflows/ci.yml`、根 Skill 和嵌套 Skill，且不包含 cache untracked 文件；外部目录 symlink、`SKILL.md` symlink 和超大 manifest 均不得被发现。

## 2026-07-29 - Skill update 链路上的三处静默分叉

- Symptom: 三个互相独立、都不报错的现象 —— (a) 从 Finder 启动的 GUI 永远显示"无更新"，而同一台机器上 CLI 正常；(b) patrol 发现的更新在重启后消失；(c) `skillstar update` 跑完后技能仍显示可更新，且 Agent 侧内容没变。
- Root cause（三处同源问题：同一个事实有多个实现）：
  1. **repo-cache 判定两份实现且已分叉。** `update_checker` 用 `std::fs::read_link` 解析链接，`repo_scanner::ops` 用 `fs_ops::read_link_resolved`（含 Windows junction 回退）。junction 部署的技能在 update 应用侧算 repo-cached、在 update 检测侧不算。
  2. **update 检测的 git 子进程绕过 `command_with_path`。** `update_checker::git_rev_parse` 和它自己那份 `compute_subtree_hash` 用裸 `Command::new("git")`；同 crate 的 `repo_scanner::scan` 那份用 `command_with_path`。macOS 从 Finder 启动的进程没有 login shell PATH，`git` 找不到 → `rev_parse` 失败 → `compare_heads` 返回 `None` → 汇报"无更新"，全程无日志。
  3. **`update_available` 有三个所有者。** patrol 只 emit `patrol://skill-checked` 事件、从不落盘，所以它的发现活不过重启，并在重启前与 JSON snapshot 长期不一致。
  4. **CLI 绕过 update 事务。** `cmd_update` 只做 `git_ops::check_update` + `pull_repo`，跳过 lockfile hash 写入、同 repo 兄弟扇出、Agent relink、项目 cascade 和 update state 清除；对 repo-cached 技能还在错误的目录层级 pull。
- Fix: `repo_link` 独占链接判定并统一走 `read_link_resolved`；subtree hash 合并进 `git_ops::compute_subtree_hash`（内部 `run_git` → `command_with_path`）；`update_state` 独占 update 可用状态，三个写入者全部写穿，陈旧判定按技能名 revision 在该 module 内裁决；CLI 与 GUI 共用 `skill_update` 事务。
- Files: `crates/skillstar-skills/src/{repo_link.rs,update_state.rs,update_checker.rs,git/ops.rs,installed_skill.rs,skill_update/}`、`crates/skillstar-app/src/cli/manage.rs`、`src-tauri/src/core/patrol.rs`。
- Self-check:
  - `grep -rn 'Command::new("git")' crates/` 应只命中测试 fixture；产品代码一律走 `command_with_path`（否则 GUI 从 Finder 启动时静默失效）。
  - `grep -rn 'repos_cache_dir' crates/skillstar-skills/src/` 中做「是不是 repo cache 链接」判定的只能是 `repo_link`；`repo_scanner::{cache,detect,maintenance}` 命中的是 cache 目录管理，不是该判定。
  - `grep -rn 'junction::get_target' crates/` 应只命中 `fs_ops::read_link_resolved`，以及 `content::read_raw_link_target` 这一个有意的例外 —— 内容快照要 hash 链接的字面目标，解析成绝对路径会让 hash 依赖机器。除此之外，手写 `std::fs::read_link` + junction 回退就是在复制 `read_link_resolved`；这类手抄版本次共清理掉四份。
  - `cargo test -p skillstar-skills repo_link update_state skill_update::plan`。
  - 通用判据：一个事实如果存在两个入口、且能给出不同答案，那就是分叉，无论两份代码看起来多像。

## 2026-07-12 - Grok account switch could overwrite a working CLI session with a billing-only token

- Symptom: switching the active Grok Usage card succeeded in SkillStar, but launching Grok immediately opened browser OAuth again.
- Root cause (confirmed by deterministic tests and the local Grok log):
  1. The xAI authorize URL still requested only `openid profile email offline_access grok-cli:access api:access`; it omitted the Grok CLI's required `conversations:read` / `conversations:write` scopes.
  2. The generic CLI writer treated any non-empty access token as valid. An older billing-capable token was written over a working `~/.grok/auth.json` entry and reported as success even though Grok rejects that session.
  3. The documented protections (per-account full-entry snapshot, private file mode, write-after-read verification) were not implemented behind a single testable seam, so code and the backend SSOT had drifted independently.
  4. SkillStar could write an entry without Grok's required `create_time`; `~/.grok/logs/unified.jsonl` repeatedly recorded `auth disk state ... Unreadable ... missing field create_time`, after which Grok launched browser login.
  5. Usage refresh could rotate the active xAI refresh-token generation only in the card store while leaving `auth.json` stale, and SkillStar did not cooperate with Grok's official refresh lock/adopt-sibling protocol.
- Fix: isolate Grok activation in `skillstar_app::usage_switch::grok` behind the `activate_subscription` / `resync_active_subscription` facade; give it a private encrypted, cross-process-locked session store and sole ownership of stable-identity outgoing capture, target restore/merge, schema/scope/effective-expiry validation, atomic `0600` write, post-write verification, and active-pin commit/reconciliation/rollback. Add the missing OAuth scopes; normalize every entry source to Grok's required schema; cooperate with the official `auth.json.lock` across sibling adoption, refresh and active CLI projection; preserve refresh/id tokens only when the same xAI subject is proven; immediately reactivate an active row after OAuth; serialize refresh/edit/delete/switch under one cross-process catalog lock; persist fetch results through narrow field patches; prefer a matching live disk session without downgrading newer cards; distinguish pre/post-replace errors; and reject known narrow-scope or unrestorable tokens before touching the working CLI file. Existing cards created with the old scope set must re-authorize once.
- Files: `crates/skillstar-usage/src/{fetchers/oauth/{xai.rs,xai_tests.rs},storage.rs,refresh_guard.rs}`, `crates/skillstar-app/src/usage_switch{.rs,/grok.rs,/grok/{io.rs},/grok_tests.rs}`, `src-tauri/src/commands/usage_commands.rs`, `src/features/usage/{hooks/useUsageData.ts,components/{UsagePanel,UsageCardWindow}.tsx}`, i18n, `docs/features/usage/README.md`.
- Self-check: `cargo test -p skillstar-usage oauth_scopes_include_conversations_for_grok_cli`; `cargo test -p skillstar-app usage_switch::grok`; `bunx vitest run src/features/usage/hooks/useUsageData.test.ts`; switch from a running/valid Grok account to an old narrow-scope card and confirm SkillStar reports reauthorization without changing the current `auth.json`, then re-authorize and switch successfully without browser OAuth. The written OIDC entry must contain `create_time`, and a following Usage refresh must leave the card and `auth.json` on the same access/refresh generation.

## 2026-07-12 - GitHub Actions chronic failures (Windows CI / Release / CI)

- Symptom: Actions history was almost all red — Windows CI never green; Release v0.0.3 failed all 4 platforms; CI red on Rust tests / flaky agent detection.
- Root causes (recurring patterns, not one-off flakes):
  1. **Dual lockfile drift:** local + Linux/macOS use Bun (`bun.lock`); Windows CI uses `npm ci` (`package-lock.json`). Dep changes updated package.json/bun.lock but left package-lock stale → every Windows run died at install with "Missing: … from lock file".
  2. **Release-only TypeScript:** main CI ran lint + vitest but not `bun run build` (tsc). Tagging v0.0.3 made tauri-action's beforeBuildCommand fail on unused imports / incomplete fixtures / dead type compares — after the tag existed.
  3. **Windows-only crate:** v0.0.1 Windows release E0433 `junction` until declared under `cfg(windows)` deps.
  4. **Tests vs real $HOME:** agent profile detection and `ssh_hosts` "vps-yy" tests assumed developer machine layout / SSH config; clean GitHub runners failed and poisoned shared env mutexes.
- Fix (process + gates, not all product bugs): delete failed/cancelled runs; document lessons in each `.github/workflows/*.yml` header; regenerate `package-lock.json`; add `bun run build` to CI + release pre-flight. Product fixes for remaining red tests (HOME sandbox / unused TS) are separate follow-ups.
- Files: `.github/workflows/{ci,windows-ci,release}.yml`, `package-lock.json`, `AGENTS.md` CI section.
- Self-check: after dep change, `npm ci` succeeds in a clean dir; `bun run build` is green before any `v*` tag; workflow comments list the patterns above.

## 2026-07-12 - Usage card "去续费 / 打开控制台" browser link looked dead

- Symptom: clicking the ExternalLink control on a Usage subscription card (tooltip「去续费 / 打开控制台」) did not open the system browser.
- Root cause (stack):
  1. Frontend used a bare `<a>` (`ExternalAnchor`) whose click path called `open_external_url` fire-and-forget with **no error toast**, so IPC/launcher failures looked like a dead button.
  2. `openExternalUrl` recorded the URL into the 900ms duplicate-suppress cache *before* invoke succeeded — a failed open blocked immediate retries.
  3. macOS launcher used bare `open` (PATH lookup); Dock/Finder launches with a thin PATH can miss it — use `/usr/bin/open`.
  4. Framer `Reorder.Item` card wrappers could swallow the anchor click without `stopPropagation`.
- Fix: Usage footer opens the console URL via `Button` + `openExternalUrl` with `stopPropagation` + failure toast; harden `open_external_url` with absolute launchers; only cache successful opens; ExternalAnchor also handles middle-click (`onAuxClick`) and keeps `target="_blank"` as progressive enhancement.
- Files: `src/features/usage/components/card/UsageCardFooter.tsx`, `src/lib/externalOpen.ts`, `src/components/ui/ExternalAnchor.tsx`, `src-tauri/src/commands/shell.rs`, i18n `usage.openConsoleFailed`.
- Self-check: `bunx vitest run src/lib/externalOpen.test.ts`; on a Usage card click「去续费 / 打开控制台」and confirm the default browser opens the catalog `subscription_url`.

## 2026-07-12 - Usage card "在新窗口打开" looked dead on Retina/4K

- Symptom: clicking「在新窗口打开」on a Usage subscription card did nothing visible (no floating card on the main display).
- Root cause: `open_usage_card_window` fed `monitor.work_area()` **physical** coordinates into `WebviewWindowBuilder::position`, which expects **logical** pixels. On a 4K display with UI scale 2 (logical 1920×1080, physical 3840×2160) the card was placed at x≈3460 — far past the right edge of the screen. Re-clicks only focused the already off-screen window.
- Fix: convert work-area origin/size by `scale_factor()` before cascade math; recovery path re-clamps existing off-screen card windows when the button is clicked again; surface create/show errors via toast.
- Files: `src-tauri/src/commands/usage_windows.rs`, `src/features/usage/components/card/UsageCardFooter.tsx`, i18n `usage.openInWindowFailed`.
- Self-check: `cargo test -p skillstar --lib commands::usage_windows`; on a Retina/4K Mac click「在新窗口打开」and confirm the floating card appears at the top-right of the main display (x ≈ logical_width − 360 − 20).

## 2026-07-11 - Grok revoked refresh token stayed as a raw error and re-login duplicated the card

- Symptom: refreshing a Grok card returned `400 invalid_grant: Refresh token has been revoked`; completing OAuth again created a second Grok card for the same account while the broken original remained.
- Root cause:
  1. xAI returns revoked refresh tokens as HTTP 400 OAuth `invalid_grant`, while the Grok token parser only mapped HTTP 401 to `UsageError::AuthRequired`. The command layer therefore saved a raw fetcher error instead of setting `requires_reauth`.
  2. The frontend correctly passed the edited subscription id, but the OAuth dispatcher/xAI login flow dropped it. Grok finalization always used `SubscriptionBuilder`'s fresh UUID, and storage upsert is intentionally keyed by subscription id rather than email.
- Fix: classify OAuth `invalid_grant` as `AuthRequired`; carry `target_subscription_id` through the xAI pending login worker; rebuild refreshed credentials onto the target Grok row while preserving user metadata and clearing the stale reauth/error state.
- Files: `crates/skillstar-usage/src/fetchers/oauth/{mod.rs,xai.rs,xai_tests.rs}`, `docs/features/usage/README.md`.
- Self-check: `cargo test -p skillstar-usage fetchers::oauth::xai::tests`; edit/re-authorize one existing Grok card and confirm its id/card count stay unchanged while the token and usage snapshot refresh.

## 2026-07-11 - Provider editor drawer could not be closed (X / Esc / left scrim)

- Symptom: Models 侧边供应商编辑抽屉点右上角 X、「完成」、Esc 或左侧空白遮罩后仍不关闭，像被卡住。
- Root cause:
  1. `ProviderEditorDrawer.requestClose` 在 `flush()` 返回 `validation` / `error` 时直接 `return`，不调用 `onClose()`。受控 `Dialog` 的 `open` 因此一直为 true，所有 dismiss 路径（X / Esc / overlay）全部失效。
  2. 遮罩 dismiss 只依赖 Radix outside-detect；未在 Overlay 上显式绑定点击关闭，体验不够稳。
- Fix:
  - `requestClose` 改为 best-effort `flush` 后 **始终** `onClose()`（`closingRef` 防重入）；非法 dirty 本就不会落盘，save 已 toast。
  - `DrawerShell` Overlay 显式 `onPointerDown` / `onClick` → `onOpenChange(false)`，左侧空白可关。
- Files: `src/features/models/components/provider/ProviderEditorDrawer.tsx`, `src/components/shared/DrawerShell.tsx`.
- Self-check: 打开供应商编辑抽屉 → 故意填非法 URL 使 dirty+validation 失败 → 点 X / 左侧遮罩 / Esc /「完成」均应能关闭；合法编辑关闭后应已 autosave。

## 2026-07-09 - Grok Usage card account switch looked like logout / forced browser re-auth

- Symptom: clicking「切为当前账号」on a Grok usage card updated SkillStar's active pin, but the Grok CLI behaved like a logout (opening Grok again required browser OAuth). Earlier reports also described the CLI keeping the previous account or the write looking incomplete.
- Root cause (stack):
  1. **OAuth scope mismatch (primary):** SkillStar's Grok login requested `openid profile email offline_access grok-cli:access api:access` only. The Grok Build CLI issues tokens that also include `conversations:read conversations:write`. Billing worked with the narrower scope, but overwriting `~/.grok/auth.json` during switch replaced a CLI-valid token with a CLI-invalid one → interactive re-auth.
  2. `set_active_subscription` could write **stored** tokens without refreshing first — near-expired keys made the CLI re-auth the old session.
  3. Grok `auth.json` writer omitted `create_time` / `coding_data_retention_opt_out` and (before JWT fill) `team_id` / `principal_*` that `grok login` stores.
  4. No post-write verification — a live `grok` process can race and rewrite the old account, while SkillStar still reported success.
  5. Usage floating card window swallowed switch errors (no toast) and only showed「重新同步到 CLI」after an in-memory `switch_result` failure (lost on reload).
- Fix: request the full Grok CLI scope set in `xai` OAuth; OAuth refresh before CLI write; JWT claim fill + `create_time` / `coding_data_retention_opt_out`; `auth.json` mode `0o600` + re-read verify; card/grid always offer CLI re-sync; surface switch outcome via toast. **Existing Grok subscriptions logged in before the scope fix must re-authorize once** so stored tokens pick up `conversations:*`.
- Files: `crates/skillstar-usage/src/fetchers/oauth/xai.rs`, `crates/skillstar-app/src/usage_switch.rs`, `src-tauri/src/commands/usage_commands.rs`, `src/features/usage/components/{UsageCardWindow,SubscriptionCard}.tsx`, `docs/features/usage/README.md`.
- Self-check: `cargo test -p skillstar-app usage_switch` + `cargo test -p skillstar-usage oauth_scopes_include_conversations_for_grok_cli`; decode a stored token's JWT `scope` claim and confirm it includes `conversations:read`; switch two Grok rows and confirm `~/.grok/auth.json` OIDC entry `email`/`user_id`/`key` match the target (close running `grok` sessions if verify fails with "写入被覆盖").

## 2026-06-15 - GUI/Tauri command core logic lacked direct test coverage

> Historical coverage snapshot. The project-path assertions below were superseded on 2026-07-14 by the shared `.agents/skills` design in D-007: Codex now participates in an ambiguous universal group, and OpenClaw supports its upstream project-level `skills` directory.

- Symptom: the Tauri commands `detect_project_agents`, `get_storage_overview`, and MCP `sync_server_to_tool` delegate to pure functions in workspace crates, but those functions had zero unit tests — only the surrounding Tauri wrappers exercised them at runtime. A regression in agent detection rules, symlink handling, or MCP sync skip-logic could ship unnoticed.
- Coverage added (13 new tests across 4 modules):
  - `detect_project_agents` (skillstar-projects/scan.rs, +4): detects codex when `.codex/skills` exists; does NOT detect when only the parent `.codex` exists (AGENTS.md "strictly on the skills dir itself"); detects multiple distinct agents with zero ambiguity; openclaw never appears at project level (global-only, empty `project_skills_rel`).
  - `dir_size_recursive` + `count_hub_skills` (src-tauri/commands/github.rs, +2): symlink target content contributes 0 bytes (1 MB via symlink excluded, only the real 5-byte file counted); valid dir / valid symlink / broken symlink / stray file all classified correctly.
  - MCP `sync_server_to_tool` / `sync_server_all_tools` (skillstar-models/mcp/tests.rs, +2): unknown tool_id surfaces an error instead of silent success; `sync_server_all_tools` returns exactly `MCP_TOOL_IDS.len()` results, one per known tool.
  - Usage `local_import` (skillstar-usage/local_import.rs, +4): missing auth.json, empty `{}`, blank access_token, unsupported catalog_id — all return clear user-facing messages (see dedicated entry below).
- Note: a first draft of the symlink-size test placed the symlink target *inside* the scanned root, which made it count as a normal subdirectory and falsely appeared to reveal a bug in `dir_size_recursive`. The function is correct; the test was restructured to put the target outside the root. Recorded here so the trap isn't re-hit.

## 2026-06-15 - Usage local_import had no error-path test coverage

- Symptom: `import_subscription_from_local` (codex/antigravity) is the entry point for one-click local credential import, but `local_import.rs` had zero unit tests — its failure modes (missing file, empty `{}` JSON, blank access_token) were only exercised manually.
- Risk: a regression that silently creates a subscription from a blank credential, or gives an unhelpful error, could ship unnoticed.
- Fix: added 4 tokio tests using a `$HOME`-scoped temp dir guard (serialized via a mutex since `home_dir()` reads `$HOME` under `cfg(test)`): missing auth.json, empty `{}` object, blank access_token, and unsupported catalog_id. All error paths now assert the user-facing message mentions the right field. skillstar-usage goes from 47 → 51 tests.
- Verified live: `~/.codex/auth.json` on this machine is `{}`, and the new test confirms that correctly yields "auth.json 缺少 tokens" instead of a crash or silent subscription.

## 2026-06-15 - Deep-link event emitted by backend has no frontend listener

- Symptom: SkillStar registers the `skillstar://` URL scheme and the backend (`src-tauri/src/lib.rs:emit_deep_link`) parses incoming `skillstar://...` URLs and emits a `skillstar://deep-link` Tauri event with the parsed payload. But no frontend code subscribes to that event, so opening a `skillstar://` URL does nothing visible.
- Investigation: `src/hooks/useTauriSetup.ts` listens to `skillstar://window-hidden` and `patrol://enabled-changed`, but a repo-wide search for `deep-link` / `deepLink` / `DEEP_LINK` in `src/` finds no `listen(...)` call for `skillstar://deep-link`. The backend half is wired but the frontend consumer is missing.
- Status: recorded as an in-progress gap, not fixed here — the current branch (`refactor/models-frontend-ia`) is mid frontend refactor and the deep-link consumer likely belongs to that work. When wiring it up, register a listener in `useTauriSetup.ts` that routes the parsed `{ host, path, query }` payload to navigation (e.g. open a skill detail / models drawer) the way `useNavigation` already models drawer deep-link requests.

## 2026-06-15 - AI skill pick errored instead of falling back when provider unreachable

- Symptom: `pick_skills` returned a hard error ("All 3 AI skill-pick rounds failed") whenever the configured AI provider was unreachable (e.g. ollama not running, wrong endpoint). The UI got no recommendations at all instead of a local-ranked fallback.
- Root cause: when `raw_success_count == 0` (all 3 consensus rounds failed at the network level), the function called `anyhow::bail!` and exited before reaching the existing `fallback_used` branch lower down. So the deterministic local shortlist that AGENTS.md requires ("fall back to deterministic local ranking when AI output is partial or invalid") was never reached on a total transport failure — only on parse failures.
- Fix: `pick_skills` now logs the all-rounds-failed case and returns a `SkillPickResponse` with `fallback_used = true, rounds_succeeded = 0` populated from `fallback_skill_pick(&ranked_candidates)`, so the UI can still show recommendations and indicate the fallback state. Verified by a new tokio test pointing at `127.0.0.1:1` (connection refused): returns in ~1.5s with the name-matching candidate ranked first.
- Note: `test_connection` and `summarize_text` correctly keep returning `Err` on transport failure — they have no meaningful fallback, unlike skill pick which has a pre-computed local ranking.

## 2026-06-15 - CLI commands find/remove/init hung launching the GUI

- Symptom: `skillstar find`, `search`, `remove`, `init` (and their aliases) appeared to hang for minutes when run from the `skillstar` binary, eventually timing out. Other commands (`list`, `install`, `doctor`) returned instantly.
- Root cause: `src-tauri/src/main.rs` routes to CLI mode only when `args[1]` is in a hard-coded `cli_commands` allow-list. That list predated several commands and was missing `find`/`search`/`remove`/`rm`/`uninstall`/`init`/`help`. Any missing subcommand fell through to `skillstar_lib::run()` and started the full Tauri GUI, which blocks indefinitely in a terminal/headless context.
- Fix: the `cli_commands` list now mirrors every variant + alias declared in `skillstar_app::cli::Commands` (including aliases like `search`, `rm`, `add`), with a comment tying the two together so future commands are not silently dropped.

## 2026-06-15 - CLI find used a throwaway empty marketplace DB

- Symptom: even after the routing fix, `skillstar find <q>` was slow and reported `snapshot: Seeding`/`RemoteError` instead of `Fresh`, despite `~/.skillstar/db/marketplace.db` holding 54k+ rows.
- Root cause: the marketplace snapshot runtime defaults to `std::env::temp_dir().join("skillstar-marketplace")` when nobody calls `configure_runtime`. GUI mode configures it during `setup`, but the CLI entry point's `migrate_and_run` hook only ran legacy path migration. So `find` opened an empty `/tmp` DB, hit the "no skill rows" branch, and triggered a blocking remote seed against skills.sh on every search.
- Fix: the Tauri CLI's `migrate_and_run` now also calls `core::marketplace::initialize_local_snapshot()` so every CLI command shares the real `~/.skillstar/db/marketplace.db`. `find` now returns `Fresh` in ~2s.

## 2026-06-15 - CLI failures returned exit code 0

- Symptom: `skillstar publish` and `skillstar update` printed `✗ ...` error messages but exited with code 0, so shell scripts and CI could not detect failure.
- Root cause: `cmd_publish`'s error branches (gh not installed, not authenticated, publish error) and `cmd_update`'s lockfile-read failure used `eprintln!` + bare `return` instead of `std::process::exit(1)`.
- Fix: both commands now exit non-zero on error, matching the convention already used by `install`/`remove`/`init`.

## 2026-06-15 - remove falsely reported deleting non-existent skills

- Symptom: `skillstar remove <typo> --yes` printed `✓ Removed 1 skill(s): <typo>` and exited 0 even though the skill was never installed, giving no feedback for misspelled names.
- Root cause: `uninstall_skill` treats a missing hub entry as a no-op and returns `Ok(())`, so `cmd_remove` counted it as removed.
- Fix: `cmd_remove` now checks existence (local skill / hub dir / lockfile entry) before calling uninstall, and reports `• '<name>' is not installed; nothing to remove.` for names that are absent instead of a misleading success.

## 2026-06-15 - remove did not accept comma-separated names

- Symptom: `skillstar remove a,b --yes` treated the whole string `a,b` as a single skill name and reported "not installed", while `install --skill a,b` correctly split on the comma.
- Root cause: the `Remove.names` clap field had no `value_delimiter`, so only whitespace-separated names worked. `install`'s `--skill`/`--agent` flags already used `value_delimiter = ','`.
- Fix: `Remove.names` now uses `value_delimiter = ','`, so both `remove a b` and `remove a,b` work, matching the rest of the CLI.

## 2026-06-11 - Xiaomi MiMo Token Plan usage was manual-only

- Symptom: Xiaomi MiMo subscriptions could only be maintained manually, so refreshing usage never fetched the Token Plan quota shown in the MiMo console.
- Root cause: the usage catalog registered `xiaomi-mimo` as `Manual` only, and the cookie fetcher dispatcher had no MiMo implementation. MiMo's console uses browser-session GET calls to `/api/v1/tokenPlan/detail` and `/api/v1/tokenPlan/usage`; the latter returns `usage.items[]` with `name`, `used`, `limit`, and fractional `percent` fields.
- Fix: `xiaomi-mimo` now supports Cookie + Manual auth. The new MiMo cookie fetcher reads the detail and usage endpoints with the pasted browser Cookie, maps the primary Token Plan item to a monthly quota window, preserves compensation Credits as credit metadata, and treats 401/403 as reauth-required.
- Follow-up: the Cookie-mode setup text is provider-specific now. MiMo is Cookie-only, no longer shows the manual auth option or OpenCode workspace instructions, and the dialog includes a quick console link for the selected provider.

## 2026-06-11 - Agent detection created empty skills folders

- Symptom: simply loading agent profiles could create empty skills folders for every built-in agent, even if the user had never used or installed that agent. Empty project-level agent folders could also be left behind when quick deploy was called with no deployable skills.
- Root cause: agent install detection provisioned `global_skills_dir` during read-only profile listing, which made every built-in profile look installed and enabled by default. Batch/project deploy paths also created target directories before confirming a source skill existed.
- Fix: profile detection is now read-only and treats an existing agent config root as installed without creating `skills/`. Toggle defaults mirror that read-only detection. Global batch link and project incremental deploy now create target directories only after finding a real hub skill to deploy, and prune a newly-created project target if nothing was linked.
- Superseded: D-009 于 2026-07-14 完全移除了本机 Agent 安装探测与探测默认值；本条保留为历史事故记录，当前实现只读取手动激活偏好。

## 2026-06-07 - Agent tool model metadata was not persisted for OpenCode

- Symptom: pulling models populated model IDs, but OpenCode sync could not reliably write enriched `name`, `limit`, and `cost` fields, and Codex `wire_api` / auth mode changes could be lost after saving.
- Root cause: the provider form reused `modelCatalog` as a local string-list variable while building the save patch, overwriting `meta.model_catalog` with plain IDs. The flat provider patch also did not apply `codex_wire_api` and `codex_auth_mode` to the provider entry fields.
- Fix: model IDs and normalized catalog metadata are now stored separately, OpenCode sync reads structured metadata from `provider.meta.model_catalog`, and flat provider updates persist Codex API/auth settings directly.

## 2026-06-07 - SKILL.md translation skipped mixed English/Chinese content and lost reuse after restart

- Symptom: SKILL.md translation could return too quickly with untranslated English when the document already contained enough Chinese text, and repeated translations after restarting the app still had to call the AI provider again.
- Root cause: the Chinese-target skip heuristic only checked CJK ratio, so mixed documents could be treated as already translated. The translation cache was session-scoped memory only, despite the UI contract expecting backend-owned durable reuse.
- Fix: translation now only skips content that is clearly already target-language Chinese, keeps mixed English/Chinese segments eligible for translation, and stores both segment-level and whole-document translation results in `~/.skillstar/db/translation_cache.db`.
- Superseded: 2026-07-14 起 SKILL.md 翻译及其缓存已由 ACP 全目录图文教程替代；本条只保留为历史故障记录。

## 2026-06-06 - Antigravity Usage credits loaded but model quota stayed empty

- Symptom: Antigravity refresh could save plan/credits (`G1-PRO-TIER`, `GOOGLE_ONE_AI`) while model quota bars stayed empty.
- Root cause: Antigravity model quota is returned by `fetchAvailableModels`, and the management-center reference tries `daily-cloudcode-pa`, `daily-cloudcode-pa.sandbox`, then `cloudcode-pa` with `{ "project": projectId }`. SkillStar only called the prod endpoint and sent the Cloud Code Assist project field shape, so the quota response could be empty even after `loadCodeAssist` worked.
- Fix: Antigravity model quota refresh now follows the same endpoint fallback order and parses the management-center quota groups into SkillStar `UsageWindow` breakdown entries.

## 2026-06-06 - Antigravity Usage login succeeded but card showed no usage data

- Symptom: after Antigravity OAuth login succeeded, the card still showed `未录入用量数据。点击编辑按钮维护。`.
- Root cause: Antigravity reused Gemini CLI's Google OAuth client and Cloud Code metadata payload. That can complete Google login but does not match Antigravity IDE's control-plane flow, so `loadCodeAssist` may return no paid credits or quota windows. The card also rendered the manual empty-state whenever hourly/weekly/monthly/balance were absent, even if `usage.credits` was present.
- Fix: Antigravity now uses its own Google OAuth client id/secret, scopes, refresh credentials, and `metadata.ideType = "ANTIGRAVITY"` for `loadCodeAssist`. The card treats `credits` and `api_keys` as real usage data and no longer overlays the manual empty-state when either is present.

## 2026-06-06 - Antigravity Usage refresh returned opaque loadCodeAssist 400

- Symptom: after Antigravity OAuth login succeeded, refreshing usage showed `fetcher error: loadCodeAssist 状态 400 Bad Request` without the Google response body.
- Root cause: the Cloud Code Assist request still used the older `{ clientMetadata: {} }` / `project` payload shape. Current Google Cloud Code Assist endpoints expect the Gemini CLI-style `metadata` payload and, when a project is known, `cloudaicompanionProject` plus `metadata.duetProject`.
- Fix: the shared Google Cloud Code helper now sends the current metadata payload, includes response body text for non-success `loadCodeAssist` responses, and Antigravity retries once without a cached project id when a project-scoped request returns 400.

## 2026-06-06 - Antigravity Usage OAuth failed with missing Google client_secret

- Symptom: logging in to Antigravity from Usage failed with `Usage: fetcher error: Google token 返回：{ "error": "invalid_request", "error_description": "client_secret is missing." }`.
- Root cause: Antigravity's Google OAuth code exchange reused the same Google OAuth client id as Gemini CLI, but did not include the required `client_secret` parameter in the `/token` request. Google rejected the authorization-code exchange before any Cloud Code quota call ran.
- Fix: Antigravity OAuth token exchange now sends the matching Google `client_secret`, aligning it with Gemini CLI login and the shared Google refresh path.

## 2026-05-24 - Cursor usage refresh showed ambiguous empty-data feedback

- Symptom: refreshing a Cursor usage card could show `已刷新，但没有拿到可展示的用量数据` after the network request succeeded.
- Root cause: the frontend treated any snapshot without quota windows, balance, credits, or API keys as a generic empty result. Cursor can return a successful `usage-summary` response that the current UI cannot turn into display bars, which made the state look like a silent no-op.
- Investigation: the logged-in Cursor Dashboard currently calls `/api/dashboard/get-current-period-usage`, `/api/dashboard/get-monthly-invoice`, `/api/dashboard/get-monthly-billing-cycle`, `/api/dashboard/get-credit-grants-balance`, and `/api/dashboard/get-aggregated-usage-events`. Directly calling these with SkillStar's old OAuth-derived WorkOS cookie returns `403`, so they appear to require the full browser Dashboard session.
- Fix: usage refresh feedback now distinguishes transport errors from empty snapshots and uses a Cursor-specific explanation when the provider returns only plan data or no recognizable quota fields.

## 2026-05-24 - Usage OAuth and marketplace startup failed on direct network

- Symptom: Google-family Usage OAuth failed with `Google token：error sending request`, and startup logged `marketplace_snapshot: startup refresh failed scope=leaderboard_all`.
- Root cause: the local SkillStar proxy config can be disabled while the machine cannot direct-connect to Google/GitHub/skills.sh. Usage already used the proxy-aware client, but marketplace remote fetches used a bare cached `reqwest::Client`, so they ignored later proxy configuration.
- Fix: marketplace remote HTTP now uses `skillstar_core::infra::http_client::probe_http_client`, sharing the app proxy config and rebuilding when proxy settings change. Usage command errors now append a network/proxy hint for transport failures.

## 2026-05-21 - OpenCode usage API readiness shown as fetcher failure

- Symptom: refreshing OpenCode usage showed `fetcher error: OpenCode API 暂未就绪（已尝试 3 个端点，均返回 200）`.
- Root cause: `https://api.opencode.ai/{v1/usage,api/usage,usage}` currently returns HTTP 200 with a plain `Not Found` body. The fetcher attempted JSON parsing and skipped all candidate endpoints, then surfaced the expected upstream limitation as a hard fetcher error.
- Fix: OpenCode OAuth and Cookie fetchers now read the response text, explicitly treat `Not Found` bodies as missing routes, and return a valid `SubscriptionUsage` snapshot with an inline `error` message instead of failing the refresh command.
- Follow-up: this is not caused by missing User-Agent or anti-bot behavior. The current OpenCode CLI uses `https://console.opencode.ai` for account/control-plane APIs (`/auth/device/*`, `/api/user`, `/api/orgs`, `/api/config`). The old `api.opencode.ai/*/usage` probes were removed because they are not official usage endpoints.

## 2026-05-21 - OpenCode reauthorization created duplicate subscription cards

- Symptom: after an OpenCode subscription required reauthorization, completing OAuth added a second OpenCode card while the old card still showed `登录已失效，请重新授权。`.
- Root cause: the OpenCode OAuth callback always constructed a subscription with a fresh UUID. `storage::upsert_subscription` only matches by subscription `id`, so it could not refresh the existing card.
- Fix: `start_oauth_login` now accepts the current subscription id when launched from an edit/reauthorization flow. The pending OAuth session stores that target id, and the OpenCode OAuth finalizer reuses it to replace tokens and clear `requires_reauth` while preserving user metadata.

## 2026-05-21 - OpenCode reauthorization still showed stale auth failure

- Symptom: after the duplicate-card fix, reauthorizing OpenCode could still leave the old card showing `登录已失效，请重新授权。`.
- Root cause: the target subscription id was attached to the pending OAuth session after the OpenCode worker had already been spawned, so the worker could race and read `None`. Also, when token exchange succeeded but the follow-up console probe failed, the old auth-failure usage snapshot was left untouched. Later refreshes also treated OpenCode console probe 401/403 as credential failure even though the just-issued OAuth token could be valid while the usage/control-plane endpoint is unavailable to this client.
- Fix: the target subscription id is now passed into `fetchers::oauth::start_login` and stored before the OpenCode worker starts. OpenCode OAuth completion now always writes a fresh post-login usage snapshot, using a non-auth warning when the token is valid but live usage probing is unavailable. OpenCode refresh now only returns `AuthRequired` when token refresh itself fails; control-plane probe failures become inline warnings instead of reauth state.

## 2026-06-19 - Skill pack post_install scripts failed on Windows; bundle path guard missed Windows-style entries

- Symptom: a skill pack whose `skillpack.toml` declares a `post_install` script was always marked `PartiallyInstalled` with "post_install script exited with code -1" on Windows, even though every skill in the pack installed correctly. `execute_post_install` unconditionally invoked `sh -c <script>`, and `sh` is absent from PATH on Windows unless Git Bash/WSL is installed — so `Command::output()` returned `Err` and the function returned `-1`. This contradicted the rest of the codebase's "Windows uses PowerShell, not bash" convention (ACP runner, cloud_code probe, Launch Deck all gate scripting by OS).
- Root cause: `crates/skillstar-skills/src/skill_pack.rs` `execute_post_install` had no `#[cfg(windows)]` branch and no interpreter selection; it hard-coded `command_with_path("sh")`. Separately, the bundle extraction path-traversal guard (`skill_bundle.rs`, two call sites) only rejected `/`-prefixed and `..`-containing entries, so a maliciously crafted archive entry using a Windows drive prefix (`C:\...`) or backslash separators (`..\foo`) could slip past it — defense-in-depth only, since legitimate `.ags`/`.agd` bundles written by this module always use `/`-delimited entries.
- Fix:
  - `execute_post_install` now selects the interpreter by platform + script extension: Unix always uses `sh -c` (backward compatible with existing bash scripts); Windows uses `powershell -NoProfile -ExecutionPolicy Bypass -File` for `.ps1`, `cmd /C` for `.bat`/`.cmd`, and falls back to `sh -c` for `.sh`/extensionless so a Git Bash install still runs bash scripts. The selection logic was extracted into a `post_install_interpreter(ext)` helper (returning a `PostInstallInterpreter` enum) so it has cross-platform unit-test coverage (2 new tests in `skill_pack.rs`).
  - A missing interpreter now logs a readable `tracing::warn!` (with the program name and the underlying io error) before returning the `-1` sentinel, instead of swallowing the error silently.
  - Bundle extraction now uses a shared `is_unsafe_archive_path` predicate that also rejects backslash separators and Windows drive-letter prefixes (`C:`, `c:`). Both extraction sites (single-skill and multi-skill import) route through it. 3 new tests in `skill_bundle.rs` cover safe relative paths, Unix absolute/traversal, and Windows-style paths.
- Verified: `cargo test -p skillstar-skills` 58 passed (was 53; +5 new), `cargo clippy -p skillstar-skills` introduces no new warnings, `cargo check --workspace` clean. Windows-specific branches are exercised by the interpreter-selection unit tests on every platform.

## 2026-06-25 - Models workbench: Claude Code wrote empty ANTHROPIC_MODEL; ZCode removed as provider tool

- Symptom: 模型工作台 (Models Hub) 功能异常。用户机器上 `~/.claude/settings.json` 的 `env` 块出现 `"ANTHROPIC_MODEL": ""`（空字符串），导致 Claude Code 模型解析失效。
- Root cause: `sync_to_claude_code_inner` 无条件把 `model` 参数写进 `ANTHROPIC_MODEL`。当 provider 未设 `default_model` 且激活时未显式指定 model 时，链路（前端 `useAgentActivation.activate` → `activate_tool` 命令 → `crud::activate_tool` 的 model resolution → `sync_to_claude_code`）会让 model 解析成空字符串 `""`，原样写入，产生无效配置。同一次还移除了 ZCode 作为模型工作台 provider tool（`sync_to_zcode`/`unsync_zcode` 及相关分支），但保留 `tool_sync::resolve_zcode_config_path()` —— 它被 MCP 子系统（`zcode_v2_opencode_mcp_remove`）和 Usage 子系统（`switch_zcode`）跨子系统复用，删除会破坏编译。
- Fix:
  - `sync_to_claude_code_inner`：`ANTHROPIC_MODEL` 改用新增的 `trim_or_null(model)` helper —— 空/空白 model 返回 `Value::Null`，由 `merge_json_env_write` 当作"移除该键"处理（与 Haiku/Sonnet/Opus 空值语义一致），不再写入无效的 `""`。
  - 模型工作台范围移除 ZCode：前端 `agentRegistry.ts`（`ProviderToolId` 联合、`PROVIDER_AGENTS`、`CONFIG_FILE_TOOLS`）、`AgentToolIcon.tsx`；后端 `tool_sync` 的 `sync_to_zcode`/`unsync_zcode`、`paths_files.rs` 各 `zcode` 分支、`backup_merge.rs` 的 `resync_active_tools` 分支、`types.rs` 注释；`providers/crud.rs` `activate_tool` 校验分支；Tauri 命令层 `tools.rs`（activate/deactivate/update_tool_settings/push_provider_to_tool_config/resync_tool/detect_tool_installation）。MCP/Usage/Projects/SSH/providers-balance 等子系统的 zcode 引用**全部保留**。
- Verified: `cargo check -p skillstar-models` 通过；`cargo test -p skillstar-models tool_sync` 51 passed（含新增 `test_sync_to_claude_code_inner_empty_model_skips_key`）；`cargo check --workspace` 通过；改动的两个前端文件 `biome check` 干净。`tool_sync::tests` 里 3 个 zcode 专用测试已删，`test_get_tool_config_targets_returns_both_tools` 的 `targets.len()` 由 5 改为 4。

## 2026-06-25 - 模型工作台所有 agent 卡片显示"未接入"（FlatProvidersResponse camelCase 序列化不匹配）

- Symptom: 在模型工作台激活 Claude（或任意 agent）后，toast 提示"已同步到配置文件"（后端 sync 确实成功，`~/.claude/settings.json` 写入正确），但卡片状态胶囊始终显示"未接入"（inactive），provider 下拉与模型选择也不出现。后端 store（`~/.skillstar/config/model_providers.json`）的 `tool_activations["claude-code"]` 数据完全正确（含 provider_id 和 model）。
- Root cause: `FlatProvidersResponse`（`src-tauri/src/commands/models_commands/mod.rs`）标注了 `#[serde(rename_all = "camelCase")]`，导致 `tool_activations` 字段被序列化成 `toolActivations` 返回给前端。而前端类型 `FlatProvidersResponse.tool_activations`（`src/types/models.ts`）及所有消费者（`activations.ts`、`providers.ts`、`useProvidersFlat.ts`、`useAgentActivation.ts`、`ModelsHub.tsx`、`devMockData.ts`）一律读 snake_case 的 `tool_activations`。于是 `data.tool_activations` 永远是 `undefined`，`toolActivations` 退化成 `{}`，`activation = data?.tool_activations?.[toolId]` 恒为 `null` → `computeAgentStatus` 走 `!activation` 分支返回 `inactive`。注意 `ProviderEntryFlat` 和 `ToolActivation` 本身**没有** `rename_all`（字段保持 snake_case），所以 provider 列表能正常工作——只有包了 `rename_all` 的 `FlatProvidersResponse` 这一层把 `tool_activations` 这个多词字段改了名，单个词的 `version`/`providers` 因 camelCase==snake_case 而未暴露问题。
- Fix: 去掉 `FlatProvidersResponse` 上的 `#[serde(rename_all = "camelCase")]`，让 `tool_activations` 保持 snake_case 与前端类型一致。该结构体只作为 `get_providers_flat` 的返回值（纯序列化输出，从不作为命令入参），去掉属性不影响反序列化入参；`version`/`providers` 是单词不受影响。
- Verified: `cargo check`（全工作区）通过；前端 `providers.test.tsx` + `activations.test.ts` 共 9 passed。前端 mock（`devMockData.ts`）与测试夹具（`providers.test.tsx`）均已使用 snake_case `tool_activations`，与修复后的后端输出一致。
- Lesson: 当一个响应结构体包了 `rename_all = "camelCase"` 而其嵌套类型没有时，多词字段会在边界处发生命名风格切换，前端按统一风格读取就会漏掉。新增/修改跨 Tauri 边界的响应结构体时，应确保字段命名风格与前端类型定义一致；若前端用 snake_case，后端响应结构体不应加 `rename_all = "camelCase"`。

## 2026-07-04 - SSH 远程 hub：单引号包裹的 `~` 从不展开 → 内容落到字面 `$HOME/~/` 目录、agent 符号链接悬空；远端脚本退出码被丢弃 → 静默失败

- Symptom: VPS skill 管理不稳定。通过 SSH push/migrate 的 skill 在 SkillStar UI 里一切正常（显示 hub_managed、push 成功），但 VPS 上的 agent CLI（claude/codex）实际读不到 skill；`git pull` / `git clone` 失败时 UI 照样提示"done"；批量 push 大于等于 10 个 skill 时中途报"open SFTP session channel"失败。
- Root cause（四个独立缺陷叠加）:
  1. **字面 `~`**：`hub.rs` 把 `~/.skillstar/hub/content/<name>` 交给 `shell_quote` 后拼进 `ln -sfn '<target>' '<link>'` —— 单引号内的 `~` shell 永不展开；SFTP 协议同样不展开 `~`（OpenSSH sftp-server 把它当普通路径分量）。结果 hub 内容经 SFTP 落进字面目录 `$HOME/~/.skillstar/...`，agent 符号链接指向相对路径 `~/.skillstar/...`（悬空）。SkillStar 自身探测（SFTP `path_exists` 同样用字面 `~`）与写入一致，所以 UI 全绿 —— 只有 VPS 上真正的消费者（agent CLI）看到坏链接。
  2. **退出码丢弃**：`exec_capture` 收到 `ChannelMsg::ExitStatus` 只用来跳出循环，值被扔掉。`pull_remote_skill`（`git pull --ff-only` 失败）、`migrate`（`mv` 失败）、`toggle` 全部静默返回 Ok；`install_remote_skill` 在 clone 失败后仍继续创建符号链接（悬空）并返回 Ok。
  3. **批量 push 通道耗尽**：`push_skill_via_hub` 每个 skill 各开一条 SFTP 通道且从不关闭；OpenSSH `MaxSessions` 默认 10，批量推 10+ 个必挂。
  4. **远端 git 可交互挂起**：`git fetch/pull/clone` 无 `GIT_TERMINAL_PROMPT=0`，https 凭证或 ssh 提示会卡满 60s `EXEC_TIMEOUT`（update-check 是每 skill 一次，分钟级卡顿）。
- Fix（crates/skillstar-ssh + commands/ssh_hosts）:
  - 新增 `hub_scripts.rs`（纯函数脚本构造器，全部可单测）：shell 脚本一律引用双引号 `"$HOME"`（远端运行时展开），SFTP 路径经 `resolve_sftp_home`（`canonicalize(".")`，非绝对路径直接报错）拼绝对路径；`validate_skill_name` 拒绝 `/`、`..`、`~`、换行。
  - `exec_capture_status` 返回远端退出码；所有变更类脚本 `set -e` + `ensure_exec_ok`，失败带远端输出上抛；install 脚本 clone 成功后才建链接。终态控制台事件改为如实（`emit_outcome`：成功 done / 失败 error）。
  - SFTP 通道每命令开一次并传入 hub 操作；批量 push 全程复用一条。
  - 远端 git 统一加 `GIT_TERMINAL_PROMPT=0 GIT_SSH_COMMAND='ssh -oBatchMode=yes'`。
  - 拨号阶段加一次自动重试（1.2s 退避；凭证在 host-key 门之后才发送，重试安全）；`open_sftp` 加 20s 超时。
  - **旧布局自愈**：每次 discovery 先跑幂等 `heal_legacy_layout_script` —— 把 `$HOME/~/.skillstar/hub/content/*` 搬回真 hub 根，并把所有以字面 `~/` 开头的 agent 链接改指绝对路径；修复数量在连接控制台以 warn 行提示。发现/分类逻辑同时兼容未自愈的旧字面 `~` 布局。
- Verified: `cargo test --workspace` 738 passed / 0 failed（skillstar-ssh 60，含新增：绝对布局与旧 `~` 布局的 discovery 分类、脚本构造器"禁止引号包 `~`"断言、git 免交互断言、heal 输出解析）；`cargo clippy -p skillstar-ssh -p skillstar` 无新告警。
- Lesson: 远端路径有两条独立的展开规则——shell 只展开未加引号的 `~`，SFTP 根本不展开。任何要跨这两个通道的路径都必须显式解析成绝对路径（shell 用 `"$HOME"`，SFTP 用 `canonicalize(".")`），并且"写入方"和"探测方"用同一个错误路径会让 bug 自洽地隐身：验证要以**第三方消费者**（VPS 上的 agent CLI）视角做。远端命令必须检查退出码，echo 标记只能用于分支分类，不能当成功判据。

## 2026-07-13 - Gemini CLI 卸载后仍出现在 Skill Agent SVG 轮播

- Symptom: 本机已找不到 `gemini` CLI，但 Skill 卡片底部仍显示 Gemini SVG，并在已链接的 Skill 上显示“Gemini CLI（移除）”。
- Root cause: `skillstar-skills` 的 CLI 安装检测把 exact skills 目录存在也视为安装信号；SkillStar 在部署 Skill 时会创建 `~/.gemini/skills`，卸载 CLI 后该目录仍可能保留，因此 `AgentProfile.installed` 错误地保持为 `true`。前端的 `installed && enabled` 过滤本身正确，但消费了错误的后端状态。
- Fix: CLI 的 skills-dir fallback 改为显式按 Agent 允许，目前仅 Codex 与 ZCode 保留兼容兜底；Gemini 必须检测到 `gemini` binary 或受支持的桌面应用信号。前端继续复用 `selectTargetableAgentProfiles`，不添加 Gemini 特判，避免 Settings、项目部署和 Skill 卡片状态分叉。
- Self-check: 在 hermetic tempdir 中创建 Gemini skills 目录，但让 binary 不存在，`detect_installed` 必须返回 `false`；同样场景下 Codex/ZCode 的兼容 fallback 必须保持 `true`；前端 `installed=false, enabled=true` 的 profile 不得进入轮播候选。
- Superseded: D-009 于 2026-07-14 删除了 `detect_installed`，上述 fallback 与 self-check 不再属于当前实现；本条只解释旧版本根因。

## 2026-07-14 - 共享 Agent skills 目录导致本机 Agent 被误发现和误启用

- Symptom: Skill、卡组和 MCP 卡片底部的 Agent SVG 列表与 Settings 的实际 Agent 状态不一致；已经卸载的 Agent 仍可能出现在 Skill/卡组里，MCP 卡片则固定显示全部受支持工具。
- Root cause: `installed` 曾由 binary、桌面应用、配置根和 skills 目录综合推断，并被当成 `enabled` 的默认值。目录是部署目标而不是身份信号；共享 `~/.agents/skills` 或卸载残留会让探测产生不可消除的 false positive。继续叠加更多例外只会把不同 Agent 的生命周期耦合到同一路径。
- Fix: 删除本机 Agent 安装探测、探测元数据和探测驱动的默认值；所有 profile 默认关闭，Settings 开关成为唯一激活来源。Skill、卡组、Project、CLI 隐式目标和 MCP rail 统一按手动 `enabled` 投影；MCP rail 不再叠加 tool 安装探测。冻结 IPC 字段 `installed` 仅镜像 `enabled`。
- Self-check: 即使 PATH、应用目录、配置根和共享 skills 目录都存在，空偏好注册表仍必须返回全部关闭；首次手动 toggle 后对应 profile 才进入各 rail；关闭后立即消失。MCP adapter 覆盖 `claude -> claude-code` 映射和 Settings 顺序，不再需要 tool status 才显示目标。

## 2026-07-10 - Claude Code 统一后旧 Desktop Chat MCP 可能变成不可见孤儿

- Symptom: 移除独立 `claude-desktop` Agent 后，旧版 SkillStar 已写入 `claude_desktop_config.json` 的 MCP 仍可能在 Desktop Chat 中运行，但主 store/UI 不再显示或清理它；若配置 JSON 损坏，宽松解析还可能把整个文件重写为空对象。
- Root cause: Claude Code 的 Desktop Code 与 CLI 共享 `~/.claude.json`，而 Desktop Chat 的 `claude_desktop_config.json` 是官方明确分离的产品配置。直接删除旧 adapter 会丢失清理证据；直接保留隐藏写入又会在用户不可见时继续投影命令和凭证。
- Fix: 公开层只保留 `claude-code`。旧 `claude-desktop=true` 仅作为 cleanup tombstone：永不 upsert，只按旧名称严格解析并移除对应 Chat MCP；成功后持久化为 `false`，失败保留 `true` 供重试。rename/delete 使用克隆 store 做事务式编排，清理失败不提交新名称/删除状态；严格 JSON 清理保留其它字段与 server，malformed 输入原文不动。前端过滤 legacy id，并统一显示投影失败警告。
- Self-check: 覆盖 legacy true/false、rename old/new、delete、malformed JSON 保留 store 证据、其它 Chat 字段不变，以及公开 `MCP_TOOL_IDS` 只有一个 Claude Code。

## 2026-07-10 - Models Agent 设置切换供应商会串写上一家的参数

- Symptom: 在 Agent 设置中修改供应商 A 的 Claude/Codex 参数后，若在自动保存完成前切换到供应商 B，界面可能继续显示 A 的草稿，并把它保存进 B；原始配置文件有未保存内容时，切换、重载、同步或关闭也可能静默覆盖草稿。
- Root cause: `AgentSettingsDialog` 的参数草稿只有一份 `params`，没有绑定 `provider.id`；供应商切换后 `persisted` 已指向 B，但旧 `params` 仍参与 dirty/save。`AgentConfigFiles` 的文件切换、格式化、重载与同步也没有把 `dirty` 当作覆盖保护条件，弹窗关闭状态更未感知该 dirty。
- Fix: 参数草稿改为 `{ providerId, values }`，保存与渲染只消费当前供应商的草稿；切换供应商和关闭前先 flush 参数自动保存，失败时留在当前界面。原始配置 dirty 会同步到弹窗保存状态，并阻止关闭、切文件、格式化、重载、同步、重绑与断开，直到用户显式保存。
- Self-check: 编辑 A 后立即切 B，B 必须显示自身参数且 A 的值不能写入 B；原始配置 textarea 变脏后，所有会替换内容或离开弹窗的入口都必须保持禁用/被拦截，保存成功后才能恢复。

## 2026-07-14 - Agent 已链接技能明细为空但徽标仍显示旧计数

- Symptom: 在 Settings 展开 Agent 后，明细已经显示“暂无已链接的技能”，同一行的徽标仍显示例如“3 已链接”；逐个解绑最后一个技能或在外部手动删除部署后都可能触发。
- Root cause: Agent 行用 `linkedSkillNames.length || profile.synced_count` 计算徽标。空数组长度 `0` 是已经加载完成的真实结果，却被 JavaScript 的逻辑或当成假值，错误回退到 Settings 首次加载时的 `synced_count` 快照；展开明细则直接读取空数组，于是同一张卡片出现两套真相。
- Fix: 保留“明细未加载时用 `synced_count` 摘要”的首屏能力，但用 `undefined` 区分未加载、用空数组表达已加载且为零；明细加载后计数与内容统一读取同一数组。展开后的零计数仍显示收起按钮，收起后隐藏零计数徽标。
- Self-check: 用 `synced_count=3` 渲染未加载行时应显示初始摘要；随后传入该 Agent 的空明细数组时，展开内容应显示“暂无已链接的技能”，徽标必须变成 `0` 而不能继续显示 `3`，并且仍可收起。

## 2026-07-14 - 来源下拉里滚动鼠标滚轮会横向滚动 topbar

- Symptom: 窗口较窄、topbar filters 溢出时，打开“来源”下拉并在选项上滚动鼠标滚轮，topbar 会跟着横向滚动，下拉与其锚点错位。
- Root cause: `PageToolbar` 的 filters 容器 `<div ref={filtersRef} onWheel={...}>` 用滚轮量驱动横向滚动。来源下拉是 Radix `Popover.Content`，通过 `Popover.Portal` 渲染到 `document.body`，DOM 上不在 filters 内部；但 React synthetic event 沿 **React 组件树** 冒泡，会跨 portal 传播回这个仍是其 React 祖先的 `onWheel` handler。handler 只判断 `scrollWidth > clientWidth`，没有校验事件是否真的源自 filters 的 DOM 子树，于是把下拉里的滚动也当成 filters 滚动执行。
- Fix: 在 `onWheel` 开头加 `if (!el.contains(e.target as Node)) return;` DOM 归属守卫。portal 出去的下拉内容不是 `el` 的 DOM 后代会被直接跳过；真正的 filter pills 是 `el` 后代，横向滚动照常。用真实 DOM 关系而非 React 树关系判断事件归属。
- Self-check: 窗口窄到 filters 溢出时，鼠标在真正的 filter pills 上滚动仍应横向滚动 topbar；打开来源（或任何 portal 下拉）后在其内容上滚动，topbar 必须保持不动。任何经 portal 渲染、但 React 树上是 filters 后代的浮层，都不应再触发 topbar 滚动。
