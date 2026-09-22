# 05 · SPIKE — VS Code Safe Storage 加解密

> 类型：kill-gate spike。回答一个问题：**我们能不能对 `state.vscdb` 里的 `secret://`
> 加密值做解密→改值→加密→写回，且官方 app 重启后仍认可？**
> blast radius 最大：windsurf / qoder / codebuddy×2 /（可选 copilot VS Code 注入）的
> L3/L4/L5 全系于它。

## 解锁的契约

`tool_store::safe_storage` 模块：`decrypt_secret(key_material, ciphertext)` /
`encrypt_secret(key_material, plaintext)`，覆盖三路平台分支。

## 机制（移植自 cockpit `vscode_inject.rs`）

- **macOS**：`security find-generic-password -s "<App> Safe Storage"` 取密码 →
  PBKDF2-SHA1(password, "saltysalt", 1000 轮) → AES-128-CBC 解 `v10` 前缀值。
- **Windows**：DPAPI 解 `Local State` 里的 os_crypt key → AES-256-GCM 解 `v11` 值。
- **Linux**：secret-tool / 硬编码 `peanuts` 口令的同构 PBKDF2 路径。
- `state.vscdb` `ItemTable` 中 `secret://<extensionId>.<key>` 值 = 上述密文的 base64。

## 验证（全部本地 fixture，无需真实账号）

- v10 已知向量 round-trip：用注入 key 加密→解密恒等；**不碰真 keychain**（测试注入密码材料）。
- v11 AES-GCM round-trip；corrupted/wrong-key 明确报错。
- 临时 `state.vscdb` 上 `secret://` 键的「解密→改→加密→写回→重解密」回环。
- 人工一步（记录结论，不阻塞）：macOS 上对真实 Windsurf 做一次写回+重启验证认可。

## 结果记录

在本文件底部「结果」节写明：三平台各自的可用性结论（可用 / 仅解密 / 不可用），
波及面按 README 停机表降级到对应 provider 的 L-级。

## 依赖

`cargo add aes cbc pbkdf2 sha1`（Windows DPAPI 用 target-dep）；走根 `Cargo.toml` 版本归一化。

## 委托给实现者的决定

- 模块内部组织与错误类型命名。
- Linux secret-tool 是否 shell out（沿用 security CLI 先例的风格）。

## 必须保持绿

- 纯新增模块；不触碰任何既有读写路径。

## 结果

本机是 macOS。只证明了**注入密钥**下的加解密，没有读登录钥匙串，也没有对真实 Windsurf 写回后重启。官方 app 是否接受写回：**未验证**。不记为失败，也不按停机表降级。

规格写「PBKDF2 1000 轮」和「Windows 的 v11 = AES-256-GCM」。对照 cockpit `vscode_inject.rs` 和 Chromium 后，这两条都不对，实现按参照实现：

- macOS `v10`：PBKDF2-HMAC-SHA1(password, `saltysalt`, **1003** 轮) → AES-128-CBC，IV 为 16 个空格，PKCS7。`test-password` 的 1003 轮密钥与 1000 轮不同，测试锁了这一点。密钥由调用方注入，模块不调用 `security`。
- Linux：同一条 CBC，**1** 轮。`peanuts` 和空口令的派生密钥与 cockpit 常量逐字节相同，前缀 `v10`。secret-tool 口令同样 1 轮，前缀 `v11`，仍然是 CBC，不是 GCM。测试不调用 `secret-tool`。
- Windows GCM：规格把这条叫 v11。cockpit / Chromium 的实际前缀仍是 `v10`，后面 12 字节 nonce，再 AES-256-GCM（ciphertext‖tag）。已知向量用 Node `aes-256-gcm` 生成，解密通过。测试注入 32 字节 os_crypt key。
- DPAPI：`unwrap_os_crypt_key_from_local_state` 只解析 Local State 的 `os_crypt.encrypted_key`。非 Windows 返回明确错误；本构建即使在 Windows 上也不链接 `CryptUnprotectData`。测试没有调用 DPAPI。
- `state.vscdb` 里的值是原始 payload 的标准 base64，不是 cockpit Copilot 路径用的 Node `{"type":"Buffer","data":[...]}`。临时库上 `secret://` 解密→改→加密→写回→再解密通过，无关行保留。

| 平台 | 结论 |
| --- | --- |
| macOS | 密码学可用（注入口令的 v10 CBC 往返，含固定向量）。真钥匙串与 Windsurf 重启未做。 |
| Linux | 密码学可用（`peanuts` / `v11` CBC，注入口令）。未跑 secret-tool，未跑真实 app。 |
| Windows | 密码学可用（注入 key 的 AES-256-GCM 往返）。DPAPI 解开未实现，不能从 Local State 得到 key。未跑真实 app。 |
