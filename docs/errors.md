# Error Log

状态：active

## 2026-09-01 - 跨 host 复用 ETag 造成假 304；SOCKS5 本地 DNS 被污染

- Symptom: 启用 marketplace / GitHub 加速后，商店同步“成功”但内容停在旧版；GitHub clone 卡在 DNS 或连到错误 IP；镜像 Test 显示可达，实际 git/raw 失败。
- Root cause: (1) `fetch_with_failover` 把上一 host 的 ETag 带到下一 host，加速源用自己的 validator 答 304。(2) `socks5://` 让 reqwest/git 在本地解析，GFW 污染把 `github.com` 指到黑洞。(3) `test_mirror` HEAD 加速源根，根路径 200 不代表 raw/git 代理可用。(4) `insteadOf` 只改写 `github.com`，raw/codeload/objects 仍直连。
- Fix: `If-None-Match` 绑定 `source_host`；SOCKS5 出网改 `socks5h`；探测改为 GET 公开 raw README；GitHub 族匿名 URL 经健康加速源包装；连续失败熔断。见 [D-050](decisions.md#d-050对抗审查加固熔断排序socks5hgithub-族匿名改写etag-绑-host)。
- Self-check: `cargo test -p skillstar-core --locked --lib -- github_health github_rewrite github_mirror proxy http_client github_http network_doctor`；`cargo test -p skillstar-marketplace --locked --lib -- etag_is_only_sent_to_the_host_that_issued_it enabled_github_mirrors_wrap_skills_sh`。

## 2026-08-31 - 安装必须是一条 vercel-skills 管线，harness 文件夹是 identity 别名

- Symptom: rust-skills / impeccable / ui-ux-pro-max-skill 的轮播和 `--agent` 走 scan 列表、chooser、整仓 clone 三条机器，缺 `.dsh` 或只有 `.claude` 时 fail-closed 或链整仓。
- Root cause: `install_skill` 与 `install_skills_batch` 各自选文件夹；scan 成功时跳过 `resolve_install_skills`；scan 失败再 `git clone` 到 hub。那是第六条路径。
- Fix: 只留 `install_from_source`：resolve → discover → 一个 chooser → hub link。Agent deploy / scope 仍在调用方。见 [D-047](decisions.md#d-047技能安装是-vercel-skills-五步管线harness-文件夹是-identity-别名)。
- Self-check: `cargo test -p skillstar-skills --locked --lib skill_install -- --nocapture`。

## 2026-08-31 - 已删除的 git ref 把无关技能的安装/轮播部署打成「无法安装该技能」

- Symptom: Library 点 banner-design 的 Antigravity 轮播，toast「无法安装该技能」。日志混入 `update_checker: prefetch git fetch failed — will preserve existing update state`，`fatal: couldn't find remote ref cursor/harness-install-units-dc3e`。该 ref 来自另一条 lock 记录（`rust` 钉在已合并并删除的 PR 分支）；cache 目录仍叫 `--ref--cursor--harness-install-units-dc3e`，工作区 HEAD 已是 `main`。
- Root cause: lock / `skillstar.ref` 把已删除分支当成硬 fetch 目标。prefetch 虽写明保留徽标，但 (1) cache-local 只按「URL 无 ref」查 `{source}` 目录，找不到 `{source}--ref--{gone}`，已在 hub 的轮播会再 fetch；(2) `fetch_and_reset_ref` / `fetch_tracked_ref` 对 `couldn't find remote ref` fail-closed，错误被映射成通用安装 toast，即使点击的是另一张卡。cache 里已有 `SKILL.md` 时不该 fail-closed。
- Fix: cache-local 通过 hub symlink 和 `--ref--*` 变体找到现有 checkout，已在 hub 的轮播不再 fetch。fetch 遇到 missing-ref 且 cache 仍有 `SKILL.md` 时使用现有文件，把 lock `git_ref` 和 `skillstar.ref` 改指仓库默认分支（或省略），不得继续钉死分支。无 `SKILL.md`、也无 cache 仍 fail-closed。prefetch 失败不得让另一技能的 install 返回错误。
- Self-check: `cargo test -p skillstar-skills --locked --lib skill_install::harness_retarget_tests -- --nocapture`；`cargo test -p skillstar-git --locked --lib ops::tests::missing_remote_ref_is_detected_through_anyhow_context -- --exact`。手工：lock 钉已删分支、cache HEAD 在 main，点另一技能的 harness 轮播必须安装成功，且 rust 的 lock `git_ref` 不再是死分支。

## 2026-08-31 - Windows dangling junction 对不上 cache，SourceMissing 被吃掉

- Symptom: Windows CI 只红 `source_dropped::a_dropped_skill_whose_content_is_already_gone_can_only_be_removed`：`blocked` 是 `[]`，期望 `[("alpha", SourceMissing)]`。Linux/macOS 绿。harness / CRLF / deny 已过。
- Root cause: 两件叠加，都不是「放宽 SourceMissing」。① `reset --hard origin/HEAD` 在 path-remote 的 Windows clone 上不一定落到 drop commit；junction 指着 `skills/alpha` 时 reset 也可能删不掉该目录，hub 仍是可读副本，`exists()` 为真就不该报 SourceMissing。② 即便链已 dangling，`repo_root_of` / `is_inside` 对缺尾巴的目标 `canonicalize` 失败后拿生路径去比；Windows temp 常是 `RUNNER~1`，cache 目录是 `runneradmin`，alpha 被踢出共享 checkout。
- Fix: 夹具钉 drop commit oid，断言 cache tree 已无 `skills/alpha`，再把 hub 建成**不可读的 managed link**（先链后删 target）。产品侧 `canonicalize_existing_prefix` 让 dangling 链仍属于同一 checkout。不要改 SourceMissing 语义，也不要在 Windows 上 skip。
- Files: `crates/skillstar-skills/src/skill_update/tests/source_dropped.rs`、`crates/skillstar-core/src/infra/fs_ops.rs`、`crates/skillstar-skills/src/repo_link.rs`。
- Self-check: `cargo test -p skillstar-skills --locked --lib skill_update::tests::source_dropped -- --nocapture`。

## 2026-08-31 - Windows gitconfig `insteadOf` 吃掉反斜杠；checkout 变成 CRLF

- Symptom: Windows CI 在 store-v4 变绿之后，`skillstar-skills` harness / private-facade 报 `fatal: 'C:UsersRUNNER~1…' does not appear to be a git repository`；部分 update 测试左边是 `echo remote-v2\r\n`。macOS/Linux 绿。此前被 fail-fast 挡住。
- Root cause: `[url "file://C:\Users\…"]` 写进 `GIT_CONFIG_GLOBAL` 时，gitconfig 把 `\` 当转义，路径变成 `C:Users…`。Windows runner 默认 `core.autocrlf=true`，测试夹具未钉 LF，checkout 带 CR。`Path::join(".claude/skills/…")` 的 `display()` 仍留着 `/`，产品返回的是 `\`。
- Fix: `skillstar_git::ops::local_file_url` 统一成 `file:///C:/…`。测试 `git init` 后钉 `core.autocrlf=false` / `core.eol=lf` / `* -text`。路径断言比 `Path::components`。不要为了绿而改产品正文或接受 CRLF digest。
- Files: `crates/skillstar-git/src/ops.rs`、`skill_install_harness_tests.rs`、`skill_update/tests.rs`、`deployment/tests.rs`、`.github/workflows/windows-ci.yml`。
- Self-check: `cargo test -p skillstar-git --locked --lib local_file_url`；`cargo test -p skillstar-skills --locked --lib harness_retarget_tests skill_update::tests`。Windows 上 insteadOf 必须 clone 到真实 temp 路径，`scripts/run.sh` 必须仍是 LF。

## 2026-08-31 - cargo-deny 因 RUSTSEC-2026-0258 拦 h2

- Symptom: Linux `cargo-deny` `advisories FAILED`：`h2 unbounded empty DATA frames`，ID `RUSTSEC-2026-0258`。Rust 测试已过。
- Root cause: lockfile 钉着 h2 0.4.13。advisory-db 在 2026-08-18 发布补丁线 `>=0.4.16`。deny.toml 要求修依赖，不要把活漏洞写进 ignore。
- Fix: `cargo update -p h2 --precise 0.4.16`。
- Files: `Cargo.lock`。
- Self-check: `cargo deny check advisories --config src-tauri/deny.toml` 必须还是 ok（忽略项不得增加这条）。

## 2026-08-31 - Windows 目录只读挡不住 store-v4 备份，迁移照样写盘

- Symptom: Windows CI 在频道 hash / proxy 变绿之后，`providers::tests::store_v4::migration_aborts_when_the_backup_cannot_be_written` 期望 `BackupFailed`，实际 `Ok(LoadedStore { version: 4 })`。Linux/macOS 同测试绿。此前被 fail-fast 挡住。
- Root cause: 夹具用 unix-only `set_mode(0o500)` 把 scratch 目录设成只读。Windows 上目录 readonly 位不阻止在该目录里 `fs::copy` 出新文件。`take_migration_backups` 又用 `exists()` 判断永久快照：若把备份路径占成目录，会当成「已有快照」跳过 copy，照样迁移。
- Fix: 永久备份只跳过**已存在的文件**（`is_file()`）。测试把 `model_providers.v3.json` 建成目录，让 `fs::copy` 在每个 OS 上都失败。不要靠目录 readonly，也不要放宽 `BackupFailed`。
- Files: `crates/skillstar-models/src/providers/store_v4.rs`、`crates/skillstar-models/src/providers/tests/store_v4.rs`、`.github/workflows/windows-ci.yml`。
- Self-check: `cargo test -p skillstar-models --locked --lib providers::tests::store_v4`；Windows CI 这条必须仍是 `BackupFailed`，且 v3 原文完整。

## 2026-08-31 - Linux CI 在 skillstar-git 被 `The operation was canceled` 砍掉

- Symptom: `test-linux` 前端 lint/tests 全绿，Rust workspace 跑到 `skillstar-git` 后整步 `cancelled`，annotation 是 `The operation was canceled.` 作业大约 7 分钟。macOS 同 suite 绿。`test-linux` **没有** `timeout-minutes`。main 上已经这样，不是 harness 测试把 cap 撑爆。
- Root cause: `terminate_child_tree` 对 unix 发 `kill -TERM -$pid`（进程组）。取消测试的 fake-git 若没能 `process_group(0)` 隔离，这就是 GitHub Actions step 的进程组；runner 收到 SIGTERM 就记成 operation canceled。看起来像 timeout 或 `cancel-in-progress`。
- Fix: 仅当 `ps` 确认该 child 是进程组组长（`process_group(0)` 生效）时才 `kill -TERM -- -$pid`；否则只杀该 pid。`child.wait()` 改为 2s 上限再 SIGKILL，避免 kill 失败后挂在 `sleep 30` 上。不要为了这个去加/加大并不存在的 job timeout，也不要删掉取消覆盖。这是 main 上已有的 Linux 红，本 PR 必须让 `cargo test --workspace` 跑完。
- Files: `crates/skillstar-git/src/transport.rs`、`.github/workflows/ci.yml`。
- Self-check: `cargo test -p skillstar-git --locked --lib`；Linux CI 必须跑完 workspace Rust tests，不能再停在 skillstar-git 的 cancel annotation。

## 2026-08-31 - Windows `get_envs()` 没有独立的 `http_proxy=None`

- Symptom: Windows CI 在频道 hash 变绿之后，`configured_proxy_is_operation_local_and_its_password_is_redacted` 报 `left: None` / `right: Some(None)`。Linux/macOS 同测试绿。此前被 hash 失败挡住，workspace fail-fast 跑不到。
- Root cause: Windows 环境变量名大小写不敏感。`env_remove("http_proxy")` 再加上 `HTTP_PROXY=…` 之后，`Command::get_envs()` 不会再列出一条独立的 `http_proxy=None`。Unix 上大小写是不同的键。Runner 上的 `HTTP_PROXY` 会让这个看起来像 flake。
- Fix: 测试先把进程级 proxy 键换成 `inherited-canary` 并在 Drop 里恢复。Unix 仍断言 Command 覆盖是 `Some(None)`。Windows 复制同一组覆盖到 `cmd /C set`（Unix 用 `env`）证明子进程看不到 canary、只看到 SkillStar proxy。密码 redaction 断言不改。
- Files: `crates/skillstar-git/src/transport_tests.rs`。
- Self-check: `cargo test -p skillstar-git --locked --lib configured_proxy_is_operation_local`。

## 2026-08-31 - Windows `autocrlf` 把频道 content-hash 改成第二套 digest

- Symptom: Windows CI 的 `skillstar-channels` 在 `exact_commit_snapshot_uses_the_shared_known_content_hash` / `exact_commit_snapshot_disables_export_ignore_and_export_subst` / `production_installer_verifies_the_exact_release_checkout` 失败；左边是 `sha256:30263545…` 一类 CRLF digest，右边是 Linux 硬编码的 `sha256:6e8b30c2…`。macOS/Linux 全绿。同一组测试在 `main` 上就已经红，不是 harness 安装引入的算法变化。
- Root cause: GitHub Windows runner 默认 `core.autocrlf=true`。`git archive` 仍会按 commit / 主机的 text/eol 属性改写 tracked 字节；测试里的 `git clone` 也会把 LF fixture checkout 成 CRLF。`snapshot_path` 哈希的是落盘字节，于是发布快照和校验 checkout 对不上 Linux 的精确 commit hash。
- Fix: 发布归档在 `.git/info/attributes` 同时关掉 `export-ignore` / `export-subst` / `text`，并给 `git archive` 钉 `core.autocrlf=false` + `core.eol=lf`。频道 hash fixture 的 `git init`/`clone` 同样钉 LF，并用 `* -text`（或让 info 层压过 `* text=auto`）。不要接受 Windows 算出来的第二套 digest。
- Files: `crates/skillstar-channels/src/shared_channels/release_scanner.rs`、`subscription_installer_tests.rs`、`.github/workflows/windows-ci.yml`。
- Self-check: `cargo test -p skillstar-channels --locked --lib exact_commit_snapshot production_installer`；Windows CI 这三条必须绿，且仍断言 LF 算法哈希 `sha256:6e8b30c29c269c5375c2149f4834f8f6d289e5842b6d75f0f912749605a537f7`。

## 2026-08-31 - 第二个 harness 复用第一条 lock，把 A 的正文部署给 B

- Symptom: `skillstar install … --agent cursor` 之后再 `install … --agent deepseek` 打印 `Reusing existing hub install(s)`，把 `~/.dsh/skills/rust` 链到 **Cursor** 的 hub 路径；lock `source_folder` 仍是 `.cursor/skills/rust`。clone 里其实有 `.dsh/skills/rust`。卡片轮播点第二个图标同样走这条。Hub 已是 `.agents/skills/impeccable` 时再 `--agent cursor` 也不改指向 `.cursor/skills/impeccable`。对 rust-skills 两份拷贝相同，对 impeccable 式改写过的 `SKILL.md` 会装错 harness。
- Root cause: 同名一条 lock 把「仓库已在 hub」当成「可以复用」。`install_skills_batch` / `try_install_from_repo_cache` 只比 git URL，不比 `source_folder` 是否已经是本次请求的 `.<harness>/` 文件夹。CLI 的 `install_or_reuse` 因此走 Reuse，随后 `batch_deploy` 把**当前** hub（另一份 harness）链到新 Agent。轮播未链接图标也曾只 `toggle_skill_for_agent`，同样部署当前 hub。
- Fix: 复用仅当现有 `source_folder` 已是本次解析到的文件夹。否则从同一 clone 改指向（不二次 clone），改之前 `pin_existing_global_links_to_current_source` 把**其他** Agent 钉到当前 payload（跳过正在改指向的 Agent）。`batch_deploy` 对目标 Agent 上指向另一份 harness / 旧 hub 的 symlink 做 link-first 替换，不得把「路径已存在」当成成功。缺少该 harness 时按 D-046 回退，不得 fail-closed。轮播未链接图标走 `install_skill(url, name, agentId)`。
- Files: `crates/skillstar-skills/src/skill_install.rs`、`crates/skillstar-skills/src/deployment/mod.rs`、`src/features/my-skills/components/SkillCard.tsx`、`docs/features/skills/README.md`。
- Self-check:
  - `cargo test -p skillstar-skills --locked stale_dsh_link_is_rewritten_to_requested_harness batch_deploy_rewrites_a_stale_link_and_leaves_other_agents_pinned`
  - 装完 Cursor 再装 DeepSeek：clone 里有 `.dsh` 时 `~/.dsh/skills/<id>` 必须解析到 `.dsh/skills/<id>`，不能是 `.cursor/skills/<id>`；已链的 Cursor 仍是 Cursor 正文。即使 `~/.dsh/skills/<id>` 事先错误地指向 cursor 文件夹，部署也必须改写，CLI 不得报 `0 new deployment(s)`。
  - Hub 已是 `.agents/skills/<id>` 时 `--agent cursor` 必须把 lock/`source_folder` 改成 `.cursor/skills/<id>`，不得静默 Reuse。

## 2026-08-31 - 已装卡点轮播又 fetch，缺 `.dsh` 还 fail-closed

- Symptom: Library 里 rust-skills / impeccable 已在 hub，点未链接的 DeepSeek 图标要等一整次 Git clone/fetch；impeccable（包内没有 `.dsh`）随后报 `This pack has no '.dsh' skill folder`，`~/.dsh/skills/impeccable` 不会出现。
- Root cause: `try_install_from_repo_cache` / CLI `fetch_repo_scanned` 一律走 `clone_or_fetch`，cache 有 `.git` 也会 `git fetch --depth 1` + reset。`resolve_install_skills` 在缺请求 harness 时 fail-closed，不回退 catalog / 现有 hub / 另一份副本。
- Fix: 已有 repo-cache 时只扫描本地 checkout（`cached_repo_dir_if_present`），不 fetch；`source_folder` 没变就不改 lock。缺 harness 时回退 `skills/<name>/` 或 `source/skills/` → 现有 hub `source_folder` → 同 identity 的另一嵌套副本，再部署到被点 Agent。只有没有嵌套 `SKILL.md` 才失败。cache 被删仍 fetch。
- Files: `crates/skillstar-skills/src/{skill_install.rs,discovery.rs,repo_scanner/cache.rs}`、`crates/skillstar-app/src/cli/install.rs`、`docs/features/skills/README.md`。
- Self-check:
  - `cargo test -p skillstar-skills --locked installed_rust_skills_deepseek_retargets_from_cache_without_clone installed_impeccable_deepseek_falls_back_to_a_skill_folder missing_git_cache_still_fetches_for_harness_install`
  - 已装 rust-skills：掐断 remote 后再 `--agent deepseek` 必须成功，dsh 链到 `.dsh/skills/rust`，cursor 不变。
  - 已装 impeccable：`--agent deepseek` 必须成功，`~/.dsh/skills/impeccable` 是含 `SKILL.md` 的技能目录，不是整仓。
  - 删掉 `repos` cache 后再装必须重新 fetch，不能静默 no-op。

## 2026-08-31 - TS 孤儿门禁在 Windows CI 上打印 ✓/✗ 触发 UnicodeEncodeError，报 0 违规却退出 1

- Symptom: Windows CI 只有 `checkTsOrphanModules.test.ts > passes on the repository as it stands` 失败，且断言 `output 包含 "0 new orphan module(s)"` 通过、`status` 却是 1；日志散落 `UnicodeEncodeError: 'charmap' codec can't encode character '\u2713'/'\u2717'`。macOS/Linux 本地与 CI 全绿。
- Root cause: 门禁的 bash 包装内嵌 `python3` heredoc，收尾 print `✓`/`✗`。GitHub Windows runner 上子进程 stdout 按 ANSI 代码页（cp1252）编码，非 ASCII print 直接抛异常；纯 ASCII 的 summary 行先打印成功，所以「0 个孤儿」的断言过了，Python 却以异常退出码 1 结束。stdout 是管道而非控制台时 Python 不用 UTF-8，这是本地终端永远复现不了的原因。
- Fix: 五个内嵌 python3 的门禁脚本（check_ts_orphan_modules / check_no_orphan_modules / check_dep_graph_doc / check_workspace_deps / check_command_boundaries）统一在调用行加 `PYTHONIOENCODING=utf-8`，让 Python stdio 与平台代码页解耦。
- Self-check: `for f in $(grep -l python3 scripts/internal/*.sh); do grep -q PYTHONIOENCODING "$f" || echo "missing: $f"; done` 必须无输出；新增内嵌 python3 的门禁若 print 非 ASCII 字符，调用行必须带 `PYTHONIOENCODING=utf-8`。

## 2026-08-31 - Windows 的持锁文件不能由第二个句柄读取来验证 holder 内容

- Symptom: Windows CI 的 `usage_switch::custody_tests::grok_shares_the_cli_lock_file_and_writes_its_holder_line` 在 `fs::read_to_string(~/.grok/auth.json.lock)` 报 `Os { code: 33, message: The process cannot access the file because another process has locked a portion of the file }`；macOS/Linux 通过。
- Root cause: `Custody::lock()` 取得 Grok 官方 lock 后，在返回 `CustodyLease` 前写入并 `sync_data()` PID holder 行。Unix 文件锁通常是 advisory，旧测试可以在 lease 持有时用第二个 handle 读取；Windows 正确执行排他/字节范围锁，第二次打开读取被拒绝。测试误把 Unix 的偶然可读性当成跨平台契约。
- Fix: 在内层作用域取得并释放 lease，随后读取同一官方 lock 文件的持久内容；仍断言内容以当前 PID 开头且 `auth.json.skillstar.lock` 未出现。不要加 retry、`cfg(windows)` skip 或修改生产锁语义。若要测试锁竞争，另建明确的互斥不变量测试。
- Self-check: `cargo test -p skillstar-app usage_switch::custody_tests::grok_shares_the_cli_lock_file_and_writes_its_holder_line --locked -- --exact`，并以 Windows CI 验证真实文件锁语义。

## 2026-08-31 - Actions runner shutdown 不等于 Rust 测试断言失败

- Symptom: Linux 的 Rust tests job 显示失败，但 job log 没有 `FAILED`、panic 或 `test result: FAILED`；末尾直接是 `The runner has received a shutdown signal` 和 `The operation was canceled`。
- Root cause: 这是 GitHub Actions runner/工作流层的取消信号，不是测试返回的失败。对有 `cancel-in-progress` 的 workflow，重跑旧 run 也可能与新 push run 争用同一 concurrency group，进一步制造这种假红。
- Fix: 不要据此修改测试或用无关 retry 掩盖问题；先抓取失败步骤原始日志区分 runner cancel 与断言失败。需要重新取证时，让下一次相关 push 的新 run 自然执行，不要在它运行时重跑旧 run。
- Self-check: `gh run view <run-id> --job <job-id> --log-failed` 的末尾必须先确认是否存在测试失败标记；只有实际断言/编译错误才进入代码修复。

## 2026-08-31 - process-backed Vitest gate 的过紧 wall-clock timeout 会把满载调度延迟当成失败

- Symptom: pre-push 的完整 `bun run test` 偶发仅在 `checkTsOrphanModules.test.ts > passes on the repository as it stands` 处失败，断言 `run.timedOut` 收到 `true`；同一脚本单独运行约 0.2–0.6 秒且输出 `0 new orphan module(s)`。
- Root cause: 测试通过 `execFileSync` 执行真实 bash/Python gate，并把 child 的 **wall-clock** timeout 固定为 15 秒。完整 Vitest pool 同时 transform/运行大量文件、或宿主还有编译任务时，OS 可能让这个同步 child 在实际开始前就耗尽 15 秒；它把调度竞争误判为脚本挂死。外层 test 的 20 秒 timeout 不能修复，因为 child 已先被 `execFileSync` 杀掉。
- Fix: 仍保留有限 hang guard，但将 child 上限设为 60 秒，并给 Vitest 额外 10 秒观察退出与 cleanup。不要因为 gate 独立运行很快就把该上限缩回 15 秒；测试的目标是完整 suite 下的可靠性，不是 microbenchmark。
- Self-check: `bun run test` 必须通过完整 Vitest suite；单独测量 `/usr/bin/time -p bash scripts/internal/check_ts_orphan_modules.sh >/dev/null` 只用于发现真实算法退化，不能替代全套验证。

## 2026-08-31 - 取消测试不能用 worker 启动前的固定时间窗判断 child 已就绪

- Symptom: 全 workspace 并发测试时，`transport_tests::fake_transport_sees_credential_only_while_running_and_is_killed_on_cancel` 偶发整整等待 10 秒后报 marker 未出现，但 worker 返回的是 `Cancelled`。单独运行通常全绿，使它看起来像 credential 配置错误。
- Root cause: 旧测试把 `execute_remote_command` 放到 worker，并由主线程从 spawn worker 的瞬间开始 marker deadline。繁忙时 worker 可能尚未被调度、或尚未走到 `command.spawn()`，主线程就置 cancel；transport 的 preflight 会直接返回 cancelled，或轮询路径会在 shell 写 marker 前杀 child。加长 timeout 只是扩大竞态窗口。
- Fix: 主测试线程同步执行 transport；watcher 只在 marker（child 确认收到了 operation-scoped credential）出现后取消。测试私有 `AtomicBool` 在 command 返回后通知 watcher 无 marker 地退出失败，避免 pre-spawn cancel，同时仍覆盖 marker 之后 `sleep 30` 子进程的 kill/reap。
- Self-check: `for n in 1 2 3 4 5; do cargo test -p skillstar-git transport_tests::fake_transport_sees_credential_only_while_running_and_is_killed_on_cancel --locked -- --exact || exit; done` 必须每次运行 1 个测试且全绿。

## 2026-08-31 - tiny_http responder 的空闲超时不是 mock server 生命周期信号

- Symptom: `cloud_code::tests::supported_quota_summary_does_not_call_model_fallback` 与 `model_fallback_runs_only_for_unsupported_summary` 偶发把对 `127.0.0.1` 的 `retrieveUserQuotaSummary` 请求报为 transient send failure；单独运行时常常消失。
- Root cause: responder 用 `recv_timeout(250ms)`，并把 `Ok(None)` 当成服务完成而 drop `tiny_http::Server`。Tokio 调度或满载 CI 可以在首次请求（或 404 后的 fallback 请求）前停顿超过 250ms，端口已经被关闭；生产实现正确将这种连接失败映射为 transient，测试却把 fixture 生命周期错误伪装成业务失败。
- Fix: 主测试持有 `Arc<Server>`，responder 用阻塞 `recv()` 服务请求；先保存 fetch 结果，再调用 `server.unblock()` 唤醒 responder、join，最后再 unwrap/assert。失败路径也会 shutdown/reap，不依赖调度时间窗。
- Self-check: `for n in 1 2 3 4 5; do cargo test -p skillstar-usage cloud_code::tests:: --locked || exit; done` 必须每次通过全部 cloud_code 测试。

## 2026-08-27 - clippy 棘轮在热缓存下读到 0，照它的提示锁定基线会让冷构建 CI 挂掉

- Symptom: 本地刚跑过 `cargo clippy` 后立即执行 `scripts/internal/check_clippy_ratchet.sh`，输出 `summary: 0 clippy diagnostics (baseline: 1)` 并主动提示 `note: count dropped below baseline — lower scripts/internal/clippy_baseline.txt to lock in the improvement`。照做把基线改成 0，CI 冷构建时真实计数仍是 1，`1 > 0` 直接失败。
- Root cause: 脚本靠 `grep -c '^warning:'` 数 `cargo clippy` 的 stdout，而 cargo 对**未变更且已缓存**的 crate 不会重新发出诊断。热缓存下 `skillstar-usage` 不重编译，那条 `needless_lifetimes`（在冻结的 `fetchers/oauth/cursor.rs`，按项目规则不得修改）就不出现在输出里，计数虚假归零。脚本本身不区分"真的修好了"和"这次没编译它"。
- Fix: 采信该脚本的计数前，先强制目标 crate 重编译（`touch` 相关源文件，或 `cargo clean -p <crate>`）再跑。基线维持 1；那一条属于冻结文件，不可清零。
- Self-check: `touch crates/skillstar-usage/src/fetchers/oauth/cursor.rs && bash scripts/internal/check_clippy_ratchet.sh` 必须报 `1 clippy diagnostics`。同一陷阱适用于任何以编译器输出为计数源的门禁。

## 2026-08-27 - 更新检查在普通目录里让 git 向上逃逸到用户的其他仓库

- Symptom: 无明显症状,这正是危险处。hub 里的普通目录技能(bundle 导入、pack 安装、手工放入)每次巡检都会在自己目录里跑 `git fetch --depth 1`;当任何祖先目录是真仓库(`$HOME` 的 dotfiles 仓库、被版本管理的 `~/.skillstar`),git 解析到的是**那个**仓库,于是 SkillStar 定期对用户无关的仓库做浅取,并拿它的 HEAD 与 FETCH_HEAD 算出技能的更新徽章。
- Root cause: 所有 git 调用只设 `current_dir`,不设 `-C` / `--git-dir` / `GIT_CEILING_DIRECTORIES`,所以 git 会向上走查找 `.git`。`ensure_worktree_checked_out_in_session` 自带 `if !repo_path.join(".git").exists() { return Ok(false) }` 守卫,但紧挨着它的 `check_update_in_session` 没有 —— 同一对调用里一个 fail-closed 一个不设防。
- Fix: 守卫放进共享的 `check_update_in_session`(`crates/skillstar-git/src/ops.rs`),不是放进某一个调用方 —— patrol 与前台刷新是两个调用方,只修一个会漏。返回 `Err` 而非 `Ok(false)`:两个调用方都刻意把失败映射为"未知"以免用假的 `false` 覆盖真实徽章。
- Self-check: `cargo test -p skillstar-git --locked check_update_refuses_a_directory_without_its_own_git`。凡是靠 cwd 定位仓库的 git 包装函数,都要问一句"目标不是仓库时会发生什么"。

## 2026-08-27 - 删 UI 不删后端,留下一串永远走不到的代码

- Symptom: 一批功能"看起来实现了"但用户永远触发不到:Codex 写 `~/.zshrc` 的 385 行后端 + Tauri 命令 + 前端 IPC 声明齐全,却没有任何按钮;`OAuthStartInfo` 的 `user_code`/`verification_uri` 恒为 `None`,前端却有整块设备码 UI 等着渲染;skill-pack 的 `list`/`remove`/`doctor` CLI 操作一个没有写入方的 store。
- Root cause: 提交 `8e53552 chore(models): remove dead code and promote prototype to production paths` 只删了前端调用点,后端命令、域实现、IPC 声明、文档承诺原样留下。死代码不会自己报错:内部 workspace crate 里的 `pub` 项不触发 dead_code lint,Tauri 命令注册了就"被使用",`#![allow(dead_code)]` 还会主动压掉信号。文档因此长期描述着不存在的行为。
- Fix: 按"入口 → 域实现 → 命令 → IPC 声明 → devMock → locale key → 文档"整条链一次性删净,并摘掉掩盖信号的 `#![allow(dead_code)]`。
- Self-check: 删除任何 UI 调用点后,反向追一遍它独占的后端链路。判断"是废弃还是没接线"用 `git log -S '<命令名>' -- src/`:前端历史上出现过则是被删的功能(继续删干净),从未出现过则是没接线的脚手架(同样删,但要在功能文档里写清缺口)。

## 2026-08-27 - tool-sync 把解析失败的用户配置静默重建成仅含托管块的骨架

- Symptom: 用户的 `~/.codex/config.toml`（或 OpenCode/Pi/OMP/Claude 的 JSON/YAML 配置）里有一个语法错误后,下一次 provider 同步"成功",但文件里只剩 SkillStar 托管块——用户自己的 MCP servers、profiles、OAuth token 全部消失。滚动备份只保留 5 份,后续自动 resync 会把最后一份好备份也轮换掉。
- Root cause: 同步写入路径在读现有文件时用 `unwrap_or_default()` / `unwrap_or_else(|_| init_root())` 把解析失败当成"文件不存在",随后整文件重写。这正是 store_v4 模块文档点名要终结的 v3 缺陷,且同文件的 unsync 路径早已 fail-closed(`with_context(...)?`),形成双标。
- Fix: 所有 sync 写入方统一走 `backup_merge::read_existing_config`:文件缺失或空白 → 从头初始化;存在但解析失败 → 硬错误 `Failed to parse … — fix or remove it before syncing`。同时全部写入点从裸 `fs::write` 换成 `skillstar_core::infra::fs_ops::atomic_write`,崩溃不再截断配置。
- Self-check: 往沙箱 `~/.codex/config.toml` 写一行非法 TOML 后触发同步,必须报错且文件字节不变;`cargo test -p skillstar-models --locked tool_sync`。

## 2026-08-27 - 非 GitHub 仓库的上游新增技能检测永远静默跳过

- Symptom: 从 GitLab/SSH 源安装的技能一切正常,但上游新增技能的 ghost 检测从不出现;无日志、无报错。
- Root cause: `detect_new_skills_in_cached_repos` 自己用 `strip_prefix("https://github.com/")` 推导 cache 目录 key,而安装路径用 `Source::parse(url).short`。两套推导只在 GitHub https URL 上碰巧一致;GitLab/SSH 源算出的目录名对不上,`.git` 存在性检查失败后 `continue` 静默丢弃。同一份 key 出现两套推导逻辑时,不一致就是这种"只对主流路径生效"的静默缺陷。
- Fix: detect 侧改用与安装完全相同的 `Source::parse(&git_url).map(|s| s.short)`,失败回退原始 URL。
- Self-check: `cargo test -p skillstar-skills --locked repo_scanner`;手工:GitLab 源安装后上游加技能,fetch 后 ghost 卡必须出现。

## 2026-08-21 - 上游新增的 Skill 永远检测不到，刷新按钮也无效

- Symptom: `mattpocock/skills` 新增 `skills/in-progress/implement-spec` 一小时后，My Skills 既没有 ghost 卡也没有推送；点右上角刷新毫无变化。本地 cache 的 `HEAD` 与 `origin/HEAD` 都停在前一天的 commit。
- Root cause: ghost 检测扫描的是 repo cache 的**工作树**，而工作树只在安装/更新/扫描时 `reset --hard origin/HEAD`。两条更新检查路径都不移动它：UI 的 `refresh_skill_updates` 走 GitHub Trees API（完全不碰 git，连 `origin/HEAD` 都不前进），patrol 只 `git fetch`（动 `origin/main`，不动工作树）。"上游只新增了一个目录"这种变化因此在结构上不可见，直到用户碰巧更新了同仓库的别的 Skill。另外工具栏刷新只重取 `list_skills` 与 `refresh_skill_updates`，从不重取 `check_new_repo_skills`。
- Fix: 检测改为本地 `HEAD` tree 与 tracked ref tree 的差集，manifest 直接从 Git 对象经 session 读取（见 skills README「Patrol 与页面职责」）；API 快路径只在远端 commit 仍等于本地 tracked ref 时替代 fetch，否则回退 `git fetch` 让 ref 跟上；前端在每次更新检查完成后（按 `dataUpdatedAt`，不按 data 身份）重取 ghost。
- Self-check: `cargo test -p skillstar-skills --locked upstream_additions_surface_after_a_fetch_without_touching_the_checkout`。手工：上游新增一个 Skill 后点刷新，cache 里 `git rev-parse origin/HEAD` 应前进到上游 tip 而 `HEAD` 不动，ghost 卡出现且带描述。

## 2026-08-20 - 仓库根技能更新后徽标立刻回来

- Symptom: 像 `ip-as-logo` 这种 SKILL.md 在仓库根的技能，点「更新」成功后徽标马上又亮；再点一次还是一样，看起来像一直在更新。
- Root cause: GitHub Trees 快路径把响应顶层 `sha` 当成根目录 tree SHA，再和本地 `HEAD^{tree}` 比较。`GET /repos/{owner}/{repo}/git/trees/{branch}?recursive=1` 在用 branch/commit 查询时，GitHub 把 **commit SHA** 放进 `sha`，真正的 tree SHA 在 commit 对象里（例如 `ip-as-logo-skill@main` 的 `sha` 是 `2cb23157…`，tree 是 `819c89fe…`）。根技能的这两个值永远不相等，所以已经在远端 tip 也会被判成有更新。子目录技能不受影响，它们用的是 `tree[]` 里的真实 tree SHA。
- Fix: 根技能在 API 快路径上同时接受「本地 HEAD == 远程 sha」和「本地 HEAD^{tree} == 远程 sha」；子目录比较不变。
- Self-check: `cargo test -p skillstar-skills --locked github_trees_api_commit_ish_sha_does_not_badge_an_up_to_date_root_skill`。造一个根技能，把 API `folders[""]` 设成该 checkout 的 commit SHA（刻意不等于 tree SHA）时必须报无更新；换成另一个 SHA 时仍报有更新。

## 2026-08-17 - Antigravity 摘要额度被模型目录的全量剩余值遮蔽

- Symptom: SkillStar 桌面 App 中 Antigravity 卡片显示 `0%` / `剩余 100%`，所有模型行都是 `0 / 100`，但 Antigravity 官方可见的 5h/weekly 使用窗口已有消耗；刷新没有报错，只是继续展示模型目录的全量剩余值。
- Root cause: `fetchAvailableModels` 是模型可用性目录，不一定反映用户可见的计费/限流窗口；部分账号会为模型目录返回 `remainingFraction = 1`。旧逻辑只在模型窗口为空时才请求 `retrieveUserQuotaSummary`，因此一组“看起来完整、实际没有用量”的模型窗口会遮蔽真正的摘要额度；同时汇总窗口使用平均值，不能代表最紧张的限制。
- Fix: 每个 Cloud Code endpoint 都优先读取 `retrieveUserQuotaSummary`，并兼容直接 `groups` 与 `response.groups` 两种响应形状；只有摘要为空时才保留模型窗口作为 fallback。汇总窗口按最高已消耗百分比展示，且 `used` / `percent` 语义统一；上游标签中的 `Remaining` 后缀会被去掉，避免与卡片的已消耗百分比混淆。
- Self-check: 在桌面 App 的 Usage → Antigravity 点“刷新 Antigravity”，卡片应优先显示 Gemini / Claude + GPT 的 5h/weekly 窗口；只有摘要接口无窗口时才显示模型目录明细。测试覆盖摘要响应形状、分组标签、最紧张窗口选择和 `used` / `percent` 一致性。

## 2026-08-17 - Antigravity 额度请求丢失项目或被解析器静默过滤

- Symptom: Antigravity 的 plan/credits 能刷新，但模型额度为空、显示不全，或接口失败后卡片像是“成功刷新”却没有任何解释。
- Root cause: `loadCodeAssist` 的 `cloudaicompanionProject` 可能是 `{ "id": "projects/..." }`，旧解析只接受字符串，后续 `fetchAvailableModels` 因缺少 `project` 得到错误/不完整结果；额度解析还只接受固定模型 ID，且 fetcher 用 `unwrap_or_default()` 吞掉了额度请求错误。
- Fix: 兼容字符串与对象项目字段；Cloud Code 的 `loadCodeAssist` 按 daily、sandbox、production 回退并发送 Antigravity 完整 metadata；模型额度保留已知分组，同时显示新增的 Gemini/Claude/GPT/Image 模型；模型接口为空时 best-effort 读取 `retrieveUserQuotaSummary`；额度请求失败保留 plan/credits 并写入可见错误，401 仍触发重新授权。
- Files: `crates/skillstar-usage/src/cloud_code.rs`、`crates/skillstar-usage/src/fetchers/oauth/antigravity.rs`、`docs/features/usage/README.md`。
- Self-check: `cargo test -p skillstar-usage cloud_code::tests --locked`、`cargo test -p skillstar-usage fetchers::oauth::antigravity::tests --locked`；重点覆盖对象项目 ID、新模型 ID、summary bucket、额度请求错误可见性。

## 2026-08-17 - Cursor 切换账号只改 active pin，没有改 IDE 登录态

- Symptom: Usage 页面有多张 Cursor OAuth 卡，点“切为当前账号”后卡片 pin 变化，但 Cursor 仍使用原账号；此前 Cursor 卡甚至没有真实的 IDE 同步适配器。
- Root cause: `usage_switch` 只把 Cursor 当作无本地凭证的 catalog；Cursor 实际把 OAuth token 分散存于 `state.vscdb` 的 `cursorAuth/accessToken`、`cursorAuth/refreshToken`、`cursorAuth/cachedEmail` 和镜像 key。
- Fix: 新增 Cursor IDE adapter，在 catalog 锁内事务写入并回读验证真实 state.vscdb，验证成功后才更新 active pin；对账读取 IDE 当前账号，刷新前采纳本地轮换，刷新后投影新 token；本地导入复用同一读取路径。
- Files: `crates/skillstar-app/src/usage_switch/cursor.rs`、`crates/skillstar-usage/src/{tool_paths.rs,vscdb.rs,local_import.rs}`、`crates/skillstar-app/src/usage_switch/custody_tests.rs`。
- Self-check: 两张 Cursor 订阅连续切换后，`cursorAuth/accessToken` / `cursorAuth/refreshToken` 必须分别等于目标卡凭证；删除或损坏 state.vscdb 时不得只更新 active pin；`reconcile_cli_account("cursor")` 必须返回 `LinkedTo`、`Diverged` 或 `Missing` 的真实状态。

## 2026-08-17 - Antigravity OAuth 因缺少外部 client 配置而无法登录

- Symptom: Antigravity 登录直接失败并提示设置 `SKILLSTAR_ANTIGRAVITY_CLIENT_ID` / `SKILLSTAR_ANTIGRAVITY_CLIENT_SECRET` 或 `~/.skillstar/config/antigravity_oauth.json`。
- Root cause: 本地桌面 OAuth 的公开 client 标识被错误地当成了每台机器都必须自行提供的配置；同时切号只更新 Usage active pin，没有写回 Antigravity IDE 的真实凭证存储。
- Fix: 使用参考桌面客户端同源的内置 OAuth fallback，保留 env / config file override；切换时写入并回读验证 macOS Keychain 或 legacy `state.vscdb`，验证成功后才更新 active pin。刷新前先采纳 IDE 已轮换的 access/refresh token，刷新后再把新凭证投影回 IDE；切换失败时明确返回“切换未生效”。
- Files: `crates/skillstar-usage/src/antigravity_oauth_config.rs`、`crates/skillstar-app/src/usage_switch.rs`、`crates/skillstar-app/src/usage_switch/antigravity.rs`、`crates/skillstar-usage/src/{protobuf_oauth.rs,vscdb.rs,tool_paths.rs}`、`docs/features/usage/README.md`。
- Self-check: Antigravity OAuth 无 env / 配置文件时仍能得到内置 client；切换后 `reconcile_cli_accounts` 必须从 IDE 真实存储读回目标 refresh token，而不是只依据 `active_per_catalog.json`；IDE 自行轮换 token 后，下一次 Usage refresh 必须先更新订阅再刷新。

## 2026-08-16 - 本地 GitHub 登录报 This build does not include a GitHub App client ID

- Symptom: 从源码跑 `tauri dev`，侧边栏 GitHub 账户点「开始登录」或「重试」后固定英文错误 `This build does not include a GitHub App client ID`。仓库根已有 `.env` 且填了 `SKILLSTAR_GITHUB_APP_CLIENT_ID` 仍然失败。
- Root cause: Client ID 只看进程环境变量和编译期 `option_env!`。Vite 会读 `.env`，Rust 后端不会；`ProductionGitHubGateway` 还在进程启动时把缺失结果缓存下来，所以 UI「重试」不会重新解析。官方 Release 靠 CI 仓库变量编进二进制，本地构建两者都空。
- Fix: 解析顺序改为环境变量 → 编译期嵌入 → 从 cwd / `CARGO_MANIFEST_DIR` 向上查找 `.env`；gateway 改为每次登录动作即时解析，不再缓存 Unavailable。
- Files: `crates/skillstar-skills/src/github_auth/gateway.rs`、`.env.example`、`docs/features/skills/README.md`。
- Self-check: 不 `export`、只在仓库根 `.env` 写 Client ID，重启 `tauri dev` 后「开始登录」必须进入设备码界面而不是这条 Unavailable；解析测试覆盖注释行、空值、引号，以及从 `crates/skillstar-skills` 子目录向上找到祖先 `.env`。

## 2026-08-15 - 声明了却没人写：角色面板收下用户输入然后丢掉，UI 与磁盘长期不一致

- Symptom: 三种表现，同一个根因。① Claude Code 的模型映射面板可以填 Sonnet/Opus/Haiku，保存后 `~/.claude/settings.json` 里没有任何 `ANTHROPIC_DEFAULT_*_MODEL`；重开面板值还在，因为它从来只活在渲染进程。② OMP 角色面板里一个角色显示 `某 provider/某模型`，`~/.omp/agent/config.yml` 的 `modelRoles` 里却没有该条目，且同步结果是绿色成功。③ 面板给每个模型都列出 9 个 thinking 等级，选了对没有推理档的模型无效的等级，也不会有任何提示。
- Root cause: 「角色」这个概念在 v3 有两处互不相通的实现——Claude 的层级模型在 `provider.meta`、OMP 的角色在 binding 的无 schema settings 袋——因此没有任何一层能回答「这个 Agent 支持哪些角色」。于是三件事各自失配：前端 Claude 面板只有 `useState`（后端契约其实早就就绪，断链在前端）；OMP 写盘函数 `resolve_omp_roles` 对「provider 未绑定 / 无端点 / 无模型」三种情况各有一个裸 `continue`，调用方拿不到任何差异信息；thinking 等级是一个全局 9 元常量，与模型能力无关。共同点是**声明与写盘之间没有约束**：UI 可以提供一个写盘侧根本不会处理的设置，而且没有任何机制会发现。
- Fix: 角色词表提升为域内类型 `providers::roles`（`RoleDef{id, agent_key, primary, inherits, requires}` + `DroppedRole`/`RoleDropReason`）；`AgentSpec` 增加 `roles` 列，每个 Agent 声明自己能投影的角色及其磁盘键名；`ToolSyncResultFlat` 增加 `dropped_roles`，OMP 与 Claude 写盘时逐条回报跳过原因，前端落到角色行并弹一次警告；`ModelCatalogEntry` 增加 `reasoning`，`omp_thinking_levels_for` 按能力裁剪等级；Claude 面板改为经 `update_agent_settings` 落到 `AgentBinding.roles`。
- Files: `crates/skillstar-models/src/providers/roles.rs`、`crates/skillstar-models/src/tool_sync/{agents,sync,omp_provider,types}.rs`、`crates/skillstar-app/src/models/agents.rs`、`src/features/models/components/hub/matrix/rich/ClaudeMappingPanel.tsx`、`src/features/models/api/{agents,activations}.ts`。
- Self-check:
  - 通用判据：**只要一处「声明能力」和另一处「实现能力」分处两个文件，就必须有测试把两者对上，否则它们迟早不一致**。这里是 `every_declared_role_reaches_disk`：给每个 Agent 的全部声明角色赋值、跑真实 writer、断言每个 `agent_key` 出现在写出的字节里。新增角色但忘了改 writer 会直接红。
  - 「成功」与「完整」是两个问题。写盘成功不代表用户配的东西都写进去了，差集只有 writer 算得出来，所以它必须是返回值的一部分而不是日志。任何新 writer 若有 `continue`/`skip` 分支，都要顺手回报原因。
  - 前端不要重算后端的跳过规则。规则复制一份就会在 writer 改动时过期；`useRoleDrops` 只记住后端的裁决。
  - 存储键与磁盘键不是一回事。v4 迁移把 `smol` 改名为 `fast`，前端若继续按 `smol` 读写，老用户的角色会「消失」同时旁边多出一条重复角色。`registry_agent_keys_match_the_migration_table` 与 `ompRoles.test.ts` 分别钉住两侧。
  - 「不知道」不能渲染成「不支持」。模型目录没有 reasoning 数据时必须给出完整等级表，只有明确的能力声明才收窄。

## 2026-08-14 - 稀疏 checkout 按 Skill 名去重会把仍存在的已安装来源误报为删除

- Symptom: 更新 `impeccable` 时弹出「来源已不再提供」，选择「彻底移除该 Skill」却收到 `Skill 'impeccable' still comes from its source; keep or discard the local changes instead of removing it`。Git reflog 显示每次更新都先 reset 到远端提交、随即回滚；远端提交仍包含 lockfile 记录的 `.agents/skills/impeccable`。`--list` / `--preview` 稀疏检出后只留下 `.agent/skills/impeccable`（Antigravity），Hub 与 `~/.cursor/skills/impeccable` 变悬空。
- Root cause: 远端新增了同名的 `.agent/skills/impeccable` provider 副本。`derive_sparse_skill_dirs` 曾为节省物化范围按 Skill 目录名（basename）去重，`.agent/...` 与 `.agents/...` 的 `source_priority` 相同且前者按字典序先出现，于是 sparse set 只保留 `.agent/...`。`git sparse-checkout set` 随即移除已安装链接指向的 `.agents/...` 工作树目录。更新路径还会把根 `SKILL.md` 当成“整仓 checkout”信号，进一步丢掉嵌套 harness 目录。
- Fix: 稀疏检出保留**全部**含 `SKILL.md` 的嵌套目录（不再按 basename 去重）；根 `SKILL.md` 只在没有嵌套技能时才触发全量 checkout。repo-cache 更新仍合并已安装 `source_folder`。真正被远端删除的路径即使留在 sparse pattern 中也不会被物化。
- Files: `crates/skillstar-skills/src/repo_scanner/ops.rs`、`crates/skillstar-skills/src/skill_update/tests/source_dropped.rs`。
- Self-check:
  - 通用判据：**发现结果的去重策略不能改写已经安装的 provenance**。同名 provider 路径用于“新安装选哪个”，lockfile 的 `source_folder` 用于“已安装项继续跟哪个”；两者不是同一个问题。
  - 回归 fixture 必须从仅有 `.agents/skills/impeccable` 的 sparse checkout 开始，再让远端同时保留该路径并新增 `.agent/skills/impeccable`；更新应直接成功，不能产生 `SourceRemoved`。
  - 同时保留真正删除来源的测试：远端确实移除原路径时，即使 sparse pattern 仍含旧目录，目标仍不存在，必须继续进入可保留副本/移除的停止状态。

## 2026-08-14 - 跨 provider 的时间戳单位不是常识：Claude Code 的 expiresAt 是毫秒

- Symptom: 尚未发生（新增 `anthropic` fetcher 时提前拦下）。若直接把 `claudeAiOauth.expiresAt` 写进 `Subscription::access_token_expires_at`，过期时刻会落在约五万年后，`token_refresh::needs_refresh` 之类的过期判定对该行**永远返回 false**——不会报错，只会静默地永不失效，等到上游真的 401 才暴露，且届时错误分类正确、症状却是“卡片突然要求重新登录且没有任何前兆”。
- Root cause: `skillstar-usage` 内所有既有 provider 的 token 过期都是 epoch **秒**（`TokenResponse::expires_at()`、`jwt_exp`、`Subscription::access_token_expires_at` 全是秒），于是“过期字段是秒”变成了一条没有人写下来、也没有类型系统兜底的隐含约定。Claude Code 是 Node 生态出身，`Date.now()` 给的是毫秒。跨生态借用凭证文件时，**字段名相同不代表单位相同**，而 `i64` 对两者一视同仁。
- Fix: 反序列化字段命名为 `expires_at_ms` 并只经 `ClaudeOAuth::expires_at_seconds()` 出口（`/1_000`），doc comment 直接写“禁止直接使用该字段”；配一个断言毫秒→秒换算的单元测试。
- Files: `crates/skillstar-usage/src/fetchers/oauth/anthropic.rs`、`anthropic_tests.rs`。
- Self-check:
  - 通用判据：**从别的工具的凭证文件/配置文件里读时间戳，先确认单位**。判据很便宜：把值当秒解释，看看落在哪一年。落在 2100 年之后就是毫秒。
  - 通用判据：**单位应该编码进字段名而不是注释**（`expires_at_ms` vs `expires_at`），并且只留一个换算出口；靠调用点自觉乘除迟早漏一处。
  - 回归判据：任何新增的、从第三方 CLI 读凭证的 fetcher，必须有一个“过期字段单位”的单元测试，而不是等 401。

## 2026-08-14 - 手抄的 DTO 会把“键缺席”抄成 `null`

- Symptom: 前端类型写 `total: number | null`，运行时拿到的却是 `undefined`；`x === null` 分支永不触发，而 `x ?? fallback` 侥幸把它掩盖成“看起来能跑”。对称的第二种：永远序列化的 bool 被手抄成可选 `?`，于是前端为一个不可能缺席的字段写了永远走不到的兜底分支。`/usage` 一次性暴露了这两类共六处。
- Root cause: Rust 字段带 `#[serde(default, skip_serializing_if = "Option::is_none")]` 时，`None` 让**整个键从 JSON 里消失**，而不是序列化成 `null`。手抄类型的人凭 `Option<T>` 直觉写成 `| null`，就此漂移；两种写法都能通过 `tsc`，也都能通过 code review，只有运行时行为不同，且不同的那一半是“分支静默失效”而不是报错。
- Fix: 不要手抄。让 ts-rs 生成——它读 serde 属性，两种都不会错。`src/features/usage/types.ts` 已退化为 re-export barrel，决策见 [decisions.md](./decisions.md) D-034。
- Files: `crates/skillstar-app/src/usage/dto.rs`、`src/types/generated/`、`src/features/usage/types.ts`。
- Self-check:
  - 通用判据：**任何手写的、镜像后端形状的前端类型都是待爆的漂移**，review 抓不住它，因为两侧各自都自洽。判据是“这个形状有没有 Rust 来源”，有就必须生成。
  - 回归判据：`bash scripts/internal/check_generated_types.sh` 是门禁；新增 DTO 时先确认它进了 `types:gen`，而不是先在前端写一份。

## 2026-08-12 - 共享 skills 目录：没有归属记录时，清理一律退化成"看起来像技能就删"

- Symptom: 多个 Agent 解析到同一个物理 skills 目录时，一方的移除会静默删掉另一方仍在使用的技能，两侧各有独立表现。Global 侧：cline 与 zed 共用 `~/.agents/skills`，在 SkillCard 或 Settings 里对 cline 取消部署某技能，zed/warp/loaf/dexto/kimi-code-cli 五个仍启用的 Agent 一并失去它（`deployment/mod.rs:305-336`，全程无共享检查，且用的是 `require_global_profile` 而非 enabled 版本，对已禁用 Agent 也照删）。Project 侧：卸载 hub 技能时 `remove_skill_from_all_projects`(`projects/sync.rs:132-157`) 对**全部** profile（含从未启用的、含 openclaw 的项目根 `skills`）拼路径，只要 `is_link || is_dir` 就删，用户手写的 `.agents/skills/foo` 被静默删除；一次 `full_sync` 的 `clear_project_symlinks`(`projects/helpers.rs:36-39`) 更是按目录清空而非按 manifest 清空，共享目录里所有未登记技能一次 Apply 后消失。UI 侧的伴生症状是计数串台：6 个共用 `~/.agents/skills` 的 Agent 各自显示同一个总数（`registry.rs:131-153`），任一 Agent 装了技能则 6 个图标一起亮（`installed_skill.rs:532-548`）。
- Root cause: 一个共同的形状——**磁盘上不存在"这条 entry 是谁装的"这一信息，而代码在需要它时退化成了启发式**。Global 侧 `deployment/` 下根本没有任何归属记录（Project 侧至少有 `skills-list.json`），于是删除判据只剩 `fs_ops.rs:265-274` 的"是 link，或者是含 `SKILL.md` 的目录"——这个判据对"是不是一个技能"回答正确，对"该不该由我删"完全失明。Project 侧有 manifest 却在三条路径上不查它（cleanup、clear、rebuild），等价于退回同一个启发式。更深一层：**per-agent 归属在物理世界从未存在过**。`~/.agents/skills` 是 open agent skills 生态的共享约定，zed 进程 `ls` 一下就能加载 cline 部署的技能；系统曾试图用 manifest 维护一个磁盘无法兑现的隔离承诺，于是任何"存量已有部署但无归属记录"的起手都没有正确答案。这类缺陷在**单所有者场景下永远测不出来**——一个目录一个 Agent 时，"看起来像技能就删"和"是我装的才删"给出完全相同的结果。
- Fix: 决策见 [decisions.md](./decisions.md) D-024（**已定，落地未开始**）：目录塌缩为一等部署单元 + 归属零状态推导（entry 属于 SkillStar ⟺ 其链接目标落在 `hub_skills_dir()` 下，已验证 5 个全局写入点 src 全是 hub 绝对路径且 `read_link_resolved` 只解一跳）。容器判定**必须复用** `repo_link::is_inside`(`repo_link.rs:65-93`)，不要新写 `starts_with`。
- Files: `crates/skillstar-skills/src/deployment/mod.rs`、`crates/skillstar-skills/src/projects/{sync,helpers,rebuild,scan}.rs`、`crates/skillstar-skills/src/{installed_skill,repo_link}.rs`、`crates/skillstar-skills/src/agents/{builtin,registry,custom}.rs`、`crates/skillstar-core/src/infra/fs_ops.rs`。
- Self-check:
  - 通用判据：**当一个物理资源可被多个逻辑所有者共享时，"删除"必须由归属回答，不能由"这东西长得像不像我管的类型"回答**。后者在单所有者下与前者等价，因此不会被现有测试发现；任何按 `read_dir` 遍历后逐项判断"像不像技能"的清理循环都是这个反模式的实例。
  - 通用判据：**归属信息如果在物理世界不存在，就不要用状态去发明它**。存量数据没有正确的回填起手（记为无主/归给全部/归给第一个都错），且清单与磁盘是两次写、崩溃必然分叉。优先找一个能从磁盘零状态推导的谓词。
  - 通用判据：**同一个容器判定不允许有第二份实现**。`repo_link.rs:4-9` 已记录过 Windows junction 因两份实现分叉而误判的事故；今天 `local_skill.rs:174`、`git/gh_manager.rs:432`、`storage_maintenance.rs:183` 三处裸 `resolved.starts_with(&dir)` 缺 canonicalize 与大小写折叠，是同一事故的复发预备队。
  - 回归判据：任何新增的"清理/取消部署/清空目录"路径，必须先回答"这个目录还有没有别的已启用 Agent 解析到它"。用 `crates/skillstar-skills/src/agents/builtin.rs` 的 `BUILTIN_AGENT_DEFS` 按 `resolve_global_dir` 与 `project_skills_rel` 分组即可枚举出全部共享组；今天共 3 组 global 与 4 组 project 共享目录。
  - 反例警告：不要把 `deploy_modes` 当遗留字段。`docs/features/skills/README.md` 曾声称它"只为兼容旧 manifest"，实际 `projects/sync.rs:78-84,227,461-463` 正在读写它决定 symlink/copy。

## 2026-08-12 - tool-sync 单元测试并行必挂：所有测试共用同一个 sandbox HOME

- Symptom: `cargo test --workspace`（默认并行）随机挂 1–2 个 skillstar-models 测试，`--test-threads=1` 必过。典型是 `tool_sync::tests::part1::test_codex_official_sync_oauth_preserves_auth_json` 断言 `access_token` 拿到别的测试写的值，以及 `tool_sync::agents::tests::registry_paths_match_legacy_resolvers` 比较两个 home 解析结果时左右不一致（一个是 `skillstar-toolsync-test-<pid>` 回退目录，一个是 sandbox TempDir）。
- Root cause: `tool_sync/tests/mod.rs` 的 `use_sandbox_home()` 用一个 `LazyLock<TempDir>` + `std::env::set_var(SKILLSTAR_TOOL_SYNC_HOME, ...)`，所有测试共享**同一个** sandbox HOME，于是共享同一个 `~/.codex/auth.json` 并互相覆盖；而 `set_var` 是进程级的，在 `LazyLock` 首次初始化的那一刻翻转，正在并行跑的其它测试会在同一个测试体内前后读到两个不同的 home。“设了临时目录”不等于“隔离”。
- Fix: 不再改进程级环境变量。`tool_sync/mod.rs` 增加 `#[cfg(test)]` 的 thread-local `TEST_HOME_OVERRIDE`，`sandbox_home()` 优先读它；`use_sandbox_home()` 返回一个 RAII guard，为每个测试新建独立 TempDir 并在 Drop 时还原。libtest 一个测试一个线程，因此 thread-local 天然给出 per-test 隔离，且完全不需要 `unsafe set_var` / 互斥锁 / 串行化。
- Files: `crates/skillstar-models/src/tool_sync/mod.rs`、`crates/skillstar-models/src/tool_sync/tests/mod.rs`。
- Self-check:
  - 通用判据：**进程级环境变量不能用来做并行测试隔离**。测试隔离的作用域必须和测试的并发单位一致——libtest 的并发单位是线程，那么覆盖点就得是 thread-local，而不是进程全局。
  - 连续跑 5 次 `cargo test -p skillstar-models --locked`（默认并行）必须全绿；出现 `--test-threads=1` 才过的现象即为回归。
  - 新增任何会解析 home 的 tool-sync 测试，必须 `let _sandbox = use_sandbox_home();` 并**持有** guard（`SandboxHome` 带 `#[must_use]`，写成裸调用会告警）。
  - AGENTS.md「tool-sync 测试必须设置 `SKILLSTAR_TOOL_SYNC_HOME` 到临时目录」的意图是**每个测试各自隔离**，不是共用一个目录。

## 2026-08-12 - degraded 收尾：同步状态行自相矛盾、降级判据只问了一个 scope、绕过快照的读路径不报降级

- Symptom: 三个都不会立刻可见，都会让上面几条已经修好的契约在边角上重新破洞。(1) 配了 `config/marketplace_mirror.json` 且镜像与主站内容分叉时，新鲜度会莫名滞后：带着上一份载荷的 ETag 去问新 host，本该 200 的请求被答成 304。(2) 只逛过 hot / trending、从没同步过 all 的用户，搜索会把 hot 的降级兜底行报成 `Fresh`。(3) 本地 SQLite 读失败改走远端直取时，即使远端返回的也是降级载荷，用户只看到「不是来自快照」，看不到「这批数据不完整」。
- Root cause: 一个共同的形状——**降级/一致性信息在某一条路径上被丢掉了，而那条路径恰好没人问过它**。
  1. `mark_scope_success_with_meta_in_tx` 的 `etag = COALESCE(excluded.etag, 旧值)` 是为「同字节 200 轮换 validator」写的，却同时作用在**完整重写**路径上。`source_host` / `payload_sha256` 都有 `CASE WHEN fetched_unchanged` 保护，只有 `etag` 没有，于是一次不带 `ETag` 头的 200 会让同一行里 `etag` 描述上一份载荷、另外两列描述这一份。
  2. `search_local` / `ai_search_local` 只问 `scope_is_degraded(leaderboard_all)`，但 `hot` / `trending` 的兜底行经由同一个 `upsert_skill_in_tx` 写进同一张 `marketplace_skill`。判据的范围比数据的范围窄。
  3. 七处 `ErrorFallback` 分支调用的是 `remote::get_*`（丢掉 `FetchMeta` 的 `.0` 包装），而 `ErrorFallback` 是唯一完全不读 `marketplace_sync_state` 的读路径——远端的 `degraded` 一旦在包装层丢掉，下游再也无从得知。
- Fix: (1) 拆开两条路径：`etag = CASE WHEN fetched_unchanged THEN COALESCE(新,旧) ELSE excluded.etag END`。完整重写时 etag 跟随本次载荷，没有就置 NULL。不变式写在函数文档里：**同一行的 `etag` / `payload_sha256` / `source_host` 必须描述同一份载荷**；no-change 路径采纳轮换后的 validator 不违反它（字节相同即同一份载荷）。(2) 新增 `shared_skill_table_is_degraded(conn)`，一次 `EXISTS` 查完 all / hot / trending 三个 scope（搜索每次按键都会走，不能做成逐 scope 查询）；scope 列表由 `leaderboard_scope()` 派生，不手抄。(3) 七处 `ErrorFallback` 改走 `remote::*_with_meta(..., None)`，统一经由新的 `error_fallback(data, local_err, meta)` 构造；`meta.degraded` 为真时把原因追加到 `error` 字段（状态枚举与健康路径共用，塞不下第二根轴）。
- Files: `crates/skillstar-marketplace/src/snapshot/{sync_state.rs,local_first.rs}`、`crates/skillstar-marketplace/src/snapshot/tests/part6.rs`。
- Self-check:
  - 通用判据：**同一行里描述同一个对象的几列，必须由同一条写路径同时写入**。只要有一列走了 `COALESCE` 而邻居走了 `CASE`，这一行迟早自相矛盾，而且没有任何下游能发现。
  - 通用判据：**判据的范围必须覆盖数据的范围**。多个 scope 往同一张共享表写行时，只问其中一个 scope 的质量标记，等于对其余几个失明。
  - 通用判据：绕过状态存储的旁路（直取远端、缓存穿透、应急通道）也必须携带质量信息，否则「降级数据永不冒充完整数据」只在主路径上成立。
  - `sqlite3 ~/.skillstar/db/marketplace.db "SELECT scope,source_host,substr(payload_sha256,1,8),etag FROM marketplace_sync_state;"` —— 只在配了镜像时有意义：`source_host` 刚变过而 `etag` 没跟着变（也没被清空）即为回归。
  - `cargo test -p skillstar-marketplace part6`、`cargo test -p skillstar-marketplace degraded`。

## 2026-08-12 - Marketplace 默认 all tab 只看"表里有没有行"，永远报 Fresh

- Symptom: 最常用的 all tab 永远显示"已是最新"：TTL 过期不刷新、降级兜底数据不带任何提示、前端的自动 stale 刷新 / 重试额度 / 重试按钮在这个视图上从不触发。hot / trending 却一切正常。
- Root cause: `list_skills_local()`（`list_marketplace_skills_local` 命令，`Marketplace.tsx` 默认 tab）只查 `marketplace_skill` 有没有行，非空即 `Fresh`，完全不读 `marketplace_sync_state`。于是一次搜索 seed 留下的一行、或 degraded 兜底写入的 200 行，都会让它报 Fresh；`Stale` 在这条命令上根本不可达，前端所有 stale 自愈逻辑成为死代码。同一文件里 `get_leaderboard_local` / `get_publishers_local` 等读路径都是按 scope 判新鲜度的——只有它例外。
- Fix: `list_skills_local()` 的数据仍读全表，但新鲜度、seed 状态、`updated_at` 一律来自 `leaderboard_all` scope——正是前端刷新（`sync_marketplace_scope("leaderboard_all")`）和 `schedule_startup_refreshes` 重试的同一个 scope。状态集与 hot/trending 对齐：Fresh / Stale / Miss / Seeding / ErrorFallback / RemoteError。同时本地 SQLite 读失败不再伪装成 `RemoteError`（前端会显示"检查网络或代理"却附一句 `database is locked`），改为与其它读路径一致的"先试远端 → ErrorFallback，双双失败才 RemoteError"。
- Files: `crates/skillstar-marketplace/src/snapshot/local_first.rs`。
- Self-check:
  - 通用判据：**读路径的新鲜度判据必须和写路径的 scope 是同一个键**。任何"有行 = 新鲜"的判断都等于把 TTL 和降级标记全部作废。
  - `sqlite3 ~/.skillstar/db/marketplace.db "SELECT scope,last_success_at,next_refresh_at,degraded_reason FROM marketplace_sync_state WHERE scope='leaderboard_all';"` —— `next_refresh_at` 为空或过期时，all tab 必须显示 stale。
  - `cargo test -p skillstar-marketplace the_all_tab`。

## 2026-08-12 - Marketplace degraded 快照自锁：兜底数据写进去就再也刷不出来

- Symptom: 上游榜单 HTML 改版后，用户看到 200 行兜底数据 + 一条永久错误条 + 一个点几次都必然失败的重试按钮。既没有接受降级数据的出口，也没有停止报错的出口。
- Root cause: 两条守卫互相打架。空库时允许写入 degraded 兜底并把 `next_refresh_at` 清空（立刻 stale）；但另一条守卫只要「本次 degraded 且本地有行」就直接失败返回。第一次兜底 seed 成功之后本地就有行了，于是之后每一次刷新都确定性失败，永远失败。根因是**"本地有没有行"回答不了"本地这批行是好数据还是兜底数据"**——降级状态只有进入语义、没有退出语义，也没有持久化标记。
- Fix: 给 `marketplace_sync_state` 加 `degraded_reason` 列（schema v12，幂等 ALTER），由 `mark_scope_success_with_meta_in_tx` 在**写数据的同一事务内**维护：degraded 载荷 → 不给 TTL + 写 `degraded_reason`，且 `last_error` 保持 NULL（同步确实成功了，`last_success_at` 与 `last_error` 同时非空会被所有诊断消费方读成失败）；完整载荷 → 清 `degraded_reason` + 恢复 TTL，这是唯一的退出；304 / 同字节 → 行没变，质量也不变，保留原 `degraded_reason`（否则一次 304 就能给兜底数据发一个完整 6 小时 TTL）。判定收敛到纯函数 `plan_refresh(meta, previous_sha256, has_local_rows, stored_is_degraded)`：拒绝用降级数据覆盖**完整**快照，但允许降级数据替换**同样降级**的快照。
- Files: `crates/skillstar-marketplace/src/snapshot/{sync.rs,sync_state.rs,migrations.rs,mod.rs}`。
- Self-check:
  - 通用判据：任何"拒绝写入"的守卫都必须能回答"被保护的到底是什么"。用行数代替质量标记，第一次降级写入就会把守卫变成自锁。凡是可能确定性重复失败的路径，必须明确写出退出条件。
  - `sqlite3 ~/.skillstar/db/marketplace.db "SELECT scope,last_success_at,last_error,next_refresh_at,degraded_reason FROM marketplace_sync_state;"` —— `degraded_reason` 非空时 `next_refresh_at` 必须为空。数据可不可信只看 `degraded_reason`，判据见下一条故障的自检（`last_success_at` 与 `last_error` 同时非空是正常可达状态，不是回归）。
  - `cargo test -p skillstar-marketplace plan_refresh`、`cargo test -p skillstar-marketplace degraded`。

## 2026-08-12 - degraded 机制在它唯一要防的场景里不生效，进去了还出不来

- Symptom: 上一条修好的「自锁」并没有让机制真正工作。skills.sh SSR 改版后，用户拿到的仍然是 200 条模糊匹配冒充完整榜单、完整 6 小时 TTL、"已是最新"；而一旦真的进入 degraded（空库首次兜底 seed），又会被内容寻址永久钉死在兜底数据上，前端每次会话 3 次自动刷新全部 `SkipRewrite`，看起来成功、数据不变、无出口。
- Root cause: 三个各自独立、方向相同的错误。
  1. **判定点在合并之后**：`meta.degraded = true` 的前提是 `skills.is_empty()`，但在它之前 `fetch_all_skills_via_api()` 已经把同一个兜底端点的 ≤200 条追加进 `skills`。真实主场景（HTML 解析 0 条 → API 补充 200 条）于是产出 `len()==200 && degraded==false`。degraded 只在「同一个 URL 第一次失败、紧接着第二次成功」这条不存在的抖动路径上可达——整套 v12 列 / `plan_refresh` 分支 / TTL 抑制 / 前端 stale 标签全部空转。
  2. **出口被内容寻址挡住**：`plan_refresh` 的真值表漏了 `(stored_is_degraded=true, meta.degraded=false, payload_unchanged=true)`。degraded 时存下的 `payload_sha256` 是那份**当时解析不了**的载荷的指纹；解析器修好后重新解析同一份字节正是主要恢复路径，却恰好被跳写命中。
  3. **拿不到 body 就无从判断**：`conditional_etag` 只看行数不看质量，degraded scope 照发 `If-None-Match`，上游字节没变就一直 304，永远没有载荷可重新解析。
- Fix: (1) 降级判定移到合并之前，抽成纯函数 `combine_leaderboard(html_skills, api_skills) -> (skills, degraded)`：`degraded` 由「HTML 有没有解析出榜单」决定；两半都空则返回 Err（没有载荷 ≠ 降级载荷，不允许用空榜单覆盖快照）。(2) `plan_refresh` 增加「stored degraded 且本次完整且**有真实 body**」强制 `Rewrite`（304 无 body，仍走 `SkipRewrite` 以保留标记而不是把 scope 重写成空）。(3) `conditional_etag(previous_etag, has_local_rows, stored_is_degraded)`：degraded 时不发 ETag。(4) `commit_unchanged` 路径改为 `etag = COALESCE(excluded.etag, 旧值)`——同字节响应也可能轮换 validator，留着旧的等于以后再也拿不到 304。
- Files: `crates/skillstar-marketplace/src/remote/{leaderboard.rs,tests.rs}`、`crates/skillstar-marketplace/src/snapshot/{sync.rs,sync_state.rs,local_first.rs}`、`crates/skillstar-marketplace/src/snapshot/tests/part4.rs`。
- Self-check:
  - 通用判据：**降级/拒绝类判定必须放在「还分得清好数据和兜底数据」的那一刻**。任何在合并、追加、去重之后再问「结果是不是空的」的判定，都会在真实降级场景里恒假。
  - 通用判据：跳写（内容寻址、`If-None-Match`、缓存）与**质量升级**是两条正交的轴。只要「本次载荷和上次一样」能压过「上次那份是坏的」，坏状态就没有出口——凡是有降级态的地方都要显式写出 exit 优先级。
  - 通用判据：`last_success_at` 与 `last_error` 同时非空是**正常可达状态**（任何一次成功之后的刷新失败都会产生），它不表示「没有数据」。数据可不可信只由 `degraded_reason` 回答。
  - `sqlite3 ~/.skillstar/db/marketplace.db "SELECT scope,next_refresh_at,degraded_reason,etag FROM marketplace_sync_state WHERE scope LIKE 'leaderboard%';"` —— 榜单行数骤降到 ~200 而 `degraded_reason` 为 NULL 即为回归。
  - `cargo test -p skillstar-marketplace degraded`、`cargo test -p skillstar-marketplace leaderboard`。

## 2026-08-12 - 测试失明（第二轮）：迁移只测了函数自己，接线和持久化读回零覆盖

- Symptom: 上一轮 degraded 修复带着新增测试合并，75 个测试全绿；红队把两处关键代码变异回缺陷版本，**仍然全绿**。
- Root cause: 两种同型错误，都属于「测了函数自己，没测它被接进哪里」。
  1. `stored_scope_state` 的 `degraded: state.degraded_reason.is_some()` 变异成 `degraded: false`（精确复活自锁）不转红：所有 degraded 测试都直接调 `mark_scope_success_with_meta_in_tx` 然后读列，从不经过「读回持久化状态 → 喂给 `plan_refresh`」这条线；`plan_refresh` 的真值表测试根本不碰 SQLite。
  2. `migrate_schema` 的 `if version < 12` 变异成 `< 11` 不转红：v12 的测试先调 `create_connection()`（整条链已经跑完、列已经在了）再直接调两次 `migrate_v11_to_v12`，测的是函数自身幂等，不是迁移有没有接进版本链。而升级路径是唯一会坏的路径——全新库从 version 0 走基础建表，永远看不到这个缺陷。
- Fix: (1) 把决定收敛到 `plan_scope_refresh(scope, meta)`——五个 `sync_scope_*` 都经过它，它内部完成「读回状态 → 判定」，测试直接断言它的返回值。(2) 新增手工种 `user_version = 11` 真实老库、走完整 `create_connection()` 的用例，并断言真正的症状（`scope_sync_state` / `get_marketplace_sync_states` 可读），不只断言列存在。(3) `stored_scope_state` / `scope_has_local_rows` 的读失败不再静默——加 warn 日志，因为读失败与上述变异等价。
- Files: `crates/skillstar-marketplace/src/snapshot/sync.rs`、`crates/skillstar-marketplace/src/snapshot/tests/part4.rs`。
- Self-check:
  - 通用判据：**迁移测试必须从「上一版本的真实数据库」开始，走完整启动路径**。从 `create_connection()` 开始的迁移测试只能证明幂等，不能证明接线；这类缺陷对全新安装完全不可见。
  - 通用判据：凡是「持久化状态 → 决策」的链路，测试必须**跨过持久化边界**。分别断言「列写对了」和「纯函数判对了」，中间那一段读回逻辑仍是零覆盖。
  - 变异复核清单（改完必须逐条确认转红）：`stored_scope_state` 的 `degraded` 恒 false；`migrate_schema` 的 `version < 12`；`combine_leaderboard` 的 `degraded` 恒 false；`plan_refresh` 的 stored-degraded 强制重写分支。

## 2026-08-12 - 测试失明：断言"中间量"和"两个 helper 各自"都不算测过

- Symptom: 上面两条 URL 拼接和跳写校验的修复，各自带着新增测试合并，但把修复代码变异回缺陷版本后 66 个测试**全绿**——测试名承诺的行为其实没有被测试。
- Root cause: 两种同型错误。(1) `join_url` 两端都做了防御，只喂给它规范化过的 host 就永远拼对，测的是它自己而不是真实请求路径；真实路径 `fetch_with_failover` 的 URL 构造零覆盖。(2) 测试分别断言 `payload_unchanged(...)` 和 `scope_has_local_rows(...)` 两个 helper，从没把它们**组合**起来跑过任何判定，所以删掉五处 `has_local_rows &&` 守卫全绿。
- Fix: 把两处判定各自抽成可测的纯函数并直接断言判定结果——`failover_targets_for(hosts, path)`（把 host 列表作为参数传入，才能覆盖"host 丢了尾斜杠"这个真正的缺陷形状）和 `plan_refresh(...)`。判定必须只有这一个落点，`sync_scope_*` 只做分派。
- Files: `crates/skillstar-marketplace/src/remote/{mod.rs,tests.rs}`、`crates/skillstar-marketplace/src/snapshot/sync.rs`、`crates/skillstar-marketplace/src/snapshot/tests/part4.rs`。
- Self-check:
  - 通用判据：新增测试后，**把修复代码手工变异回缺陷版本，确认测试确实变红**。测不红的测试等于没写。断言中间量（host 列表、单个 helper 的返回值）永远不构成对判定的覆盖。
  - 隔离副本做法：`rsync -a --exclude target --exclude node_modules --exclude .git ./ /tmp/mut/` 后在副本里改代码跑 `CARGO_TARGET_DIR=/tmp/muttarget cargo test -p skillstar-marketplace --lib`，不要在仓库里做变异。

## 2026-08-12 - Marketplace URL 拼接丢斜杠：所有远端请求打到 `https://skills.shhot`

- Symptom: 市场每个 tab 都红字 "Marketplace request failed" + 空列表，只有 Popular/All 榜单有数据；搜索永远失败且不自愈；配了 mirror 的用户完全正常。全套 58 个单测绿。
- Root cause: 主 host 常量 `https://skills.sh` 没有尾斜杠（mirror host 走 `normalize_host` 有），而 `fetch_with_failover` 用 `format!("{host}{}", path.trim_start_matches('/'))` 又把 path 的前导斜杠删掉 —— 两端都不负责分隔符。于是 `/hot` → `https://skills.shhot`（NXDOMAIN，curl code=000），只有 path 为 `"/"` 的 Popular/All 恰好拼对。测试只断言 host 列表、从不断言最终 URL，还把"主 host 无尾斜杠、mirror 有"这个自相矛盾的状态写成了期望值，所以全绿。
- Fix: `join_url()` 两端都防御 —— `format!("{}/{}", host.trim_end_matches('/'), path.trim_start_matches('/'))`；`marketplace_hosts()` 的主 host 也过 `normalize_host`。测试改为直接断言最终 URL。
- Files: `crates/skillstar-marketplace/src/remote/{mod.rs,tests.rs}`。
- Self-check:
  - `sqlite3 ~/.skillstar/db/marketplace.db "SELECT scope,last_success_at,last_error,source_host FROM marketplace_sync_state;"` —— 有 `last_attempt_at` 但 `last_success_at` 恒为 NULL，就是远端从来没通。
  - 通用判据：只要「host 是否带尾斜杠」和「path 是否带前导斜杠」由两处代码各自决定，就必然有一种组合拼错。断言中间量（host 列表）不算测过，必须断言最终 URL。

## 2026-08-12 - 首次同步失败把用户永久钉死在空市场，且看起来像"已同步"

- Symptom: 第一次打开应用时正好断网/被墙 → 首次报错，之后网络恢复也永远是空列表、无报错、重启无效；诊断面板显示该 scope 无任何错误。
- Root cause（三段叠加，每一段单独看都"合理"）：
  1. `sync_scope_*` 在发起网络请求**之前**先 `mark_scope_attempt` 插行，而 `sync_seed_state` 只按「这一行在不在」判定 `Synced` → 第一次失败后永远是 `Synced` → local-first 直接返回 `Miss`，不再重试远端。
  2. `is_scope_stale` 要求 `last_success_at` 非空 → 从未成功过的 scope 永远排不进启动刷新，兜底路径也失效。
  3. 远端 fetch 失败用 `?` 直接返回，从不调 `mark_scope_error`，而 `mark_scope_attempt_in_tx` 每次尝试开头又把 `last_error` 置 NULL → 库里 `last_error` 恒为 NULL，排查时看起来"没有错误"。
  另有同源的第四段：内容寻址只比对 `payload_sha256`，不看本地行是否还在 —— 本地数据丢失后只要上游字节没变就永远跳过重写，却报告成功 + Fresh。被墙网络恰恰最容易返回字节稳定的挑战页。
- Fix: `sync_seed_state` 改按 `last_success_at.is_some()` 判定；`is_scope_stale` 对「有行但从未成功」返回 true，让启动刷新重试；五个 `sync_scope_*` 的远端失败显式 `mark_scope_error` 后再返回 Err；跳写前增加本地行数校验，行数为 0 时连 ETag 都不发（否则 304 空 body 无法重建数据）。
- Files: `crates/skillstar-marketplace/src/snapshot/{sync_state.rs,sync.rs,local_first.rs}`。
- Self-check:
  - `SELECT scope, last_success_at, last_error, payload_sha256 FROM marketplace_sync_state;` —— 有行且 `last_success_at IS NULL` 就是本故障；`last_error` 必须能看到真实原因，恒为 NULL 说明失败路径又不写错误了。
  - 有 `last_success_at` + `payload_sha256`，但 `marketplace_listing` 是空的 ⇒ 跳写校验失效。
  - `cargo test -p skillstar-marketplace part4`。
  - 通用判据：「尝试过」不等于「成功过」。任何用行存在与否代表成功的状态机，都会被"失败前先插行"的写法反噬。

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
- Self-check: `cargo test -p skillstar-usage oauth_scopes_include_conversations_for_grok_cli`; `cargo test -p skillstar-app usage_switch` (the Grok-only module was replaced by the shared symlink custody engine — see D-033);  `bunx vitest run src/features/usage/hooks/useUsageData.test.ts`; switch from a running/valid Grok account to an old narrow-scope card and confirm SkillStar reports reauthorization without changing the current `auth.json`, then re-authorize and switch successfully without browser OAuth. The written OIDC entry must contain `create_time`, and a following Usage refresh must leave the card and `auth.json` on the same access/refresh generation.

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

## 2026-08-13 - MCP 写入路径宽松解析：畸形配置被静默清空，畸形 store 让全部 MCP 永久丢失

- Symptom: 用户 `~/.claude.json`（或 codex/opencode/zcode 配置）有语法错误、临时不可读时，点一次 MCP 开关就把整个文件替换成只含 `mcpServers` 的新 JSON，Claude Code 的其它设置全部消失；`~/.skillstar/config/mcp_servers.json` 解析失败时 MCP 页面显示为空，随后任意一次写入把用户全部 MCP server 永久覆盖。
- Root cause: 写入路径的读函数把「文件不存在」和「存在但读/解析失败」混为一谈，都回退成空 Map / `McpStore::default()`；而写入是整文件 `write` 或 `tmp+rename` 替换。宽松解析在只读场景（计数、探测）无害，在「读-改-写」场景等于把解析失败翻译成"用户没有配置"。store 侧还没有写前备份，覆盖后无从恢复。
- Fix: 读函数区分三态——不存在/空文件视为空配置并继续；读失败、解析失败、根对象或目标键类型不对一律返回错误，原文件不动。四条 upsert 与对应 remove 路径（claude-code/kiro/cursor 的 `mcpServers`、opencode 的 `mcp`、codex/grok 的 `mcp_servers` TOML、zcode 的 `mcp.servers`）统一走同一对 strict reader，legacy Desktop Chat 的 `_strict` 变体退化为别名。store 解析失败额外把原文件另存为 `.corrupt.<epoch_ms>`（同内容复用同一份，避免每次进页面堆一份）并把错误传播到命令层；`write_mcp_store` 覆盖已存在文件前复用 `tool_sync::create_rolling_backup`。
- Self-check: 对每条写入路径写一个「畸形文件存在 → 调用返回 Err → 文件字节完全不变」的测试；store 侧另外验证 `.corrupt.<ts>` 副本内容等于原文、重复读只留一份副本、二次写入留下内容等于旧版本的 `.bak.<ts>`。同时保留正向用例：缺失文件仍会被创建、既有的无关键（如 `theme`）不被改动。

## 2026-08-13 - 装第二个技能会永久锁死整个仓库；崩溃残留的 staging 会伪装成"无 lock entry 的已装技能"

- Symptom: 从同一个仓库安装第二个技能后，该仓库的所有安装、扫描和更新都失败并报 `Skill 'A' has local changes; preserve them as a local copy or explicitly discard them`，而用户从未编辑过 A；`refresh_skill_updates` 同时显示"无更新"，与错误自相矛盾。另一种形态是错误里点名 `.skillstar-remove-writer-1234` 这类用户看不见也无从处理的隐藏路径，同样锁死整个仓库。
- Root cause: 两个独立缺陷叠加在同一道门禁上。① `repo_scanner::cache` 在任何 scan/install 前无条件 `git fetch` + `reset --hard`，但 `scan_install` 只为本次 target 写 lock entry，同 checkout 的其它已装技能磁盘内容被推进、baseline 却停在旧 commit，于是被算成"本地分歧"。② 每条写入路径都用点开头的 staging 名（`.skillstar-remove-*`、`.skillstar-install-*`、`.importing-*`）做 rename 换入，进程被杀会留下残留；对 repo-cache 技能这个残留就是一个指向该 cache 的符号链接，而 `ensure_installed_checkout_is_clean`、`collect_skill_dirs`、patrol 的 hub 遍历都不过滤点开头条目，于是把它当成已装技能并要求它有 lock entry。卸载 staging 名里还带 `std::process::id()`，使得已有的自愈分支只能匹配当前进程的残留，跨进程崩溃留下的永远清不掉。
- Fix: `hub_entry::is_managed_hub_entry` 成为所有 hub 遍历共用的唯一判定（`cache.rs`、`installed_skill::collect_skill_dirs`、`patrol::collect_hub_skills`、`divergence`），点开头条目一律不算已装技能；`hub_entry::sweep_stale_staging` 在持有 update transaction lock 的安装与卸载入口清扫残留，且不走 `fs_ops::remove_link_or_copy`（那个函数对没有 SKILL.md 的目录会拒绝删除，而半写状态恰恰没有 SKILL.md）；卸载 staging 名去掉 pid，使自愈跨进程生效——安全前提是它的三个调用方都持有 transaction lock。`skill_update::refresh_baselines_after_checkout_reset` 在两处 reset 成功后为同 checkout 的全部已装技能刷新 baseline，只在门禁已证明 worktree 干净之后调用，因此内容变化必然来自上游而不是用户；内容在 reset 后消失的技能保留旧 baseline，那是更新路径识别"来源已删除"的依据。
- Self-check: 造一个含点开头 staging 残留（符号链接、无 SKILL.md 的半写目录、完整目录三种形态）的 hub，清扫后只有 staging 消失、真实技能与无关 dotfile 保留、被指向的 repo cache 内容不受影响；同一仓库连装两个技能后，第一个技能不得出现 local divergence，且该仓库的后续安装/扫描/更新都必须继续可用。

## 2026-08-13 - 检查失败被当成"没有更新"，一次离线巡检就把真实徽标擦成最新

- Symptom: 断网或 `.git` 损坏时跑一轮 patrol 或刷新，非 repo-cache 技能的更新徽标全部消失并落盘；恢复网络后界面显示"全部最新"，直到下一次成功检查才恢复。
- Root cause: repo-cache 路径的契约是"检查失败返回 `None` → 调用方跳过 → 保留上一次结果"，`installed_skill.rs` 与 `patrol.rs` 的跳过逻辑也都实现好了；但非 repo-cache 的回退路径写成 `Some(check_update_in_session(..).unwrap_or(false))`，patrol 侧写成 `Err => Some(false)`，任务 join 失败也写成 `Some(false)`。三处都把"不知道"断言成"没有更新"，而这个 `false` 会被 `commit_scan` 持久化。
- Fix: 三处统一改成返回 `None`（`.ok()`），交给既有的跳过逻辑。判定语义与 `docs/features/skills/README.md` 已经声明的"失败保留徽标"对齐。
- Self-check: 让检查返回 `Err`，`update_state` 中该技能的既有 `true` 必须原样保留而不是被写成 `false`；这条同样适用于 patrol 的任务 panic/取消路径。

## 2026-08-20 - 侧边栏收起卡顿：先修错了层，真正的成本在每帧重绘而不是重排

- Symptom: 点击侧边栏收起/展开，200ms 全程掉帧（不是点击那一下顿住，是整个滑动过程不连贯）。直觉指向 `Sidebar`，改侧边栏本身没有效果。
- Root cause: 第一轮按"动画布局属性 → 每帧重排"定的性，删掉 `#main-content` 的 `transition-[padding-left]` 改成 FLIP 之后，用户反馈毫无改善。实测 React 同步渲染开销只有 0.2–19ms（5 技能 / 403 节点的 Chrome repro），说明 JS 和重排都不是主导项。真正的成本是 WKWebView 的**每帧重绘**：`<aside>` 是 `z-50` 的半透明浮层，200ms 内持续改变 width，而它正压在 `.ss-main-chrome` 上——那张卡片有 `bg-card/80` 半透明底、`ring-1`、`rounded-l-[26px]`，以及一道 `shadow-[0_24px_80px_-40px_...]`。`index.css` 里那段注释早就写明这类模糊重绘的代价（"a 95px blur repaint across the whole column"），只是没人把它和侧边栏动画联系起来。全仓另有 66 处 `backdrop-blur`。
- Fix: 分两步。① 删掉 `#main-content` 的 `transition-[padding-left]`——即使不是主因，12 次全树重排也是纯浪费，这一步保留。② 二分：删掉 `<aside>` 的 `transition-[width]` 和临时加过的 FLIP，伸缩改为完全瞬时，零动画帧。
- Self-check: `src/App.test.ts` 断言 `#main-content` 的 className 不含任何 `transition-[`，并先断言取到的确实是那个元素（否则正则失配会让守卫静默通过）。更一般的教训有两条：包裹页面树的容器不允许 CSS 过渡布局属性，需要位移就走 transform；以及**动画卡顿不要只查重排**——半透明层、大模糊阴影、backdrop-filter 之上的任何几何动画，成本在合成器而不是布局，用测量而不是推理来定位（先问"是点击那一下顿，还是整个过程掉帧"，两者指向完全不同的层）。

## 2026-08-21 - 发布者仓库卡片显示 11 个技能，点进去只有 3 个，刷新也不收敛

- Symptom: `vercel/ai` 的仓库卡片写着 11 个技能，点进去列表只有 3 个；`publisher_repos` 与 `repo_skills` 两个 scope 都刷新过，数字照旧。skills.sh 自己的发布者页和仓库页都是 3。
- Root cause: 同一个仓库有两个写入方，互不校正。`publisher_repos:<publisher>` 读 `/official` 聚合载荷，把 `skill_count=11` 写进 `marketplace_repo`，并把内嵌的 11 条技能写进 `marketplace_repo_skill`；`repo_skills:<source>` 抓仓库页，把同一张技能表 delete+reinsert 成 3 条。卡片读的是 `marketplace_repo.skill_count` 这个缓存列，列表读的是技能行，于是 11 对 3 永久并存。更糟的是聚合页的 `totalInstalls` 几乎每次都变，指纹不同就整表重写，把过期的 11 条再灌回来，列表本身也在 11 和 3 之间来回翻。
- Fix: 两处。① `load_publisher_repos_snapshot` 的技能数改为从 `marketplace_repo_skill` 行数推导，没有行才回退到存储列——卡片与列表共用一个事实。② 聚合内嵌技能抽成 `seed_repo_skills_from_official_in_tx`，只给 `repo_skills:<source>` 从未成功过的仓库做种子；仓库页一旦抓过就是该仓库的权威，聚合不再覆盖。前端在 `repo_skills` 同步成功后同时失效 `publisherRepos` 查询，否则卡片缓存仍是旧值。
- Self-check: `snapshot/tests/part8.rs`——先种 3 条，模拟仓库页抓到 1 条并记成功，再跑一次种子必须仍是 1 条，且 `load_publisher_repos_snapshot` 对该仓库返回 1、对没有行的仓库返回存储列。更一般的教训：**一张表不能有两个不分先后的写入方**；任何"聚合页内嵌明细"都只配做种子，明细页一旦有自己的 scope 就要让位。
