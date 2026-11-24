# Error Log

状态：active

记录根因不直观、容易在后续修改或重构时复发的故障模式与自检方式。

## 2026-11-25 - 只在 macOS 上跑过的测试在 Windows 上的六种挂法

- Symptom: Windows CI 第一次跑到 `cargo test --workspace`：38 条失败，集中在 `ss-accounts`、`ss-channels`、`ss-skills`、`ss-mcp-hub` 的测试里；macOS / Linux 上同一棵树全绿。
- Root cause 与 Fix:
  1. **路径按字符串断言**：`s.ends_with(".codex/auth.json")`、`join("home/.grok/auth.json").display().to_string()`。Windows 的分隔符是 `\`。改用 `Path::ends_with` 与 `PathBuf` 相等——按分量比较，`/` 在 Windows 上同样是分隔符。
  2. **手拼 JSON 塞路径**：`format!(r#"{{"dataBaseDir":"{}"}}"#, base.display())`。Windows 路径里的反斜杠是非法的 JSON 转义，配置解析失败后回落到默认值。改用 `serde_json::json!` / `to_string`。
  3. **`File::open` 之后 `set_times` 倒填时间**：`File::open` 不带 `FILE_FLAG_BACKUP_SEMANTICS`，在 Windows 上打不开目录；只读句柄也没有 `FILE_WRITE_ATTRIBUTES`，对普通文件同样被拒。统一走 `test_sandbox::backdate`（Windows 上用 `OpenOptionsExt` 显式申请这两样）；只对文件时 `File::options().write(true)` 也行。
  4. **给 macOS 写的捷径没有平台条件**：`projects/binding.rs` 的 `symlink_pair` 借 `/tmp → /private/tmp` 造一对别名。Windows 上 `canonicalize("/tmp")` 读到的是当前盘上的 `\tmp`，捷径返回不带盘符的 `/tmp/…`，按 Rust 的定义不是绝对路径，被产品正确地拒绝。捷径现在只在 `Path::new("/tmp").is_absolute()` 时生效，Windows 走原本就备好的 junction 分支。
  5. **沙箱靠 `HOME` 重定向，被测代码却问操作系统**：`dirs::download_dir()` / `dirs::home_dir()` 在 Windows 上走「已知文件夹」接口，不看环境变量，用例把导出文件写进了 runner 的真实用户目录（用例自己的护栏拦下了）。把「导出到指定目录」从 `export_deck_bundle` 里拆出来（`export_deck_bundle_to`、`default_export_dir`），测试指定沙箱目录。
  6. **非 unix 的降级分支抽掉了用例的前提**：符号链接用例在 Windows 上退化成写一个普通文件，后面的断言却仍按链接写。只属于 unix 的性质用 `#[cfg(unix)]` 限定，平台无关的部分（BOM / CRLF 保留）单独一条在所有平台跑。
- Files: `crates/ss-accounts/src/{reconcile_tests.rs,service_tests.rs,switch/{codex,grok,kiro,zcode}_tests.rs,switch/paths.rs}`、`crates/ss-mcp-hub/tests/lossless_sync.rs`、`crates/ss-skills/src/{test_sandbox.rs,health/tests.rs,materialize_tests.rs,projects/binding.rs,skill_bundle/mod.rs}`。
- Self-check: 本机复现不了（Docker 虚拟机只有 2 核，跑不动完整测试腿），只有 CI 的 Windows 腿能验证，所以推送前按这六条自查新写的测试；`cargo test --workspace --locked --no-fail-fast` 让一轮 CI 把失败报全。

## 2026-11-25 - Windows 上导入普通本地文件夹、粘贴带盘符路径的命令、以 `file:///C:/…` 为来源都失败

- Symptom: Windows 上扫描一个不含 `.git` 的本地文件夹，报 `Failed to clone 'file:///C:/…' … does not appear to be a git repository`；粘贴 `npx skills add "C:\Users\me\repo" --skill x` 报「仓库格式无效」。此外凡是以 `local_file_url(临时目录)` 当来源的用例（技能更新、频道安装，20 余条）报 `Invalid source: Local path does not exist`。macOS / Linux 上同一操作正常。
- Root cause: 两处都只对 POSIX 路径成立。
  1. `ss_git::ops::local_file_url` 把盘符路径写成 `file:///C:/…`（三个斜杠；少一个 git 会把 `C:` 当主机名）。读回这个 URL 的地方却按 `C:/…` 开头判盘符，于是得到 `/C:/…`——带前导斜杠的路径在 Windows 上不是绝对路径。这样的读法有两份：`ss_skills::fetch::local_source_dir` 得到 `is_dir() == false`，`fetch_source` 跳过「就地借用本地目录」去 `git clone` 一个普通文件夹；`source_resolver::parse_local_source` 则直接报路径不存在。第一轮只修了前一处，21 条用例在第二轮才暴露后一处。
  2. `source_resolver::tokenize_shell_words` 在双引号内把任何反斜杠都当转义符吃掉（POSIX 里双引号内只在 `"`、`\`、`` ` ``、`$` 前才是；PowerShell 与 cmd 里根本不是转义符），`"C:\Users\me\repo"` 变成 `C:Usersmerepo`：含 `:` 又不是绝对路径，落到 `parse_github_shorthand` 报格式无效。不加引号的 `C:\Users\me\repo` 同理。
- Fix: `file://` URL 到路径输入的还原收拢成 `source_resolver::file_url_to_path_input` 一个函数，`local_source_dir` 与 `parse_local_source` 共用，在 Windows 上去掉盘符前那个属于 URL 的斜杠；分词器在双引号内按 POSIX 规则只在四个字符前当转义，不加引号的词一旦以盘符（`C:`）开头就整词保留反斜杠。
- Files: `crates/ss-skills/src/fetch.rs`、`crates/ss-skills/src/source_resolver.rs`、`crates/ss-skills/src/source_resolver_tests.rs`。
- Self-check: `cargo test -p ss-skills --locked`——`parse_install_input_keeps_windows_path_backslashes` 与 `file_url_path_input_keeps_the_caller_prefixing_rule` 三条腿都跑；`drive_letter_file_url_resolves_to_its_directory`、`source_parse_accepts_a_drive_letter_file_url` 与 `local_file_url_borrows_in_place` 在 Windows 腿守住。再写消费 `local_file_url` 产出的代码时走 `file_url_to_path_input`，不要另写一份还原逻辑。

## 2026-11-24 - 开发机上 `nsh test gui` 全绿，空的 `$HOME`（CI）上挂四条：装载尾的后台更新检查把夹具冲掉

- Symptom: 真实 `$HOME` 下 652/653，换成空的 `$HOME` 跑同一棵树挂 4 条：`App.test.tsx` 的侧栏折叠用例等 `id:skills-scroll` 超时；`mySkills.test.tsx` 的 `channel mutations …` 断言 `calls` 为 1 实得 2；`originMenu.test.tsx` 两条等 `skills:12` 超时，画面停在 `skills:0`。CI 的 runner 正是空库。
- Root cause:
  1. 空库的技能页走 `EmptyInstalled`，没有 `skills-scroll`（它只在 `CardPane` 里）。
  2. `state.ts` 的 `refresh()` 在 `loading.set(false)` 之后才调 `maybeCheckUpdates()`，读到的已经是刚播种的 hub 夹具。开发机的首装真实装载带 hub 技能，立刻起一次后台检查，`lastUpdateCheckAt` 落定、三十分钟节流窗口打开；空库机从没检查过，于是后台检查放行，`checkUpdates` 的 `finally` 再 `refresh()` 一次——计数用例多数一次，host 夹具被真实快照（空）冲掉。开发机上这次真实检查要走网络，冲掉的时刻晚于用例结束，所以一直没暴露。
- Fix: 按 `detailDrawer.test.host.tsx` 文件头的既有约定隔离：`originMenu` host 置 `busy` hold 挡住后台检查，并用 `setSnapshotLoaderForTest` 让后续 refresh 回到当前名单；计数用例在用例期间置 `busy`；侧栏折叠用例的等待锚点改为 `id:skills-scroll` 或空态的 `id:my-skills-goto-marketplace`。
- Files: `gui/App.test.tsx`、`gui/src/pages/my_skills/{mySkills.test.tsx,originMenu.test.host.tsx}`。
- Self-check: `CARGO_HOME="$HOME/.cargo" RUSTUP_HOME="$HOME/.rustup" HOME="$(mktemp -d)" nsh test gui`（真实的 `CARGO_HOME` / `RUSTUP_HOME` 要在覆盖 `HOME` 之前展开）。不要用 `SKILLSTAR_DATA_DIR` / `SKILLSTAR_HUB_DIR` 模拟：原生测试的 `IsolatedDataDir` 不中和 `SKILLSTAR_HUB_DIR`，真实 `~/.agents/skills` 也挡不住，会得出假失败。新写依赖 `skills` / `loading` 的测试 host，先对照 `detailDrawer.test.host.tsx` 的两道隔离。

## 2026-11-24 - 改了桥的签名，`nsh check gui` 仍按旧声明报 `Expected N arguments`，`nsh test` 的界面用例跑的是旧桥

- Symptom: 改 `gui/native` 的导出签名后，`nsh check gui` 在调用点报参数个数不对，重跑 `nsh check` / `nsh build` / `nsh test` 都不消失；`gui/.nsh/types/skillstar-gui/native.d.ts` 和 `gui/.nsh/rust/description.json` 的修改时间停在旧日期。界面用例对桥的改动没有反应。
- Root cause: nsh 每次带着 `CARGO_TARGET_DIR=<app>/target` 构建 describe 程序和测试运行器，构建完只要 `gui/target/debug/nsh-app-ss-gui-bridge-{describe,test,macos-host}` 存在就直接拿来用。huacnlee/cargo fork 忽略 `CARGO_TARGET_DIR`，产物落在 `gui/.nsh/rust/target`；`gui/target` 里留着fork 接管之前构建的旧程序，nsh 给旧 describe 程序盖的缓存戳一直不变，声明就被判定为最新。
- Fix: 把 `gui/target/debug` 下三个旧程序改名挪开（`*.stale-20261120`），nsh 按它自己的回退逻辑改用 `gui/.nsh/rust/target` 下的新程序，声明随即重新生成。`gui/target` 整个目录是切换前的遗留，当前工具链不再写入。
- Files: 无源码改动；机器上的 `gui/target/`（git 忽略）。
- Self-check: 改桥签名后看 `gui/.nsh/rust/description.json` 的修改时间是否跟着变；不变就查 `ls gui/target/debug/nsh-app-ss-gui-bridge-*`。在 cargo fork 与官方 cargo 之间来回切换之后尤其要看。

## 2026-11-24 - 市场安装完成后卡片停在「安装」，从卡片卸载又卸掉了同名的另一条技能

- Symptom: 市场页给一张卡点「安装」，通知带报「已安装 …」之后卡片仍是「安装」，详情列也没有「已安装」徽章；切 tab 或重新搜索后才变成「卸载」。变成「卸载」后再点，卸掉的是同名的另一条本地技能，刚装的那条还在。
- Root cause: 规范库里的同名目录已被别的来源占用时（本例是认领进来、软链已悬空的 `ui-ux-pro-max`），安装器按同名异源并存规则把新技能落成 `ui-ux-pro-max-nextlevelbuilder`，原名记在 `library.json` 的 `called`。桥的 `install_skill` 返回的是落盘名，读侧 `commitInstall` 拿它去匹配行名，匹配不到就不翻；页面安装后不重读，于是一直停在旧状态（域侧重读时按 `(source, called)` 判得对，CLI `skillstar find` 显示 `inst`）。卸载方向同理：读侧只传行名，桥按名卸载，删的是占用原名的那条。
- Fix: 读侧（市场页与发布者页各一份）成功后翻被操作的行 `skill.name`，落盘名对应的行一并翻；`uninstaller` 多带一个行的 `source`，桥 `Marketplace.uninstall_skill(source, name)` 先用 `ss_marketplace::snapshot::installed_folder_for` 解析落盘目录再卸。解析与「已安装」判定共用同一张索引（`InstalledIndex`）。
- Files: `crates/ss-marketplace/src/snapshot/{local_first.rs,mod.rs,tests/part9.rs}`、`gui/native/src/facades/marketplace.rs`、`gui/src/pages/marketplace/{readSide.ts,index.tsx,installIdentity.test.ts}`、`gui/src/pages/marketplace/publisher/{readSide.ts,index.tsx}`。
- Self-check: `cargo test -p ss-marketplace --locked part9`、`nsh test gui/src/pages/marketplace/installIdentity.test.ts`。改桥的 `install_skill` / `uninstall_skill` 签名或返回值时，两个读侧的 `commitInstall` 要一起看：身份是市场行，落盘名只是结果。

## 2026-11-24 - 市场多个技能同时安装，后点的卡把先点的卡的转圈顶停

- Symptom: 市场页连点两张卡的「安装」，第一张卡按钮上的转圈和阶段句在第二张开始后收起（右上角任务卡片两张都在）。先发起的那个装完，还会把仍在飞的另一张也解开，让它可以再点一次。
- Root cause: 读侧的忙碌状态是单值 `signal<string | null>`，只装得下一个技能名：第二次 `commitInstall` 直接覆盖，任一 promise 落地又无条件写回 `null`。发布者页有一份同构的读侧，写法相同。按项的等待（忙碌 / 刷新中 / 探测中）要以键集合记，单值只适用于整页一次一个的操作（账号的 `refreshingIds`、MCP 的 `probingSet` 是对的写法）。
- Fix: `busy` 改为 `ReadonlySet<string>`（技能名 → 在飞），每次整组替换新的 `Set`，空态共用 `NO_BUSY`；开始只加自己，结束只删自己；`commitInstall` 对在飞的技能直接返回。详情列的 `busy` 从「页内有任一在飞」改为「所选技能在飞」。
- Files: `gui/src/pages/marketplace/{readSide.ts,board.tsx,index.tsx,card.tsx}`、`gui/src/pages/marketplace/publisher/{readSide.ts,board.tsx,index.tsx}`、`gui/src/pages/marketplace/installBusy.test.{tsx,host.tsx}`。
- Self-check: `nsh test gui/src/pages/marketplace/installBusy.test.tsx`。写「转圈还在」的断言时注意：`IconSpin` 松开后会滑行到一圈结束才收起（最长 `TURN_MS` + `HOLD_MS`），紧跟在松开之后的 `app.shows("id:mk-spin-…")` 对已经被停掉的转圈同样为真，用例在回归下照样通过。先 `app.wait(TURN_MS + HOLD_MS)` 再断言，并用变异（把判定临时改回只认最后一个名字）确认用例会红。

## 2026-11-24 - `cargo run` 报 `No such file or directory`：huacnlee/cargo 的 `build.gc` 在 macOS 上把刚链接好的 `./target/debug/skillstar` 当孤儿删掉

- Symptom: `./dev.sh`（`git pull` + `cargo run -p ss-bin --bin skillstar --locked`）在 `Finished` 之后打印 `Cleaned 1 stale build unit, 244.2MiB freed (build.gc)`，随即 `could not execute process target/debug/skillstar (never executed)` / `No such file or directory (os error 2)`。构建本身全绿、不到 1s，`./target/debug/` 里只剩 `skillstar.d`。一小时内重跑正常，下一次 GC 放行（每小时至多一次）时又失败。
- Root cause: 本机的 `cargo` 是 `huacnlee/cargo` fork（`cargo -Vv` 带 `(huacnlee/cargo 0.103.0-1)`），产物本体在 `~/.cargo/targets/<包>-<hash>/debug/build/<crate>/<hash>/out/`，`./target` 里只是副本。macOS 上 cargo 用 `fs::copy`（`fclonefileat` 写时复制克隆，规避 Gatekeeper 竞态）放副本：独立 inode、`nlink == 1`、保留原始构建时刻的 mtime。fork 的 `collect_orphan_outputs` 靠 `nlink == 1` 判定「持有它的 build unit 已被回收」，再加上「mtime 早于 `build.gc.max-unused`（默认 1 天）且旁边有 `.d`」就删，并且跑在本次构建的 `link_targets` 之后、`output_depinfo` 之前。于是构建超过 1 天、本次无需重编的产物，会在构建末尾被删。同一判定覆盖 `./target/<profile>/` 顶层所有产物，`cargo build --release` 无需重编时其后的打包步骤同样拿不到产物（未单独实测）。
- Fix: 用户级 `~/.cargo/config.toml` 设 `[build.gc] max-unused = "3650 days"`，让 `./target` 里的产物不因年龄被判成孤儿；磁盘上限仍由 `max-size`（auto）/ `shared-max-size` / `projects-max-size` 兜底。不写进仓库的 `.cargo/config.toml`：官方 cargo 会把它报成 unused config key。上游 `main` 的 `collect_orphan_outputs` 与 0.103.0-1 相同，截至本日未修复。
- Files: `~/.cargo/config.toml`（机器配置，不在仓库内）。
- Self-check: `cargo -Vv` 带 `huacnlee/cargo` 时，确认 `~/.cargo/config.toml` 里有 `[build.gc] max-unused`。验证办法：临时加 `frequency = "always"` 强制每次构建都跑 GC，连跑两次 `cargo build -p ss-bin --bin skillstar --locked`，用 `stat -f '%l %Sm %N' target/debug/skillstar` 确认文件仍在，验证完删掉 `frequency` 行。`Finished` 后紧跟 `Cleaned N stale build unit(s) ... (build.gc)` 且可执行文件缺失时先查这里，不要去清 `~/.cargo/targets`。

## 2026-11-20 - 导入弹窗点击「继续在后台」触发 `potential infinite loop` 页面崩溃：共享看板命令型写方法自读自写泄漏订阅

- Symptom: 在「我的技能」导入弹窗等待超过 10s 后点击「继续在后台」（`parkImport()`），主内容区立即被 `<ErrorBoundary>` 接管并显示「页面渲染出现异常 / potential infinite loop: an effect keeps changing what it reads」，右上角角标同时残留「正在导入 / 正在准备仓库内容...」。
- Root cause:
  1. **命令型写方法未用 `untracked` 读自身信号**：`importModalState.ts` 的模块级 `effect` 在 `importParked()` 为真且处于扫描/安装阶段时同步调用 `putJob(...)`；而 `jobBoard.ts` 的 `putJob` / `endJob` / `dropJob` 先直接调用 `jobs()`（把 `jobs` 订阅进当前调用者 `effect`），随后立即调用 `jobs.set(...)` 写入新数组引用，导致该 `effect` 在同一响应式轮次内把自身标脏并无限重跑，触发 `nsh-js` runaway 保护抛错。
  2. **同类兄弟与收口细节**：`noticeBoard.ts`（`push`/`hold`/`release`/`showRestored`/`close`/`remove`/`dismissReplaceKey`）与 `chrome/dialog.tsx`（`openConfirm`/`close`/`closeConfirm`）也以同样方式在命令型写方法中直接读取自身状态信号；`jobBoard.ts` 的 `endJob` 在检查 `!job || job.done` 前清除了自动关闭定时器、`dropJob` 在目标不存在时仍构造新数组；`importModalState.ts` 的 `revealParkedImport()` 在 `effect` 内未用 `untracked` 读取 `importModalOpen()`，且 `phase === "completed"` 分支未复位 `importParked.set(false)`。
- Fix:
  - `gui/src/shell/jobBoard.ts`、`gui/src/shell/noticeBoard.ts`、`gui/src/chrome/dialog.tsx`：保留读接口追踪订阅语义，全部命令型写方法改用 `untracked(...)` 读取自身状态数组；`dropJob` / `close` / `closeConfirm` 仅在长度变化时写回；`endJob` 先检查 `!job || job.done` 再清定时器。
  - `gui/src/pages/my_skills/importModalState.ts`：`revealParkedImport()` 改用 `untracked(importModalOpen)`，`phase === "completed"` 分支补上 `importParked.set(false)`。
- Files: `gui/src/shell/{jobBoard.ts,noticeBoard.ts}`、`gui/src/chrome/dialog.tsx`、`gui/src/pages/my_skills/{importModalState.ts,importModalSlot.test.tsx}`、`docs/errors.md`。
- Self-check: `nsh check gui`、`nsh test gui/src/pages/my_skills/importModalSlot.test.tsx`、`nsh test gui/src/shell/desktopShell.test.tsx` 与 `nsh test gui`；所有在 `effect` 中可被调用的共享状态看板命令型写方法，读取自身状态信号时一律包裹 `untracked(...)`，仅对外暴露的读接口保留追踪订阅。

## 2026-11-19 - 账号后台自动刷新对瞬时错误刷屏霸占、单账号/全冷却停摆，且底层网络错误与系统代理变化被吞掉

- Symptom: 某个账号额度请求连续传输失败时，日志每隔约 50 秒重复打印同一账号的 `quota read failed; keeping the last snapshot` 并带上无底层细节的 `error sending request for url (...)`；其它健康账号长时间不被轮询；同时在单账号或全员冷却情况下调度器可能停止唤醒，且系统代理变更后复用连接池仍持续报错。
- Root cause:
  1. **瞬时错误固定 15s 冷却 + LRU 不记失败尝试时刻**：`autoRefresh.ts` 对 `transient` 错误固定冷却 `15_000ms` 且不更新尝试时间，而 `service.rs` 失败时保留旧快照 `fetched_at`；当步进 `Δt > 15s`（如默认 `300s / 6 ≈ 50s`）时，同一失败账号每次 tick 都已出冷却且时间戳最旧，连续霸占 LRU 队首并饿死其余账号。
  2. **调度器扣除在途账号与全冷却直接停摆**：`scheduleNext` 把 `inFlight` 账号从 `eligibleCount` 扣除并在 `eligibleCount === 0` 时调用 `clearTimer()`，导致多账号分母少算、单账号首 tick 即停摆，以及全员冷却后无人唤醒。
  3. **吞掉 `Error::source()` 链**：`usage/fetch/mod.rs`、`oauth/token.rs`、OAuth agents 与 `network_doctor.rs` 仅格式化 `reqwest::Error` 顶层字符串（`error sending request for url (...)`），丢失了超时、DNS、代理拒绝、TLS、连接重置等源错误链。
  4. **共享 HTTP 客户端过期系统代理与连接池无失效接口**：`SHARED_PROBE_CLIENT` 仅按 `ProxyConfig` 指纹缓存，系统代理在运行期切换或连接池异常后，传输失败从不失效重建客户端。
- Fix:
  - `gui/src/pages/accounts/autoRefresh.ts`：引入 `computeBackoffMs` 对 `transient`（15s 起）与 `rate_limited`（60s 起）统一指数退避至 10 分钟；`AutoRefreshSlot` 增加 `lastAttemptedAt`，`pickNextLruAccount` 按 `max(lastAttemptedAt, lastRefreshedAt) ?? fetchedAt` 轮转；`scheduleNext` 不再扣除在途账号，并在全员冷却时定于最早 `cooldownUntil` 唤醒。
  - `crates/ss-core/src/infra/http_client.rs`：新增 `invalidate_probe_http_client()` 与 `format_error_chain()`；`crates/ss-core/src/config/network_doctor.rs` 改用 `format_error_chain`。
  - `crates/ss-accounts/src/{usage/fetch/mod.rs,oauth/token.rs,oauth/agents/{antigravity,grok,kiro,zcode}.rs}`：传输失败时调用 `invalidate_probe_http_client()` 并输出完整错误链；`crates/ss-accounts/src/service.rs` 的警告日志补充 `agent` 字段。
- Files: `docs/features/accounts/README.md`、`docs/errors.md`、`crates/ss-core/src/infra/http_client.rs`、`crates/ss-core/src/config/network_doctor.rs`、`crates/ss-accounts/src/{service.rs,usage/fetch/mod.rs,oauth/token.rs,oauth/agents/{antigravity,grok,kiro,zcode}.rs}`、`gui/src/pages/accounts/{autoRefresh.ts,accounts.test.tsx}`。
- Self-check: `cargo test -p ss-core --locked -- http_client`、`cargo test -p ss-accounts --locked`、`nsh check gui`、`nsh test gui/src/pages/accounts/accounts.test.tsx`；调度器单测需验证瞬时错误指数退避、失败后让出 LRU 队首以及单账号/全冷却保活不清空定时器。

## 2026-11-18 - 侧栏折叠与深浅主题切换卡顿延迟：定向过渡节流、保活页二次布局级联与滚动条/菜单同步重建

- Symptom: 点击侧栏底栏「折叠/展开」（`sidebar-collapse`）与「切换背景样式」（`sidebar-theme`）按钮时出现明显延迟与掉帧（macOS 实测折叠约 550–590ms、主题切换约 260–370ms 且伴随多帧掉帧与两段跳变）。
- Root cause:
  1. **折叠触发方向切换动画 + `FRAME_BATCHES` 节流**：`ModeSwitcher`（`SliderSegmented`）在折叠时翻转 `vertical`（`translateX` ↔ `translateY`、`scaleX` ↔ `scaleY`）却仍挂着 `200ms`/`250ms` `transform` 过渡，使 `nsh-platform-macos` 进入 `defers_batches()`（每帧仅应用 `4ms` `FRAME_BATCHES` 队列），把后续布局批延后多帧；同时 `BrandRow` / `SkillsNavRow` / `AccountsProviderMenu` / `SegmentPill` 用 `<If>` 反复销毁并重建数十个 `<Text>` 节点，且默认 `skills` 模式下仍同步挂载并更新隐藏的 `AccountsProviderMenu`。
  2. **跨保活页 `onLayout` 二次布局级联**：折叠改变主面板宽 `124px` 后，`MySkills` / `Marketplace` / `SkillCards` / `Accounts` / `Settings` 全部保活页在下一帧收到 `onLayout` 写入各自 `paneWidth`，不仅迫使隐藏页同步重算与重排 `<List>`，还让激活页滞后一帧再重算列宽造成两段跳变。
  3. **主题切换重建滚动条与同步菜单/写盘阻塞**：`installScrollbarTheme()` 在 `effect` 内随主题调用 `Scrollbar.setTheme()`，每次切主题销毁并重建全部 `<Scroll>` / `<List>` 滚动条视图与动画节点；`desktopShell.ts` 的 `Menu.setApplicationMenu` 与 `Shell.tsx` 的 `Settings.loadPrefs/savePrefs` 在点击同一帧同步执行，`setThemeMode()` 也未对同值幂等去重。
- Fix:
  - `chrome/segmented.tsx` 仅在同方向切换选中项时启用 `motionTransition()` / `stretchTransition()`，方向翻转零帧落位；`Sidebar.tsx` / `skillsNav.tsx` / `pages/accounts/rail.tsx` / `chrome/segmented.tsx` 用 `display: "none"` 保活标签节点，并惰性挂载 `AccountsProviderMenu`。
  - `shell/nav.ts` + `Shell.tsx` 测量根宽导出同步 `shellPanelInnerWidth()` 与门控 `createPaneWidth()`，让激活页在首帧直接拿到新宽度、隐藏保活页完全跳过 `onLayout` 与 `<List>` 重算。
  - `services/themeMode.ts` 为 `setThemeMode()` 增加幂等守卫，并将 `installScrollbarTheme()` 改为启动时单次注册 `bg-muted-foreground/35`（跟随 `setColorScheme` 自动换色、不再销毁重建滚动条）；`desktopShell.ts` 与 `Shell.tsx` 将后续 `Menu.setApplicationMenu` 和 prefs 持久化合并延迟到下一事件循环。
- Files: `gui/src/chrome/segmented.tsx`、`gui/src/shell/{nav,desktopShell}.ts`、`gui/src/shell/{Shell,Sidebar,skillsNav}.tsx`、`gui/src/services/themeMode.ts`、`gui/src/pages/accounts/{rail,views,index}.tsx`、`gui/src/pages/my_skills/canvas.tsx`、`gui/src/pages/marketplace/{board,index}.tsx`、`gui/src/pages/skill_cards/index.tsx`、`gui/src/pages/settings/index.tsx`、`gui/App.test.tsx`。
- Self-check: 跑 `nsh check gui`、`nsh test gui/App.test.tsx` 与 `nsh test --trace` 检查 `press(sidebar-collapse)`（节点数零增减、无 `FRAME_BATCHES` 拖尾）与 `press(sidebar-theme)`（无 `Scrollbar` `Delete`/`Create`）；`SliderSegmented` 禁止在 `vertical` 翻转时触发位移/拉伸过渡，保活页 `paneWidth` 一律经 `createPaneWidth` 门控。

## 2026-11-18 - 账号页与托盘弹窗卡顿：无虚拟化全树重建、隐藏保活子树空转与高频子进程/读写锁

- Symptom: 账号数量较多（如 50–120 个）时，账号工作台勾选、单卡刷新、切换视图与滚动掉帧卡顿，托盘「全部刷新」甚至冻结主线程数秒，切回别的页面后后台自动刷新仍持续卡住主交互。
- Root cause:
  1. **`nsh` 视图抖动与缺虚拟化**：三态视图把全部账号挂在非虚拟化 `<Scroll>` + 内联 `{arr.length > 0 ? ... : ...}` / `{cond ? ... : null}` / IIFE / 裸 `.map()` 里——在 `nsh` 中，普通 JSX 三元表达式和 IIFE 编译为 `insert(parent, () => ...)`，任一被追踪信号更新（勾选、搜索、`refreshingIds` 变化、单卡刷新）哪怕条件真假未翻也会整块销毁并重建 `<Scroll>` 与整组卡片/图标节点；`Compact`/`List` 未加行级 `computed` 与 `Set` 查表，且列表状态切换或单行刷新后整表调用 `load()`/`reconcileLive()` 全量替换数组触发 `O(N²)` 重建。
  2. **隐藏保活子树与共享运行时空转**：`Shell` 按 `opacity` 保活的 `AccountsPage` 与 `hide()` 保活的 `<Window id="quota-popover" kind="popup">` 共享同一套 JS 响应式运行时与原生布局引擎；切走页面或收起弹出窗后，两个界面仍继续订阅 `sharedAccountsState`，伴随 `IconSpin` 提前注册计时器/`CADisplayLink`、`Tray.set` 订阅整份 `accounts()` 快照以及无冷却节流的切回/弹窗对账风暴，持续制造后台布局与 AppKit/Win32 重绘开销。
  3. **`ss-accounts` 与同步偏好读写阻塞**：`encryption_key()` 每次加解密重复派生密钥（冷路径触发 `machine_uid` 子进程 `ioreg`）；`reconcile` 按指纹对账与无效记录清理遍历解密全部 `records/*.json` 而未利用索引指纹；`vscdb` 只读探测走带 `CREATE TABLE` 的可写打开与长 `busy_timeout`；`AgentRefreshLock` 跨约 20s 额度 HTTP 请求持有，阻塞切号写入；JS 线程同步调 `Settings` facade 直接写盘 `gui_prefs.json`。
- Fix:
  - 工作台三态视图全部迁入 `<List>` 虚拟化（`AccountGridView`/`AccountCompactView` 按视口列宽打包行，`AccountListView` 按 `id` 键控）与 `<If>` + 键控 `<Each>`，行内用 `computed` + `ReadonlySet` 把单卡/勾选更新收敛到单行；`state.ts` 改为原位 DTO 补丁与分批合并刷新，去掉冗余整表 `load()`。
  - `AccountsWorkbench` 用 `pageActive()`、`QuotaPopoverPanel` 用 `active` 在隐藏时冻结后台订阅与 `IconSpin`（`IconSpin` 延后到首次 `spin === true` 才创建计时器门控），`Tray.set` 改仅追踪布尔 `hasAccounts`，并给弹窗/恢复同步加冷却节流。
  - `ss-accounts` 缓存 `encryption_key()`、索引 `IndexEntry` 写入非敏感令牌指纹供对账命中免解密、并行化 `reconcile`、`vscdb::open_read_only` 免建表只读打开、`AgentRefreshLock` 收窄到令牌刷新与落盘阶段、共用 Tokio runtime；`Settings` 偏好写入增加缓存并转到后台队列线程持久化。
- Files: `gui/src/pages/accounts/{index,views,card,meters,toolbar,selectionBar,rail,quotaPopover,quotaPopoverParts,quotaPopoverMeters,quotaPopoverAlternates}.tsx`、`gui/src/pages/accounts/{state,addFlow,autoRefresh,logic,quotaPopoverLogic}.ts`、`gui/src/chrome/feedback.tsx`、`gui/src/shell/{residentWindow,desktopShell,trayQuotaWindow}.ts*`、`gui/native/src/facades/settings.rs`、`crates/ss-accounts/src/{crypto,store,reconcile,service}.rs`、`crates/ss-accounts/src/oauth/rt.rs`、`crates/ss-accounts/src/switch/vscdb.rs`。
- Self-check: 跑 `nsh test --trace gui/src/pages/accounts/accounts.test.tsx` 与 `quotaPopover.test.tsx` 关注 `ops`/`mount items`，列表页组件内禁止用裸 JSX 三元或 `.map()` 包裹重组件子树（改用 `<If>` + `<List>`/`<Each>`）；保活页与隐藏 `<Window>` 必须对共享后台状态做活跃态门控；`ss-accounts` 读路径禁止批量解密所有记录或跨网络 I/O 持写锁，跑 `cargo test -p ss-accounts --locked`。

## 2026-11-18 - 其余页面与公共壳/chrome 首屏膨胀、折叠/切页重建与 JS 线程阻塞

- Symptom: 启动时首屏即挂载全部页面并发起多个桥接加载；侧栏折叠、切页、分段切换、卡片操作及设置/MCP/项目页大列表仍触发整子树重建与明显卡顿；`Pulse` / `IconSweep` / `Alert` / `DetailFooter` 在切换主题或数据更新时出现颜色冻结或数值不跟随；空闲状态下后台仍有常驻计时器唤醒。
- Root cause:
  1. **启动全量挂载与首帧零宽陷阱**：`Shell.tsx` 启动时一次性渲染全部 8 个保活页面并触发 `Marketplace` / `Projects` / `Settings` 初始读盘；`SettingsColumn`（`settings/index.tsx`）按 `paneWidth() < SETTINGS_NAV_MIN_PANE` 懒挂分区时未加 `paneWidth() > 0` 守卫，导致 `onLayout` 前初始 `0` 宽被判为窄屏而首帧全挂 6 个分区；跨页事件总线在非激活页也直接触发重载与双刷新。
  2. **共用控件重建、无虚拟化与海量预置 `<Tip>`**：`SliderSegmented`、`SkillsNav`、`Sidebar`（`BrandRow`/`ModeSwitcher`/`SidebarFooter`）及 `AgentRail` 在 `.map()` 或 `<If>` 内读取动态状态或直接传值，导致折叠、切页、切 tab 或单槽切换时整组销毁重建；`AgentRail` 与工具栏为每个槽位预建未显示的 `<Tip>` 节点；`MCP` 列表/商店/发现条、`Projects` 左栏、`Translation` 模型菜单及 `Settings` 83 个智能体行缺少 `<List>` 虚拟化或行级 `computed` 收窄；`translation.ts` 的 `t()` 每次调用做 `.split(".")` 遍历。
  3. **空转计时器与组件体一次性求值冻结**：`JobBand` 无任务时仍常驻 1s `setInterval`；`NoticeBand` 对非撤销通知仍常驻 100ms 轮询；`Pulse`、`IconSweep`、`Alert`、`ConfirmCard` 与 `projects/detail.tsx` `DetailFooter` 在组件体顶层一次性调用 `paletteToken()`/`toneSurface()`/计数，在 `nsh` 单次执行模型下导致切主题或变更后滞留旧值。
  4. **JS 线程同步原生阻塞与全目录扫描级联**：`settings_storage::about_snapshot` 同步拉起 `git`/`gh` 子进程；`AgentRegistry` 在轻查询路径反复扫描 83 个目录且未并行化；`Marketplace` 重复初始化运行时并阻塞异步线程；`ss-mcp-hub` 每次变更重扫 36 个智能体与发现目录；`ss-core::translation` 每次读密钥与写配置同步碰盘；`settings*` 配置读写缺少文件戳缓存与后台写队列。
- Fix:
  - `Shell.tsx` 改为按 `mountedPages` 首次访问懒挂载并保活，把轻量 `Settings.loadPrefs()` 播种与未访问设置页前的主题持久化前移至壳层；各页接入 `pageActive()` 门控、`refreshDirty` 延后重刷与原位增量合并；`SettingsColumn` 补充 `paneWidth() > 0` 守卫，仅首帧挂载当前分区。
  - `Sidebar.tsx` 用 `flexDirection` 翻转与单一 `SliderSegmented`/`BrandMark` 消除折叠重建；`skillsNav.tsx` 与 `chrome/segmented.tsx` 把动态读移入行内并走键控 `<Each>`；`AgentRail`、`toolbar.tsx` 与 `barMenu.tsx` 改为悬停按需挂载 `<Tip>`；`repoAvatar.tsx` 用 `<If>{(uri) => ...}</If>` 去重解析；`mcp/`、`projects/`、`settings/sections/translation.tsx` 补齐 `<List>` 虚拟化与按行 `computed`/`ReadonlySet`；`services/translation.ts` 预展平双语字典为 O(1) `Map`。
  - `jobBand.tsx` 仅在存在进行中任务时开启 1s 计时并用 `<Each key={(j) => j.id}>`；`notify.tsx` 对定长通知改用单次 `setTimeout`，仅非钉住撤销条跑 100ms 倒计时；将 `Pulse`、`IconSweep`、`Alert`、`ConfirmCard` 与 `DetailFooter` 的颜色/状态求值移入 JSX/访问器函数。
  - Rust 侧：`crates/ss-skills` 新增零磁盘扫描的 `list_profile_specs()` 并并行化 `into_profiles()` / `intake::scan()`；`crates/ss-app/src/storage_maintenance.rs` 合并单次遍历并并行扫描；`gui/native/src/facades/{marketplace,marketplace_publisher,projects,settings_agent_connections,settings_storage,my_skills*,skill_cards*}.rs` 将阻塞盘/进程/网络工作移入 `blocking(...)` 并并发探测；`crates/ss-mcp-hub` 增加 `LibraryDelta` 增量返回；`crates/ss-core/src/translation` 与 `settings{,_network,_updates}.rs` 增加缓存与合并后台写线程。
- Files: `gui/src/shell/{Shell,Sidebar,skillsNav,notify,jobBand}.tsx`、`gui/src/shell/jobBoard.ts`、`gui/src/chrome/{segmented,agentRail,repoAvatar,toolbar,barMenu,feedback,alerts,dialog}.tsx`、`gui/src/services/{translation,themeMode}.ts`、`gui/src/pages/{my_skills,marketplace,skill_cards,projects,mcp,settings}/**`、`gui/native/src/facades/{my_skills*,skill_cards*,marketplace*,mcp_hub,projects,settings*,translation}.rs`、`crates/ss-skills/src/{agents,projects,local_skill/intake.rs}`、`crates/ss-mcp-hub/src/**`、`crates/ss-core/src/translation/**`、`crates/ss-app/src/storage_maintenance*.rs`。
- Self-check: 跑 `nsh check gui`、`cargo test --workspace --locked`、`nsh test gui` 与 `bash scripts/internal/check_file_size.sh`；新增页面或列表遵循 `Shell` 懒挂载 + `pageActive()` 门控 + `<List>`/`<Each>`/`<If>`，禁止在组件体顶层求值 `paletteToken()` 或把依赖 `onLayout` 的响应式阈值直接写成 `paneWidth() < THRESHOLD`（必须加 `paneWidth() > 0`），facade 层禁止在 JS 同步函数或 Tokio worker 中直接执行目录全扫、子进程探测或同步写盘。

## 2026-11-17 - 状态栏 / 托盘额度弹出窗每次点击有约 250ms 明显延迟

- Symptom: 点击 macOS 顶部菜单栏图标时，额度弹出窗需要约 250ms 才显现，交互有明显卡顿感。
- Root cause: ① `<Window id="quota-popover">` 包裹在 `<If when={trayPopoverOpen()}>` 内且开启了 `closeOnBlur` / `closeOnEscape`，每次收起都会彻底销毁原生 `NshPanel` 与已解码的 `<AgentIcon>` 纹理；下次点击触发 `nsh-platform-macos` 的 `open_window`，被首帧等待门禁（`image::wait_for_frame` + `surface::after(250ms, first_frame)`）阻塞显示；② 点击回调在 `focus()` 之前同步发起了 `reconcileLive()` 与 `refreshStale()`，且列表未使用 `<Each>` 按 `agentId` 保活，导致展开同一帧内状态抖动重建全部卡片节点。
- Fix: 启动后静默预热 `<Window id="quota-popover">` 并立即 `hide()`（将窗口状态切为 `Hidden`，使 250ms `first_frame` 不自动置顶抢焦点），收起时走 `WindowRef.hide()` 保活而非销毁窗口（配合 180ms 失焦防抖避免点击状态栏图标自身收起时重开）；点击时先同步 `setBounds` + `focus()` 零延迟上屏，下一帧（`setTimeout 16ms`）再异步对账与补刷，面板列表改用 `<Each key={(g) => g.agentId}>` 原地保活。
- Files: `gui/src/shell/{residentWindow.ts,trayQuotaWindow.tsx}`、`gui/src/pages/accounts/quotaPopover.tsx`。
- Self-check: 高频呼出型辅助 `<Window kind="popup">` 应采用 `hide()` / `focus()` 保活复用，禁止用 `<If>` 或 `closeOnBlur: true` 反复销毁重建原生窗口。

## 2026-11-17 - 展开额度菜单却把主窗口一并带到前台：`focus()` 让 AppKit 为了给面板键盘而激活整个 App

- Symptom: 在别的 App 里点菜单栏图标（或按 ⌥⇧Q）展开额度面板，SkillStar 的主窗口（本来只是压在别的 App 后面）跟着跳到最前面，菜单栏也变成 SkillStar 的；用户要的是「只展开菜单」。
- Root cause: 面板用 `WindowRef.focus()` 呈现。`focus()` 即使窗口属性是 `activate: false`（AppKit 侧因此带 `NSNonactivatingPanel` 掩码）仍会 `makeKeyAndOrderFront`；应用不在最前时，AppKit 为了让这个窗口成为 key window 会激活整个应用，激活就把该应用所有可见窗口（主窗口）带到其它 App 前面。nsh 0.5.1 没有「成为 key 但不激活应用」的 API，`show()` 才是「orderFrontRegardless、不激活、不抢键盘」的那条路。
- Fix: `presentTrayQuotaPopover()` 按「应用是否已在前台」分岔——`focusedWindow() !== null`（已在最前）用 `focus()`，否则只 `show()`；同时在 `installDesktopFeatures` 的 lifecycle 回调里补一次「回到 SkillStar 就收起面板」，因为非激活呈现时收不到失焦。
- Files: `gui/src/shell/{residentWindow.ts,desktopShell.ts}`。
- Self-check: 真机上在别的 App 里按 ⌥⇧Q，同时用 `for i in $(seq 1 300); do n=$(lsappinfo info -only name "$(lsappinfo front)" | head -1); printf '%s %s\n' "$(date +%H:%M:%S)" "$n" >> /tmp/front.log; sleep 0.1; done` 采样最前面的 App：面板展开期间最前面的 App 必须仍然是原来那个（改动前会变成 skillstar）。无头测试驱动不了窗口激活，这条只能真机验。
- 后续：由于后台 `show()` 非激活呈现（以及 macOS `NonactivatingPanel` 样式）在点击其它 App、桌面或菜单栏空白处时不会触发 `onFocusChange({ focused: false })`，外点收起由 `gui/native/src/popover_dismiss.rs` 在 `trayPopoverOpen()` 为 `true` 期间挂载 macOS 全局与本地 `NSEvent` 鼠标按下监听处理（经 `Settings.wait_popover_outside_click` / `cancel_popover_outside_click` 接入 `TrayQuotaWindow`）。不要将 macOS 后台唤起改回 `focus()`，也不要移除该外点监听。

## 2026-11-17 - 额度下拉窗的卡片列表被裁掉半张卡、滚轮无处可滚：滚动容器按组数阈值挂载

- Symptom: 状态栏 / 侧栏的全局额度下拉窗里最后一张卡片在底栏上方被切断（卡片尾部的内边距与描边看不见），指针悬停在卡片上滚轮与触控板都没有反应；换一组账号（更多 Agent）后同样的面板却能滚。
- Root cause: 内容区用 `groups().length > 3 || 展开备选` 决定挂不挂 `<Scroll>`，其余情况退回裸 `<View>`。组数少但单卡窗口多（例如 Antigravity 一张卡里四个模型组窗口）时内容仍高于视口，外框 `overflow: "hidden"` 直接裁切，没有滚动容器，滚轮和触控板都无处可滚。可滚与否只能由「内容高度 vs 视口高度」决定，不能由数据组数近似。
- Fix: 内容区一律挂 `<Scroll>`；视口框写明确上限 `QUOTA_POPOVER_BODY_MAX_H = 392`（短内容仍收缩到内容高度，弹出窗不留空白），面板上限改为常量推导 `QUOTA_POPOVER_MAX_H = 40 + 392 + 34 + 2 = 468`，顶栏 / 底栏高度也用同一组常量，避免几何漂移。回归测试用「单组」「五组长卡片」两种样本断言滚动容器存在、面板贴住上限、列表能拖动。
- Files: `gui/src/pages/accounts/{quotaPopover.tsx,quotaPopoverLogic.ts,quotaPopover.test.tsx,quotaPopover.test.host.tsx}`。
- Self-check: `nsh test gui/src/pages/accounts/quotaPopover.test.tsx`（`popover body is a scroll box for every group count and never clips the card list` 去掉无条件 `<Scroll>` 会以「单组也必须有滚动容器」失败）。判断一个限高列表能不能滚，只看它有没有滚动容器与真实滚动范围，不要用组数 / 行数阈值近似。

## 2026-11-17 - 被 SIGTERM 结束的 Antigravity 2.32.1 之后 `open -a` 拉不起来

- Symptom: 用 `kill -TERM` 让 Antigravity 退出（它自己打印 `Got signal terminated, shutting down`，退出是干净的），随后 `open -a /Applications/Antigravity.app` 与 `open -b com.google.antigravity` 都返回 0，但客户端要么完全不出现，要么由 launchd 拉起约 20ms 后 `termination reported by launchd (0, 0, 0)` 结束；`~/Library/Logs/Antigravity/main.log` 没有新的启动横幅（死在打日志之前），无崩溃报告。
- Root cause: 未定论。候选：launchd 对同一 bundle 短时间反复拉起的节流、或 sidecar 关闭时序留下的启动前状态。已排除：代码签名与公证（`codesign --verify` / `spctl` 均通过）、自动更新（`Up to date (2.32.1)`）、端口占用（62420 已释放）、`SingletonLock` 残留（没有）、`DevToolsActivePort`（删掉后仍不行）。
- Fix: 这条故障留下的规则是**不要**用 `kill -TERM` / `kill -KILL` 结束 Antigravity；需要重启时由用户用客户端自己的退出（macOS ⌘Q、Windows 关窗口）再打开。切号本身已不再自动重启客户端，重启通路整体删除（[D-121](./decisions.md) 撤销 D-120）。
- Files: 无（`crates/ss-accounts/src/switch/antigravity_client.rs` 已按 D-121 删除）。
- 后续：真机上一次「AppleScript 优雅退出 → 拉起 → 连续存活 ≥2 秒」曾验证通过，但该通路已移除。手工排查「客户端起不来」的顺序：`pgrep -x Antigravity` → `tail ~/Library/Logs/Antigravity/main.log` → `log show --predicate 'process == "launchd" OR process == "runningboardd"'` 找 `termination reported by launchd`；受限宿主里 `open -a` 返回 0 却不启动时，`osascript -e 'tell application "Finder" to open POSIX file "<bundle>"'` 可以救回来（同样的道理：别先杀进程）。

## 2026-11-17 - Antigravity 2.x 不建 state.vscdb，切号被误判成「客户端没登录过」

- Symptom: 账号页点「切换」立刻失败，红色提示 `client credential unavailable: Antigravity's state.vscdb is not at …/Antigravity IDE/User/globalStorage/state.vscdb: start Antigravity once, sign in, then switch`，而客户端明明已登录、额度也在正常刷新。
- Root cause: Antigravity 桌面端 2.x 把用户数据目录换成 `~/Library/Application Support/Antigravity`，会话只放 macOS 钥匙串 `gemini/antigravity`（sidecar `language_server` 用 go-keyring，回退 `jetski-standalone-oauth-token`），既不创建也不读旧版 IDE 的 `state.vscdb`。切号写入层却把「vscdb 文件存在」当成客户端已登录的前置条件，于是在唯一权威落点（钥匙串）就在眼前时直接拒绝，一处都没写。
- Fix: `switch/antigravity.rs` 把 vscdb 投影改成「文件在场才写」，缺它不再失败；提交校验改成钥匙串回读同一 refresh_token（vscdb 在场时两处都校验）。钥匙串载荷按客户端原载荷合并，保留 `writer_id` / `token.expires_in` 这类客户端自有字段，只覆盖 token 与 `id_token_email`。macOS 非沙箱之外（Windows/Linux、沙箱）仍要求旧版 `state.vscdb`，因为那些平台的系统凭据通路尚未实现。
- Files: `crates/ss-accounts/src/switch/{antigravity.rs,antigravity_credential.rs,antigravity_tests.rs}`。
- Self-check: `cargo test -p ss-accounts --locked -- switch::antigravity`（`a_client_without_a_state_database_switches_through_the_keychain` 与 `a_switch_keeps_the_fields_the_client_put_in_the_keychain` 任一改动回退都会失败）。任何「客户端没登录过」类的硬前置，先核对客户端当前版本是否还写那个文件，不要只按旧版布局推断。
- 后续：客户端只在启动时读凭据（`compositeTokenStorage` 从钥匙串加载一次），运行中改写钥匙串不会热加载，它自己刷新令牌时还会写回旧会话。切号后需要用户手动重启客户端；应用不再代劳重启（[D-121](./decisions.md) 撤销了 D-120 的自动重启）。

## 2026-11-17 - 账号页自动刷新写成浏览器式存储：`nsh check gui` 13 条类型错，`nsh build` 却是绿的

- Symptom: `nsh check gui` 报 13 条 `Cannot find name 'localStorage'` / `Cannot find name 'document'`，全部集中在一个新页面文件；同一份代码 `nsh build` 成功、`cargo run` 也能起来，容易误判成「构建没问题」。
- Root cause: nsh 的 JS 环境没有 DOM。`gui/.nsh/tsconfig.json` 的 `lib` 只有 `ES2023` + `ESNext.Disposable`，`globals.d.ts` 开头写明「there is no DOM」。按浏览器习惯用 `localStorage` 存偏好、用 `document.visibilityState` 暂停轮询，在类型层根本不成立。nsh 给的全局是 `setTimeout` / `setInterval` / `queueMicrotask` / `requestAnimationFrame` / `atob` / `btoa` 这些（`window` 是 nsh 自己的桌面窗口 API，不是 DOM 的 `window`）。
- Fix: 跨重启的偏好落 `~/.skillstar/config/gui_prefs.json`（`gui/native/src/facades/settings.rs` 的 `Settings` facade，页面侧接缝 `gui/src/pages/accounts/prefs.ts`）；窗口可见性没有对应 API，也不需要——关闭主窗口只是隐藏，进程与后台唤醒继续（[D-115](./decisions.md)）。
- Files: `gui/src/pages/accounts/autoRefresh.ts`、`gui/src/pages/accounts/prefs.ts`、`gui/native/src/facades/settings.rs`。
- Self-check: 代码里不应出现 `localStorage` 或 `document.`，只有注释可以提到它们（`autoRefresh.ts` / `prefs.ts` 头部的说明就是注释）。**`nsh build` 不跑类型检查**，判断 TS 是否成立只认 `nsh check gui`；两者的分歧本身就是这条故障的信号。

## 2026-11-17 - 卡组 rail 点亮后技能页的 Agent 图标仍全灰

- Symptom: 卡组卡片底栏点亮某个 Agent（成员技能已落盘链接，CLI `skillstar list --json` 能读到 `agents`），切回「技能」页同几张卡的 Agent 图标仍是灰的；只有重启应用才恢复高亮。
- Root cause: Shell 每页只挂载一次（keep-alive），技能页的 `agentLinks` 只在自身 `refresh()` 时重读；卡组卡片的 rail 点击只刷新了卡组页，没有发 `groupsChanged`，技能页于是继续画启动时的快照。卡组详情列的链接任务一直有发这个频道，卡片底栏这条路径漏了。
- Fix: 卡组卡片 rail 任务的收尾（全部成功、部分失败回滚、异常三条路径都走 `finally`）统一发 `groupsChanged`；技能页与卡组页都订阅该频道并重载自己的快照。
- Files: `gui/src/pages/skill_cards/index.tsx`、`gui/src/shell/events.ts`。
- Self-check: `nsh test gui/src/pages/skill_cards/deckRailSync.test.tsx`（去掉这次 publish 会以超时失败）。任何跨页写入后另一页画面不跟随，先查这次写入有没有在 `shell/events.ts` 的频道上通知对方重载，而不是指望切页重挂载。

## 2026-11-16 - 仓库已扫描完成但导入框一直转圈

- Symptom: 仓库扫描或技能安装完成后，导入框仍停在进度界面，例如 `citrolabs/ego-lite` 无法进入技能选择。
- Root cause: 进度泵等待通道断开，却通过取消用的 facade 克隆持有通道发送端，工作线程结束后通道仍保持打开。
- Fix: 只有工作线程持有发送端的强引用；进度 sink 持有弱引用，让工作结束时进度泵退出，同时保留取消句柄。
- Files: `gui/native/src/facades/my_skills_import.rs`。
- Self-check: `cargo test -p ss-gui-bridge --locked progress_channel_closes_after_worker_finishes`；在 nsh 应用中扫描仓库并确认进入技能选择。

## 2026-11-16 - Antigravity 账号刷新缺少 OAuth client secret

- Symptom: 未手工配置 OAuth 客户端时，Antigravity 账号令牌到期后刷新报缺少 client secret。
- Root cause: 默认配置只有客户端 ID 和 secret 占位串，登录和换票都无法使用。
- Fix: 内置公开的桌面客户端配置，自定义客户端 ID 仍需提供对应 secret；保留现有配置覆盖。
- Files: `crates/ss-accounts/src/oauth/agents/antigravity_config.rs`。
- Self-check: `cargo test -p ss-accounts --locked antigravity`。

## 2026-11-16 - Cursor 本地导入拿到 `cursor-safe-storage:v1:` 密文，导致无法解析 user_id 与额度

- Symptom: Cursor 本机导入后卡面显示「当前」和邮箱，但一直提示「暂无额度数据」且缺少套餐档位。
- Root cause: Cursor 3.29+ 默认开启 `cursor.storage.authTokenEncryptionAtRest = true`，将 `state.vscdb` 中的 `cursorAuth/accessToken` 与 `cursorAuth/refreshToken` 写为 `cursor-safe-storage:v1:<base64>` 密文，而镜像键 `cursor.accessToken` 仍为明文 JWT。直接读取密文入库会导致后续解析 JWT `sub` 失败（`Unreadable`），本地导入容忍该错误并把密文存盘。
- Fix: `switch/cursor.rs` 识别 `cursor-safe-storage:v1:` 前缀并回退读取 `cursor.accessToken` 明文镜像、忽略加密 `refreshToken`、禁止将密文写回或粘贴入库；`service.rs` 在 `refresh_with` 前从同身份本机 `state.vscdb` 自动自愈已存密文记录；`usage/fetch/cursor.rs` 对密文直接报 `AuthRequired` 并在解析 Ultra `breakdown.bonus` 时采用 `totalPercentUsed`。
- Files: `crates/ss-accounts/src/{switch/cursor.rs,token_import.rs,service.rs,usage/fetch/cursor.rs}`。
- Self-check: `cargo test -p ss-accounts --locked`；任何从 Electron `state.vscdb` 读凭据的适配层，必须检查字段前缀是否为客户端安全存储密文，禁止未经校验直接把字符串当 JWT/token 落库。

## 2026-11-16 - nsh 横向 Flex 行内文本不省略并重叠穿框：默认 flexShrink 为 0

- Symptom: 账号卡配额长标签（如 `Gemini Models · Weekly Limit`）压盖右侧百分比并冲出右列及卡片外框。
- Root cause: `nsh` 采用 React Native 默认 `flexShrink: 0`，`flexDirection: "row"` 下的 `<Text>` 未设 `flexShrink: 1, minWidth: 0` 时保持固有全宽，导致 `numberOfLines: 1` + `textOverflow: "ellipsis"` 不触发。
- Fix: 横向行内可省略 `<Text>` 一律配齐 `flexShrink: 1, minWidth: 0`，右侧数值配 `flexShrink: 0`，卡片与内框加 `overflow: "hidden"`；超长模型组窗口自动退回单列通栏。
- Files: `gui/src/pages/accounts/{logic,meters,card,views}.tsx`。
- Self-check: `nsh test gui`；任何横向行内本应截断却撑破/压盖右侧兄弟节点的 `<Text>`，优先排查是否漏设 `flexShrink: 1, minWidth: 0` 与外框 `overflow: "hidden"`。

## 2026-11-16 - SKILL.md 阅读器在浅色主题下白底白字：排版表是模块级常量，冻结了深色调色板

- Symptom: 切换到「浅白」（`paper`）主题后打开「查看 SKILL.md…」，正文与代码块仍停留深色值（几乎看不见字），而同卡片标题与标签颜色正常切换。
- Root cause: `paletteToken()` 在**模块导入时刻**求值时只会读到启动主题；把颜色存入模块级常量对象后，切主题重跑 JSX 表达式不会让常量对象重新求值。
- Fix: 将样式对象改为渲染期调用的函数（`readerTypography()` / `translationBase()`）并在 JSX 表达式中读取；`setThemeMode()` 同步调用 `setColorScheme()` 更新引擎调色板。
- Files: `gui/src/pages/my_skills/{readerStyle.ts,skillReaderSlot.tsx}`、`gui/src/services/themeMode.ts`。
- Self-check: `nsh test gui`；任意部件切主题不跟随、必须重启才生效时，优先排查是否把 `paletteToken()` 缓存在了模块顶层常量或组件体顶部一次性求值变量中。

## 2026-11-16 - 来源筛选浮层的仓库名单滚轮滚不动：视口框写了高度却没挂 `<Scroll>`

- Symptom: 浮层列表超过视口高度时底部被截断，指针悬停滚轮无响应。
- Root cause: 外框设置了固定高度与 `overflow: "hidden"`，内部却使用裸 `<View>`，内容溢出即被裁切而无滚动容器。
- Fix: 视口框超过阈值时用 `<Scroll style={{ width: "100%", height: "100%" }}>` 包装内容（少于阈值不挂 `<Scroll>`，高度自适应）。
- Files: `gui/src/pages/my_skills/originMenu.tsx`。
- Self-check: `nsh test gui`；注意 `app.texts()` 不做滚动裁切，断言列表可滚动与底项可点击以实际交互为准。

## 2026-11-15 - nsh 顶栏空白拖不动窗口：windowDrag 只沿父链查找，兄弟拖拽层被遮挡

- Symptom: 隐藏原生标题栏后，顶栏留白、侧栏边缘或工具栏控件间隙拖不动窗口。
- Root cause: 平台拾取命中指针下方最内层可响应节点，并只沿 `parent_view()` 向上查找 `windowDrag`；绝对定位兄弟层会被子面板、外框或全宽横幅遮挡。
- Fix: 将 `desktop={{ windowDrag: true }}` 放在顶栏与侧栏的祖先外框上（`Shell` / `Sidebar` / `PageBar` / `AccountsToolbar`）；覆盖层设 `pointerEvents: "box-none"`；工具栏交互控件容器与无 `onPress` 的禁用控件显式标记 `desktop={{ windowDrag: false }}`。
- Files: `gui/src/shell/{Shell,Sidebar,notify}.tsx`、`gui/src/chrome/{toolbar,segmented}.tsx`、`gui/src/pages/{my_skills,marketplace,accounts}/`。
- Self-check: `nsh test gui`；编译运行二进制后实测拖动顶部边距、侧栏顶部与工具栏间隙。

## 2026-11-15 - 内嵌宿主漏传 nsh.json window 配置：顶部多出原生系统标题栏

- Symptom: 启动二进制时顶部出现一条额外系统标题栏，下方侧栏又空出红绿灯预留高度，形成双重占位。
- Root cause: `nsh build` 不把 `window` 配置写进 `gen/bundle.js`；宿主手工构建 `Bundle` 若漏设 `bundle.main_window`，平台后端按默认标题栏建窗。
- Fix: `gui/host/src/lib.rs` 内嵌 `gui/nsh.json`，通过 `MainWindowOptions::from_json` 注入 `bundle.main_window` 与窗口标题。
- Files: `gui/host/src/lib.rs`、`gui/nsh.json`、`gui/src/shell/Sidebar.tsx`。
- Self-check: `cargo test -p ss-gui-host --locked`。

## 2026-11-15 - 内嵌 bundle 过期：启动即报 Could not find export

- Symptom: TSX 与 `gui/native` 导出变更后，`cargo build/run` 仍内嵌旧 `bundle.js`，启动即报导出缺失。
- Root cause: `host/build.rs` 仅在产物缺失时重建，不会因源文件修改触发重新打包。
- Fix: `gui/host/build.rs` 对 `App.tsx`、`nsh.json`、`theme.json`、`src/`、`native/` 逐一注册 `rerun-if-changed` 并比对 mtime，过期即自动执行 `nsh build`。
- Files: `gui/host/build.rs`、`gui/host/src/lib.rs`。
- Self-check: 修改 `gui/src/` 后执行 `cargo check/build --bin skillstar`，确认自动触发重建并更新 `gui/host/gen/bundle.js` mtime。

## 2026-11-15 - `-no_compact_unwind` 让 panic 无法展开，dev/test 的 panic 隔离静默失效

- Symptom: 在 `.cargo/config.toml` 全局配置 `-Wl,-no_compact_unwind` 消除 `__eh_frame section too large` 告警后，测试进程出现 `fatal runtime error: failed to initiate panic, error 5`（`SIGABRT`），`catch_unwind` 与阻塞线程池 panic 恢复静默失效。
- Root cause: ld64 关闭 compact unwind 后，缺少完整 DWARF FDE 的线程栈帧无法完成 `_Unwind_RaiseException` 第一阶段查找。
- Fix: 移除工作区全局链接参数，仅在根包 `build.rs` 用 `cargo:rustc-link-arg-bin=skillstar=-Wl,-no_compact_unwind` 定向作用于产品主二进制；各测试二进制保留正常栈展开。
- Files: `.cargo/config.toml`、`build.rs`、`Cargo.toml`。
- Self-check: 默认参数跑 `cargo test --workspace --locked`；任何测试因 `error 5` 无法 panic 时先检查 `.cargo/config.toml` 是否被加入了全局链接标志。

## 2026-10-15 - 全部取消链接后技能卡仍显示已链接

- Symptom: 批量取消部署后卡片 Agent 图标仍保持高亮，刷新才消失。
- Root cause: `remove_skill_from_all_agents` 删除链接后未主动失效技能列表缓存，刷新读到旧 `agent_links` 快照。
- Fix: 每次删除循环结束后（包含部分成功后失败分支）强制清列表缓存，且按 `targets_canonical_root` 跳过规范根及外来目录。
- Files: `crates/ss-skills/src/deployment/mod.rs`。
- Self-check: `cargo test -p ss-skills --lib --locked remove_skill_from_all_agents_drops_the_skill_list_cache`。

## 2026-10-14 - 易失数据目录把 CLI 凭据软链进 /tmp，清空即悬空

- Symptom: 当开发/演示进程把 `SKILLSTAR_DATA_DIR` 设在 `/tmp` 时，写入或切换的全局文件路径落入易失目录，`/tmp` 清理后外部链接悬空。
- Root cause: 未在写入宿主目录链接前校验数据根是否为持久目录；另外 macOS 下 `/var` 是 `/private/var` 的软链，未规范化路径前缀会导致临时目录判定失配。
- Fix: 提供两侧 `canonicalize` 的 `data_dir_is_volatile()` 校验；数据目录在易失位置且未设置 `SKILLSTAR_TOOL_SYNC_HOME` 测试沙箱时拒绝接管真实 `$HOME` 下的外部状态。
- Files: `crates/ss-core/src/infra/paths.rs`。
- Self-check: `cargo test -p ss-core --locked data_dir_volatility`；所有临时试验和集成测试应优先使用非 `/tmp` 的持久目录或同时隔离 HOME。

## 2026-10-12 - rmcp 3.5.1 让 project-skills MCP 协议测试整进程 SIGABRT

- Symptom: 升级 `rmcp` 到 3.5.x 后，stdio MCP 测试在 `server_capabilities_omit_roots` 触发整进程 `SIGABRT`。
- Root cause: `rmcp` 3.5.x serve 路径在协议测试接管/关闭 stderr 时触发 EIO 导致 panic 无法输出而 abort；降回 `3.4.0` 后稳定通过。
- Fix: 根 `Cargo.toml` 显式锁定 `rmcp = "=3.4.0"`。
- Files: `Cargo.toml`、`Cargo.lock`、`crates/ss-mcp/`。
- Self-check: `cargo test -p ss-mcp --locked -- --test-threads=1`。
- 2026-11-25 复测（为 cargo-deny 的 RUSTSEC-2026-0457/0461/0462 找修复版本）：`rmcp` 3.11.0 上不再中止，但 `cargo test -p ss-mcp --locked` 连跑 5 次都是 46 过 4 败——elicitation 三条加 `server_capabilities_omit_roots`，elicitation 请求被以 `-32600: server-to-client requests are not allowed on protocol 2026-07-28 or later` 拒绝。钉子因此保留；解钉的前提是审批流不再依赖服务端到客户端请求，而不是只改测试。

## 2026-10-09 - Antigravity 镜像目录旧链接未随规范根迁移，持续告警漂移

- Symptom: 存储从 `~/.agents/skills` 迁入 `~/.skillstar/data/skills/installed`（D-100）后，Antigravity 镜像目录下的旧断链被对账判定为「非 SkillStar 所有」并反复打印警告。
- Root cause: 迁移初期只改写了普通 Agent 全局目录，遗漏了镜像目录列表；对账规则又不敢删除无法确认归属的断链。
- Fix: 启动迁移与镜像对账同时覆盖 `global_mirror_dirs`，将明确指向已迁出旧 `~/.agents/skills/<name>` 的断链识别并替换为新规范目标；真实实体目录仍保护不删。
- Files: `crates/ss-skills/src/storage_migration.rs`、`crates/ss-skills/src/deployment/ownership.rs`。
- Self-check: `cargo test -p ss-skills --lib --locked installed_skill_migration_repoints_mirror_links_left_behind`。

## 2026-10-07 - 物化边界防线：符号链接逃逸、名字路径穿越、损坏安装锁与内容 hash 碰撞

- Summary:
  1. 任何技能目录复制必须经过 `materialize::copy_confined`（检测循环、限制文件数/总字节/深度、禁止软链逃逸来源根），禁止裸用跟随链接的递归复制。
  2. 来自 `SKILL.md` frontmatter 的名称拼路径前必须经过 `materialize::canonical_skill_name` 归一化，禁止直接拼接未清洗字段。
  3. 写 `.skill-lock.json` 必须使用先备份再拒绝损坏/过高版本的写守卫（`skill_lock::load_for_write` / `mutate`），不得将看不懂的高版本锁重置为空。
  4. 目录内容校验必须做长度前缀字节流式哈希（`dir_content_hash`），并在判定无标记旧部署前先排除 `.git` 等特征目录，禁止字符串拼接哈希。
  5. 全部临时 `.skillstar-stage-*` / `.skillstar-backup-*` / `.skillstar-retain-*` 由 `sweep_stale_transients` 持锁清扫，仅清理目标仍在的安全残留。
- Files: `crates/ss-skills/src/{materialize,skill_lock}.rs`、`crates/ss-skills/src/deployment/ownership.rs`。
- Self-check: `cargo test -p ss-skills --locked`。

## 2026-10-07 - 根技能永远「可更新」、嵌套路径技能被误报「上游已移除」

- Symptom: 仓库根技能每次检测均报更新可用；嵌套路径（如 `skills/a/b`）误报上游已删除。
- Root cause: GitHub `git/trees/{ref}` 返回的是 commit SHA 而非根 `tree.sha`；旧 API 遍历又把含 `/` 的子路径直接当未找到处理。
- Fix: 检测先将 ref 解析为 commit 再读取 `tree.sha`，嵌套路径逐层遍历子树，只有父 tree 可读且确实缺少目标目录时才判定移除；API 不可达时回退 Git 克隆检查。
- Files: `crates/ss-skills/src/update_check.rs`。
- Self-check: `cargo test -p ss-skills --locked update_check`。

## 2026-10-08 - 账号与额度刷新常见陷阱：毫秒时间戳、客户端私有字段被覆盖与缺设备标识

- Symptom: 接入或刷新多平台凭证时，发生有效期算成五万年永不触发刷新、切号写回抹除客户端扩展 JSON/protobuf 未知字段、或计费接口返回缺少 `X-Device-Mid` 错误。
- Fix & Rules:
  1. 第三方凭据时间戳必须核实秒/毫秒单位（如 Claude Code `expiresAt` 为毫秒），将单位编码进字段名（`expires_at_ms`）并保留单一换算函数（`/ 1_000`）。
  2. 额度刷新若令牌无变化禁止写回客户端文件；写回前比对客户端最新会话并原位保留未知扩展字段；非沙箱环境仅按 [D-072](./decisions.md#d-072密钥纯本地加密-json-存储为主仅对官方硬要求目标定点豁免钥匙串) / [D-114](./decisions.md#d-114antigravity-的切号按-cockpit-复刻macos-需要写系统钥匙串) 豁免项操作钥匙串。
- Files: `crates/ss-accounts/`。
- Self-check: `cargo test -p ss-accounts --locked`。

## 2026-09-02 - 测试继承 hook 的 GIT_DIR 污染真实仓库 & 普通目录向上逃逸

- Symptom: Git hook 中运行测试导致宿主仓库 `.git/config` 被改写；对非 Git 技能目录跑更新检测时 `git` 向上查找到 `$HOME` 或父级仓库。
- Fix: 所有外部命令（含测试夹具）统一经过 `command_with_path` 剥离 `GIT_DIR` / `GIT_WORK_TREE` / `GIT_INDEX_FILE` / `GIT_COMMON_DIR` 等环境变量；`check_update_in_session` 在操作前显式检查目标目录自身存在 `.git`。
- Files: `crates/ss-core/src/infra/path_env.rs`、`crates/ss-git/src/ops.rs`。
- Self-check: `GIT_DIR=/.git GIT_WORK_TREE=. cargo test -p ss-core --lib path_env`、`cargo test -p ss-git --locked`。

## 2026-09-01 - 跨 host 复用 ETag 造成假 304 & SOCKS5 本地 DNS 污染

- Symptom: 切换 GitHub/Marketplace 镜像后返回假 304 导致内容卡旧；启用 `socks5://` 代理时仍受本机 DNS 污染失败。
- Fix: `If-None-Match` 严格绑定签发该 ETag 的 `source_host`；SOCKS5 代理统一规范为 `socks5h://` 远端解析 DNS；匿名镜像连通性探测使用公开 raw URL GET 而非根路径 HEAD。见 [D-023](./decisions.md#d-023拉取多源化与网络加固镜像用户排序回退socks5h-与-etag-绑-host)。
- Files: `crates/ss-core/src/config/{github_mirror,proxy}.rs`、`crates/ss-marketplace/src/remote/`。
- Self-check: `cargo test -p ss-core --locked --lib -- github_mirror proxy`、`cargo test -p ss-marketplace --locked`。

## 2026-08-31 - 跨平台 Git / 进程行为差异（Windows 路径与 CRLF、Linux 进程组取消）

- Summary:
  1. Windows `GIT_CONFIG_GLOBAL` 中 `[url "..."] insteadOf` 不能保留未转义反斜杠，必须经 `local_file_url` 转成 `file:///C:/...`；测试初始化仓库必须钉死 `core.autocrlf=false` 与 `core.eol=lf`，避免将内容校验 digest 算成 CRLF 版本；Windows 环境变量大小写不敏感，校验覆盖须在子进程环境（如 `cmd /C set`）观测。
  2. Linux 终止超时子进程仅当确认 child 自身为进程组长（`process_group(0)`）时才向负 PID 发送信号，防止误杀整条 CI job 进程组。
- Files: `crates/ss-git/src/ops.rs`、`crates/ss-git/src/transport.rs`。
- Self-check: `cargo test -p ss-git --locked --lib`。

## 2026-08-12 - Marketplace degraded 快照自锁、URL 拼接与同步状态一致原则

- Summary:
  1. 拼接 host 与 path 时必须在 `join_url` 两端防御斜杠（`trim_end_matches('/')` + `trim_start_matches('/')`），防止拼成非法域名。
  2. 新鲜度判据（`sync_state` scope）必须与写入 scope 一致，不得用「表内已有行」代替 `last_success_at` 或 `degraded_reason`；降级载荷不得写入完整 TTL，同时在收到非 304 完整载荷时无条件解除 `degraded_reason`（degraded 时不发 ETag）。
  3. 数据库/网络排障速查：
     ```bash
     sqlite3 ~/.skillstar/cache/marketplace/marketplace.db \
       "SELECT scope,last_success_at,last_error,next_refresh_at,degraded_reason,source_host,etag FROM marketplace_sync_state;"
     ```
- Files: `crates/ss-marketplace/src/remote/`、`crates/ss-marketplace/src/snapshot/`。
- Self-check: `cargo test -p ss-marketplace --locked`。

## 2026-08-05 - 稀疏工作树代表性错误与 SKILL.md 符号链接读取边界

- Symptom: 使用部分检出工作树预览私有/大型仓库时漏掉其它路径文件，或恶意仓库利用符号链接诱导发现器读取工作树外文件。
- Fix: 完整仓库扫描清单由 `git ls-tree` 权威产出；发现器对遍历目录树与 `SKILL.md` 本身使用 `symlink_metadata` 拒绝外部符号链接，并强制单文件大小上限（1 MiB）。
- Files: `crates/ss-skills/src/discovery/`。
- Self-check: `cargo test -p ss-skills --locked`。
