# 24 — WSL Codex

## 契约

Windows 上每个正在运行的发行版是单独的 Agent，id 为 `codex@wsl:<distro>`。编辑的是该发行版里的 Codex 配置，路径经 `\\wsl.localhost\<distro>`。不启动已停止的发行版。

网关地址：WSL 与 Windows 共享网络（mirrored）时写 `127.0.0.1`；NAT 时写 Windows 在该发行版里看到的地址。两种拼法都由夹具钉死。监听仍默认环回；NAT 要通，需 34 档把监听打开到局域网之后才在真机上通。本档先锁路径和 URL 拼法。

非 Windows 不调用 `wsl.exe`。路径拼写测试在所有系统上跑。

## 缝

`skillstar-gateway`。`wsl.exe` 藏在一个由测试替换的函数后面。生产实现只在 Windows 调用它。

## 人可以运行

```bash
cargo test -p skillstar-gateway wsl_codex_
```

Windows 上再对一个正在运行的发行版做一次真探针。当前机器不是 Windows 时，在 README 的 Next Agent Prompt 里写明真探针未跑，不要标成三平台已通过。

## 验证

- `wsl_codex_id_spelling`
- `wsl_codex_mirrored_url_is_loopback`
- `wsl_codex_nat_url_uses_windows_host`
- `wsl_codex_does_not_start_stopped_distro`
- `wsl_codex_non_windows_skips_exe`

## 可改

列举发行版的命令封装。

## 不可改

id 格式、不启动已停止的发行版、mirrored 仍写 127.0.0.1、非 Windows 不调用 wsl.exe。

## 必须保持绿

06 档本机 Codex 的两种写盘。WSL 写入使用同一套 toml 形态。

## 会改这一档的反馈

已停止的发行版被启动，或 mirrored 夹具写成了局域网 IP。

## 决定

- 真机探针未跑之前，规格状态不写「Windows 已验证」。
- `apply_gateway("codex@wsl:…")` 仍返回未托管。入口是 `wsl_codex_list` 和 `apply_wsl_codex`。非 Windows 的 `wsl_codex_discover` 不调用 `wsl.exe`。
- mirrored 写 `127.0.0.1`。NAT 写探针里的 Windows 地址；没探到地址时仍写 `127.0.0.1`。端口是网关监听端口。
- 已停止的发行版不探测、不写文件。`docker-desktop` 开头的名字跳过。列不出正在运行的名单时，一个都不探测。
- toml 形态与本机 Codex 相同。API 形态的目录路径是发行版里的 Linux 路径。stash 键是 `codex@wsl:<distro>.<field>`，不覆盖本机 `codex.*`。登录态仍由调用方传入。
- 不写 `wsl.json`，不记住已停止的发行版。
