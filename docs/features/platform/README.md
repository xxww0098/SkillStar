# Platform、Storage 与发布

状态：active

本文件维护跨功能平台服务：路径/存储、日志、GitHub mirror、ACP、后台生命周期、CI 和 updater。全局运行不变量见 [../../architecture.md](../../architecture.md)。

## 路径、存储和 HTTP

- 数据根、hub 根和配置路径统一由 `ss-core` resolver 产生；UI 使用后端返回的 resolved path。桌面多开 profile 根是 `data_root()/instances/<app>/<id>/`（`instances_dir()`），清单在 `config/app_instances.json`；覆盖 `SKILLSTAR_DATA_DIR` 时两者一起走。
- Storage overview 扫描 hub/cache/config 时不跟随 symlink/junction target，避免递归和 Windows 卡死。
- Storage overview、cache cleanup 与 force-delete 的跨域维护流程由 `ss-app::storage_maintenance` 拥有；GUI 只调度并展示结果。
- `SKILLSTAR_DATA_DIR`、`SKILLSTAR_HUB_DIR` 覆盖适用于所有调用方。
- 短探测走 `probe_http_client`。上游流式生成走同一代理指纹的流式客户端，见 [运行架构](../../architecture.md#网络)。都读 `config/proxy.json`。

## 日志

- 控制台订阅由 `ss-core::infra::logging` 独占：`skillstar` 的 GUI 路径调用 `init()`，MCP stdio 进程调用 `init_stderr()`。入口不得再各自拼装 `tracing-subscriber`。
- 人类可读格式为单行：本地时间（毫秒，暗色）、按级别着色并对齐的 level、暗色 target、消息与结构化字段。stdout 非终端、`NO_COLOR` 或重定向时自动关闭 ANSI；`CLICOLOR_FORCE` 可强制开启。
- `RUST_LOG` 覆盖过滤级别（桌面默认 `info`，MCP 默认 `error`）；`SKILLSTAR_LOG_JSON=1` 切换为结构化 JSON，供日志采集使用。
- MCP stdio 的 stdout 仍只承载 JSON-RPC；其诊断只写 stderr 且不着色。

## GitHub Mirror

- 配置写入 `~/.skillstar/config/github_mirror.json`，preset、校验、GitHub 族 URL rewrite、raw 文件连通性探测和 circuit breaker 由 core config module 拥有。健康状态写入 `~/.skillstar/state/github_mirror_health.json`（可重建，不是用户配置）。
- 匿名公开流量改写 GitHub 族 origin：`github.com`、`raw.githubusercontent.com`、`codeload.github.com`、`objects.githubusercontent.com`、`gist.github.com`。通过每条 Git 子进程的 `-c url.*.insteadOf` 注入；永不修改用户全局 `.gitconfig`。`api.github.com` 只在**无 Authorization** 的 HTTP 路径上经加速源包装。
- 加速源候选链按用户在 Settings 里排的顺序回退（`config.order`，首位为选中源）；连续两次传输失败打开 20 分钟熔断，熔断只把开路源从链中跳过，不改变其余源的相对顺序；全部开路则 fail-open。保存新配置重置 circuit；test 命令 GET 一个公开 raw 文件，而不是 HEAD 加速源根。
- SOCKS5 出网使用 `socks5h`（远端 DNS）。新建代理配置带国内 LLM 默认 bypass，已有 `proxy.json` 不自动改写。
- Settings 网络诊断探测代理、直连 GitHub、各加速源和 skills.sh。
- 没有应用内更新器：版本检查只提示不下载，也不再生成 `latest.json`。加速源不得用来安装二进制。见 [Updater 与发布](#updater-与发布)。

## ACP

- ACP client 随 Tauri 壳删除，当前没有 GPUI 入口。不要把客户端放回域 crate 来绕过这个缺口。
- 模型域已移除。外部 Agent 的模型选择不属于 SkillStar。

## 窗口、Tray 与后台运行

- 后台运行偏好仍可让主窗口关闭时隐藏进程。没有托盘，也没有独立用量窗口。
- 域里的 patrol 配置仍在，GPUI 不跑旧巡检循环；GUI 进程存活期间只跑 `ss-app` 的周期唤醒——频道到期自动升级（`channel_wake`）、通用技能自动更新（`skill_wake`，是否开启由 Settings 的「技能更新」偏好决定）和应用版本检查（`release_check_wake`，24 小时至多一次）。
- 确认类弹窗（消息确认、表单、账号页重置额度）都走 kit 的 AlertDialog（`chrome::open_confirm` / `open_form_dialog`）：卡片停在视口上方十分之一处（kit 契约），footer 按钮由 kit 绘制，必须有「取消」和表示该操作的确认按钮：卸载、删除、移除用 danger，其余提交用 primary。自带按钮体系的自定义弹窗（导入框、分享、阅读器）仍走 `chrome/dialog.rs` 的居中 Dialog，不要只设 `Dialog::button_props`，普通 Dialog 不渲染它。账号页自己的遮罩卡片已经居中，不走这条路径。

## CI

- `.github/workflows/ci.yml` 在 Linux 和 macOS 运行 `cargo test --workspace --locked`。Linux 额外运行结构棘轮和 cargo-deny。
- `.github/workflows/windows-ci.yml` 运行同一套 workspace Rust 测试，包含 `skillstar`。
- 只有 `Cargo.lock`。不要恢复 `bun.lock` 或 `package-lock.json`。
- workflow 顶部 `Failure lessons` 记录真实 HOME/SSH 和已退役的前端事故。修改 workflow 前先阅读。
- 结构棘轮采用 shrink-only baseline：历史债告警，新债失败。看门脚本是 workspace 依赖、文件大小、错误字符串、孤儿模块和依赖图文档。

## Updater 与发布

- 没有 updater endpoint，也没有签名私钥。不要恢复 `tauri-action` 或伪造 `latest.json`。
- 版本检查是 check-only（D-103）：域逻辑在 `ss-core::infra::release_check`，经匿名 GitHub 链路请求 `api.github.com` 的 `/releases/latest`，与产品版本（根 `Cargo.toml` `[package]` 的 `CARGO_PKG_VERSION`，由 `skillstar` 二进制启动时传入 GUI）做严格 `MAJOR.MINOR.PATCH` 比较，解析不了的 tag 一律视为不新。结果持久化到 `state/app/release_check.json`；GUI 每小时评估一次、24 小时至多实际检查一次，共享 GitHub API 冷却（`state/skills/github_api_cooldown.json`，与技能更新检查同一份）期间跳过；设置 → 关于 可手动检查、打开 Releases 页。不下载、不替换二进制。
- `v*` tag 触发 `.github/workflows/release.yml`，为 macOS arm/x64、Linux 和 Windows 上传 `skillstar` 二进制。
- GitHub `/releases/latest` 只看到已发布 release。draft 上传完成后由维护者人工发布；发布后客户端最迟 24 小时内（或手动检查时）发现新版本，但只提示，不自动下载。

发布前：

1. 产品版本只改根 `Cargo.toml` 的 `[package] version`，并更新 `Cargo.lock`。
2. 确认普通 CI 全绿。
3. 提交后打 `vX.Y.Z` tag，等待 release matrix。
4. 检查四个二进制后发布 draft。

## 验证

```bash
cargo check --workspace --locked
cargo test --workspace --locked
```
