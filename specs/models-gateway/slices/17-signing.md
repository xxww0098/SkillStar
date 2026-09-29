# 17 — 订阅签名

## 契约

网关用调用方注入的 `AccountSnapshot` 签上游。映射以 README 的表为准。`anthropic` 不产生 Anthropic HTTP，生成仍走 05 档的进程桥。`cursor` 的签名实现写在 `skillstar-gateway`，可以阅读 `cursor.rs`，不能修改它。

没有 Usage 行的 gemini CLI、devin、workbuddy、commandcode 不签订阅。它们若要出站，只用 `ProviderSnapshot` 里的 API 密钥。不读厂商自己的认证文件，不做第二套登录。

缺失快照时不发配额请求，候选保持 unknown。

## 缝

trait 定义在 `skillstar-gateway`：按 `catalog_id` 取账户与已经写好的余量。`skillstar-app` 用 `skillstar-usage` 的只读列表实现它。网关测试用假 trait。夹具是签名后的请求头，从 magpie 对应测试的期望抄来，秘密换成夹具常量。

`docs/features/models/README.md` 写明谁可以签、谁只走 API 密钥。`docs/features/usage/README.md` 加一句：网关读取已有凭证，不在这里实现第二套登录。不改 `cursor.rs`。

## 人可以运行

```bash
cargo test -p skillstar-gateway sign_
```

## 验证

- 每个有映射的 `catalog_id` 一条头夹具，anthropic 那条断言 HTTP 客户端未被调用
- `sign_cursor_does_not_modify_cursor_rs`（git diff 或测试不链接那个文件的写入）
- `sign_missing_snapshot_makes_no_quota_request`
- `sign_gemini_cli_has_no_account_path`

## 可改

trait 的方法名。

## 不可改

映射表、不改 `cursor.rs`、不刷新不写回、不读厂商认证文件。

## 必须保持绿

`cargo test -p skillstar-usage` 的现有套件。05 档的 `claude_bridge_strips_oauth_even_when_snapshot_has_token`。

## 会改这一档的反馈

夹具头比 magpie 测试多出一个该测试没期望的头，或 anthropic 路径打开了 HTTP。

## 决定

- 头不在 magpie 测试里的，不加。实现时把出处文件名写进夹具旁的一行注释。
