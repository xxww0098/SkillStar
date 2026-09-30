# 19 — Claude Code 文件

## 契约

Claude Code 的 `ANTHROPIC_BASE_URL` 是网关根 `http://127.0.0.1:<端口>`，不带 `/v1`。`ANTHROPIC_AUTH_TOKEN` 是占位 bearer `skillstar`。主模型与各档模型写进 magpie `claude.go` 使用的那些 env 键。取消托管时按 stash 恢复用户原来的 URL 和 token。

这是配置文件。05 档的进程桥是另一条路径，本档不启动 `claude`。

## 缝

`skillstar-gateway` 里 Claude Code 的 writer，由 `apply_gateway("claude", ...)` 接到 18 档的入口。不再返回 `agent_not_managed`。

## 人可以运行

临时 HOME 中保存 Claude Code，读 `settings.json` 的 env。

## 验证

- `claude_code_base_url_has_no_v1`
- `claude_code_auth_token_is_placeholder`
- `claude_code_tiers_match_fixture`
- `claude_code_unstash`

## 可改

env 键在代码里的常量表位置。键名本身不可改。

## 不可改

无 `/v1`、占位 token 不是 Usage 的 access token、不写 `CLAUDE_CODE_OAUTH_TOKEN` 进配置文件。

## 必须保持绿

`apply_gateway_` 里其他 id。05 档进程桥测试。

## 会改这一档的反馈

base URL 带了 `/v1`，或 token 等于测试注入的 Usage access token。

## 决定

- 角色 env 与 magpie 的 tier 表一致。SkillStar 旧的 `AgentBinding.roles` 不再投影到这些文件。
- 与 18 档一样整份替换 `settings.json`。托管期间其它键不留在文件里。取消托管把接管前的整份文件放回，用户原来的 URL 和 token 在里面。
- 这个函数只收到一个 `model_ref`。四个档、`ANTHROPIC_SMALL_FAST_MODEL` 和 `CLAUDE_CODE_SUBAGENT_MODEL` 都写成它。不写 `CLAUDE_CODE_OAUTH_TOKEN`，也不写 effort。
