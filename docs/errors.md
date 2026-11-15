# Error Log

状态：active

记录根因不直观、容易在后续修改或重构时复发的故障模式与自检方式。

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
