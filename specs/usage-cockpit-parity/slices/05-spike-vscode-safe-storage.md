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
