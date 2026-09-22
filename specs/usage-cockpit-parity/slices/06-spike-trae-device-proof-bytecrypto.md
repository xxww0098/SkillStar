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

## 结果

本地密码学已证明。没有调用真实 `ExchangeToken`，没有网络。服务端是否接受这份 DeviceProof：**未验证**。不降级 Trae。kill-gate 保持打开，直到手上有真实 refresh token。

- `tool_store::byte_crypto`：布局按 cockpit。6 字节头 + 32 字节随机密钥 + AES-128-CBC(SHA-512(明文) ‖ 明文)。密钥和 IV 来自 `SHA-512(SHA-512(随机密钥) ‖ salt)` 的前 32 字节。公开头 `[116, 99, 5, 16, 0, 0]` 和私有头都能往返。改密文最后一字节返回 `byte_crypto 完整性校验失败`（PKCS7 失败或 SHA-512 不一致都是这个错误）。`storage.json` 里的值是这段原始 blob 的标准 base64；本模块编解码的是 raw bytes。
- 设备密钥：`ring` 0.17，生成和签名都用 `ECDSA_P256_SHA256_ASN1_SIGNING`。cockpit 生成用 FIXED、签名用 ASN.1，曲线相同；这里合成一个算法，避免 PKCS#8 算法标识对不上。PEM 是 `PRIVATE KEY` / `PUBLIC KEY`（PKCS#8 与 SPKI）。Rust 字段是 `private_pem` / `public_pem`，不是 JSON 的 `privateKeyPEM`。
- `sign_device_proof` 返回 ASN.1 签名的标准 base64，对应 cockpit `DeviceProof.Signature`，不是整段 `{Signature, Timestamp, Nonce}` JSON。时间戳和 nonce 由调用方传入。
- 消息字节：`POST\n<path>\n<clientId>\n<refreshToken>\n<ts>\n<nonce>`。固定 nonce `00112233445566778899aabbccddeeff`、ts `1700000000` 的签名能用公钥验过；改 client id 后验签失败。
- 确定性：不是 RFC6979。`EcdsaKeyPair::sign` 把 `SystemRandom` 混进 nonce。本机对同一输入连签两次，签名不相等。测试只验签，不比较两次签名。
