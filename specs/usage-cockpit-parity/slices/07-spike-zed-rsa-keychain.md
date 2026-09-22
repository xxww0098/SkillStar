# 07 · SPIKE — Zed RSA 回调解密 + internet-password keychain

> kill-gate。回答两个问题：**zed.dev 回调里的 RSA 加密 access_token 能否解开
> （OAEP 与 PKCS1v15 两种 padding 都要试——cockpit 代码两分支并存，由服务端决定）**；
> **`security add-internet-password -s https://zed.dev` 写回 + 回读是否成立**。
> 门禁 zed 的 L2/L4。

## 解锁的契约

- `fetchers/oauth/zed.rs` 内的回调解密：`decrypt_zed_token(priv_der, ciphertext_b64)`
  （OAEP-SHA256 先试，失败回落 PKCS1v15）。
- `tool_store::keychain_cli` 的 internet-password 族：
  `find_internet_password(service, account) -> Option<String>` /
  `add_internet_password(service, account, secret) / -U 覆盖` /
  `delete_internet_password(service)`。与 generic-password 族（anthropic/codex）并存。

## 验证

- 本地 RSA-2048 fixture：公钥加密→私钥解密恒等（两种 padding 各一组向量）。
- macOS 真实 keychain：`security` CLI 对测试 service（非 zed.dev 真条目）做
  写→读→删回环；`SKILLSTAR_TOOL_SYNC_HOME` 沙箱下整体关闭（沿用 keychain.rs 先例）。
- 非 macOS：`available()` 返回 false 的编译/运行断言。

## 结果记录

- RSA 解密失败 → zed 锁 L1（TokenImport 粘 `{user_id, access_token}` JSON 仍可用）。
- keychain 写回失败 → zed 锁 L3（本机导入可读不可写）。
- 结构性结论（不需实验）：zed **永不进实例注册表**——`https://zed.dev` keychain 是全局的，
  `--user-data-dir` 对原生 Zed 不存在；写进 `apps.rs` 的 `UnsupportedApp` 理由。

## 依赖

`cargo add rsa`（RSA-2048，仅回调解密用）。

## 委托给实现者的决定

- `rsa` crate 版本与 padding 探测顺序。

## 必须保持绿

- 不触碰 codex/anthropic 的 keychain 路径。
