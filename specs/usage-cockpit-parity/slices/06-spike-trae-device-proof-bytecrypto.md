# 06 · SPIKE — Trae `DeviceProof` + iCube `byte_crypto`

> kill-gate。回答两个问题：**P-256 ECDSA 设备签名能否被 `ExchangeToken` 接受**；
> **`storage.json` 的 iCube `byte_crypto` 值能否解密→改→加密→写回**。
> 门禁 trae×4 的 L2/L3/L4。

## 解锁的契约

- `tool_store::byte_crypto`：`decode/encode` iCube `byte_crypto` 值
  （AES-128-CBC，随机密钥内嵌 blob，SHA512 完整性校验）。
- `fetchers/trae/device.rs`（草稿放 spike 内验证，转正于切片 18）：
  `generate_device_keypair() -> {privateKeyPEM, publicKeyPEM}`；
  `sign_device_proof(priv_pem, method, path, client_id, refresh_token, ts, nonce) -> String`。

## 机制（移植自 cockpit `trae_account_core_refresh.rs` / `byte_crypto` 相关）

- `DeviceProof` message 拼法逐字节对齐：
  `POST\n<path>\n<clientId>\n<refreshToken>\n<ts>\n<nonce>` → ECDSA-P256/SHA256。
- `ExchangeToken` 请求体：`ClientID`+`ClientSecret`+`RefreshToken`+`DeviceInfo`(含 PublicKey)+`DeviceProof`。
- `storage.json` 目标键：`iCubeAuthInfo://icube.cloudide`、`iCubeServerData://*`、
  `iCubeEntitlementInfo://*`、`iCubeAuthInfo://usertag`。

## 验证

- byte_crypto 编解码 fixture round-trip（含篡改检测：改一字节 → SHA512 校验失败）。
- device-proof：固定 nonce/ts 的签名确定性测试 + 验签（公钥验自己签名）。
- 人工一步：真实 `ExchangeToken` 调用一次（手上有 refresh token 时），记录接受/拒绝。

## 结果记录

- byte_crypto 失败 → trae×4 锁 L2 以下（无 L3/L4）。
- device-proof 被拒 → trae×4 锁 L1（TokenImport 粘 refresh_token；若同一 ExchangeToken
  仍可用于 refresh 则保留自动刷新，否则连 refresh 都靠重新导入）。
- 结论写本文件「结果」节 + `docs/errors.md`（若降级）。

## 依赖

`cargo add ring`（ECDSA P-256）+ `aes`/`cbc`（与 05 共用）。

## 委托给实现者的决定

- `ring` vs `p256` crate 选择（cockpit 用 ring，优先同源减少偏差）。

## 必须保持绿

- 纯新增；不动 `storage.json` 以外的任何文件族。
