# 15 — 脱敏

## 契约

请求离开本机之前，按配置遮住秘密。默认关闭。打开后，假上游看到的正文里没有夹具里的秘密，Agent 收到的响应里秘密被还原。密钥文件是 `config_dir()/redact.key`，权限 `0600`。

## 缝

遮罩在 `skillstar-gateway`，位于翻译之后、发给上游之前。配置字段在 `model_gateway.json`。测试用临时数据目录。

## 人可以运行

```bash
cargo test -p skillstar-gateway redact_
```

## 验证

- `redact_off_leaves_body`
- `redact_on_masks_upstream_and_unmasks_response`
- `redact_key_mode_is_0600`

## 可改

遮罩令牌的内部编码，只要往返测试稳定。

## 不可改

默认关闭、密钥文件权限、遮罩发生在出站之前。

## 必须保持绿

08 档的夹具在脱敏关闭时字节不变。

## 会改这一档的反馈

默认配置下假上游看到正文被改过。

## 决定

- `model_gateway.json` 的四个布尔字段是 `redact`、`redact_personal`、`redact_words`、`redact_rules`，缺省都是 false。词表与规则的数组字段是 `redact_word_list` 与 `redact_rule_list`。不另加开关。
