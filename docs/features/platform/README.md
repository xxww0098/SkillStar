# Platform、Storage 与发布

状态：active

本文件维护跨功能平台服务：路径/存储、GitHub mirror、ACP、后台生命周期、CI 和 updater。全局运行不变量见 [../../architecture.md](../../architecture.md)。

## 路径、存储和 HTTP

- 数据根、hub 根和配置路径统一由 `skillstar-core` resolver 产生；UI 使用后端返回的 resolved path。桌面多开 profile 根是 `data_root()/instances/<app>/<id>/`（`instances_dir()`），清单在 `config/app_instances.json`；覆盖 `SKILLSTAR_DATA_DIR` 时两者一起走。
- Storage overview 扫描 hub/cache/config 时不跟随 symlink/junction target，避免递归和 Windows 卡死。
- Storage overview、cache cleanup 与 force-delete 的跨域维护流程由 `skillstar-app::storage_maintenance` 拥有；Tauri command 只调度并返回 DTO。
- `SKILLSTAR_DATA_DIR`、`SKILLSTAR_HUB_DIR` 覆盖适用于所有调用方。
- 远程 HTTP 统一通过 `probe_http_client`，读取 `config/proxy.json`。

## GitHub Mirror

- 配置写入 `~/.skillstar/config/github_mirror.json`，preset、校验、GitHub 族 URL rewrite、raw 文件连通性探测和 circuit breaker 由 core config module 拥有。健康状态写入 `~/.skillstar/state/github_mirror_health.json`（可重建，不是用户配置）。
- 匿名公开流量改写 GitHub 族 origin：`github.com`、`raw.githubusercontent.com`、`codeload.github.com`、`objects.githubusercontent.com`、`gist.github.com`。通过每条 Git 子进程的 `-c url.*.insteadOf` 注入；永不修改用户全局 `.gitconfig`。`api.github.com` 只在**无 Authorization** 的 HTTP 路径上经加速源包装。
- 连续两次传输失败打开 20 分钟熔断；候选链按最近延迟排序并跳过开路；全部开路则 fail-open。保存新配置重置 circuit；test 命令 GET 一个公开 raw 文件，而不是 HEAD 加速源根。
- SOCKS5 出网使用 `socks5h`（远端 DNS）。新建代理配置带国内 LLM 默认 bypass，已有 `proxy.json` 不自动改写。
- Settings 网络诊断探测代理、直连 GitHub、各加速源、skills.sh 和 MCP Registry。
- Updater 插件直连 GitHub Releases 失败时，经匿名加速链读取 `latest.json` 只用于发现新版本；签名安装仍走插件，或提示用户打开 Releases 页面。永不从第三方加速源安装二进制。

## ACP

- ACP client 位于 `src-tauri/src/core/acp_client/`，是 Tauri 专用 adapter。
- ACP 配置命令在 `commands/acp.rs`；built-in label 的兼容归一化只针对明确的旧 built-in command。
- Skill 图文教程是 ACP 的活跃消费者。教程会话只面对隔离的 Skill staging 快照；SkillStar 客户端不暴露 terminal/写文件能力并拒绝非读取权限，prompt 同时禁止网络和修改。外部 ACP Agent 不是 OS sandbox，隔离快照是保护原 Skill 不被修改的硬边界；ACP transport 不拥有教程 freshness、HTML 校验或最终 artifact。
- ACP Agent 的模型选择和鉴权由外部 Agent 自身配置负责；SkillStar 保存启动命令与显示名称，不把 Models provider 配置伪装成 ACP 模型选择。
- `config/acp.json` 同时保存教程风格 id；Settings 只提供代码注册表中的受支持风格，后端按 id 选择版本化 prompt，不接受前端传入任意 prompt 文本。

## 窗口、Tray 与后台运行

- 后台运行开启时，主窗口 close 隐藏；关闭时退出应用并清理 tray。
- tray 与 Settings 使用同一 patrol state/event，Start/Stop label 与实际状态一致。
- tray 菜单同时展示用量额度概览，支持中英文自适应与数据实时刷新。
- 独立 Usage window 等子窗口由后端创建和定位，前端只管理窗口内业务生命周期。

## CI

- `.github/workflows/ci.yml` 在 Linux/macOS 使用 Bun，执行 lint、生产 build、test、Cargo check/test；Linux 额外运行结构棘轮和 cargo-deny。
- `.github/workflows/windows-ci.yml` 使用 `npm ci`，覆盖 lint、生产 build、前端测试和 workspace Rust 测试。
- `bun.lock` 与 `package-lock.json` 同时受控；依赖变化同时更新。
- workflow 顶部 `Failure lessons` 记录双 lockfile、tsc、真实 HOME/SSH 和 Windows-only 声明等事故。修改 workflow 前先阅读，不能把同类事故再引入。
- 结构棘轮采用 shrink-only baseline：历史债告警，新债失败；workspace、feature import、文件大小与 command boundary 均由可执行脚本看门。

## Updater 与发布

- `src-tauri/tauri.conf.json` 定义 updater endpoint、公钥和 artifact 生成；私钥只存在 GitHub Actions secrets 和维护者安全备份。
- `commands/updater.rs` 执行 check、download/install 和 restart；失败返回错误，不伪装成“已是最新版”。
- `useUpdater.ts` 负责 mount/周期检查、banner、retry 和 restart UX。
- `v*` tag 触发 `.github/workflows/release.yml` 构建 macOS arm/x64、Linux 和 Windows，并生成签名 artifact/`latest.json`。
- GitHub `/releases/latest` 只看到已发布 release；draft 构建完成后必须人工 publish，客户端才会发现更新。

发布前：

1. 同步 `package.json`、`src-tauri/Cargo.toml`、`tauri.conf.json` 版本及 lockfile。
2. 确认普通 CI 全绿并在本地运行 `bun run build`。
3. 提交后打 `vX.Y.Z` tag，等待 release matrix。
4. 检查 installers、签名和 `latest.json` 后发布 draft。

## 验证

```bash
bun run lint
bun run build
bun run test
cargo check --workspace --locked
cargo test --workspace --locked
```
