# 05 — Claude 进程桥

## 契约

订阅侧的 Claude 生成启动本机 `claude` 二进制，经 MCP 把工具调用送回网关。SkillStar 进程不向 Anthropic 的令牌地址发请求，也不向 `api.anthropic.com` 发 `/v1/messages`。即使 `AccountSnapshot` 里有 access token，子进程环境里也不出现 `CLAUDE_CODE_OAUTH_TOKEN`。

## 缝

`skillstar-gateway` 的进程桥。`skillstar-app` 调用时不传入 OAuth 字符串。查找二进制的顺序：`PATH`，然后 `~/.local/bin/claude`、`/usr/local/bin/claude`、`/opt/homebrew/bin/claude`。测试用临时目录里的假二进制，不调用真的 Claude。

环境先去掉 `ANTHROPIC_BASE_URL`、`ANTHROPIC_API_KEY`、`ANTHROPIC_AUTH_TOKEN`、`CLAUDE_CODE_OAUTH_TOKEN`、`CLAUDECODE`、`CLAUDE_CODE_ENTRYPOINT`、`CLAUDE_CODE_SSE_PORT`，再设置 `ENABLE_CLAUDEAI_MCP_SERVERS=0`、`DISABLE_AUTO_COMPACT=1`、`CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`。

参数与 README 已定行为里的列表一致。`xhigh` 写成 `max`。MCP 命令是当前可执行文件加 `claude-mcp-helper`、回调 URL、工具文件。回调路径是 `/_skillstar/claude-mcp/{token}`，处理函数拒绝非环回连接。

临时目录前缀 `skillstar-claude-`。回合中止 30 分钟，空闲保留 20 分钟且最多 6 个，工具等待 5 分钟。测试注入时钟。stderr 保留上限 1 MiB。

`src-tauri` 把 `claude-mcp-helper` 认成 CLI，不打开窗口。`README.md` 写上这条子命令。

## 人可以运行

```bash
cargo test -p skillstar-gateway claude_bridge_
```

假二进制把环境和参数写进临时文件。测试读那个文件。

## 验证

- `claude_bridge_strips_oauth_even_when_snapshot_has_token`
- `claude_bridge_args_match_magpie_list`
- `claude_bridge_callback_rejects_non_loopback`
- `claude_bridge_idle_caps`
- 网关测试进程的出站记录里没有 Anthropic 令牌 URL，也没有 `/v1/messages`

## 可改

假二进制脚本的写法、运行表的内部结构。

## 不可改

不注入、不刷新、不写回钥匙串或 `~/.claude/.credentials.json`。三个时限。回调只在环回。

## 必须保持绿

`cargo test -p skillstar-gateway`。`cargo check -p skillstar-app --locked`。

## 会改这一档的反馈

假二进制看到了 `CLAUDE_CODE_OAUTH_TOKEN`，或参数列表和 magpie `claudeCLIArgs` 不一致。

## 决定

- 相对 magpie 的允许差异写进本档测试的名字：有侧账户 token 时仍然不注入。
- helper 的 stdout 只有 MCP 帧。日志走 stderr。
