# 08 · SPIKE — ZCode `enc:v1` 凭据文件

> kill-gate。回答一个问题：**`~/.zcode/v2/credentials.json` 的 `enc:v1:` AES-256-GCM
> 值能否用机器派生密钥解密并加密写回**。门禁 zcode 的 L3/L4（L2 的 SchemePaste
> 不依赖本 spike）。

## 解锁的契约

`tool_store::enc_file`（或 `zcode_credentials.rs`，择一归属，建议 usage crate 内
`tool_store`）：`decrypt_enc_v1(key, value)` / `encrypt_enc_v1(key, plaintext)` +
`zcode_credential_key(home_dir) -> [u8;32]`。

## 机制（移植自 cockpit `src-tauri/src/modules/zcode_account.rs`）

- 值格式：`enc:v1:<b64 nonce><b64 ciphertext+tag>`（12B nonce，AES-256-GCM）。
- 密钥：`SHA256(env ZCODE_CREDENTIAL_SECRET 或
  "zcode-credential-fallback:{platform}:{home}:{username}")`。
- 文件定位：`~/.zcode/v2/credentials.json`；但 `settings.json`/`config.json` 的
  `dataBaseDir` 覆盖优先——**先读 settings 再定位 credentials**。
- 写回是整文件 JSON：备份 + 原子替换 + 回读解密校验。

## 验证

- enc:v1 round-trip fixture（含用 cockpit 格式生成的已知向量一条）。
- 密钥推导测试：env 覆盖优先于 fallback；home/username 拼接逐字节对齐。
- `dataBaseDir` 覆盖场景的路径解析测试。
- 写回回环：临时 credentials.json 解密→改字段→加密→写→重读校验。

## 结果记录

- 解密失败 → zcode 锁 L2 以下（无 L3/L4）。
- 结论写本文件「结果」节。

## 依赖

`cargo add aes-gcm`（若 `crypto.rs` 内部实现可复用则复用其 cipher 构造）。

## 委托给实现者的决定

- 复用 `crypto.rs` 的 AES-GCM 基建还是独立（密钥派生不同，cipher 调用可共享）。

## 必须保持绿

- 测试全部走 `SKILLSTAR_TOOL_SYNC_HOME`/`SKILLSTAR_DATA_DIR` 沙箱；不读真实 `~/.zcode`。

## 结果

round-trip 已证明。没有打开真实 ZCode，也没有读 `~/.zcode`。官方 app 是否接受写回：**未验证**。不降级。

规格把值写成 `enc:v1:` 后面两段 base64（nonce，以及 ciphertext‖tag）。cockpit `zcode_account.rs` 不是这样。本机用 Node 复解了它的固定向量，实际是三段 URL-safe base64、无 padding：

`enc:v1:{nonce}.{tag}.{ciphertext}`

nonce 12 字节。AES-256-GCM 的输出仍是 ciphertext‖tag，但 tag（最后 16 字节）单独成段，不跟 ciphertext 放进同一个 base64。固定向量 `enc:v1:AAECAwQFBgcICQoL.NTIF8rgqI66J7hvPIwTD8g.QTtgwDlfAEvz72ttQggYC2KZyVwLVA` 的密钥是 `SHA-256("zcode-credential-fallback:darwin:/Users/zcode-test:test-user")`，明文是 `official-fixture-token`。

密钥：非空 `ZCODE_CREDENTIAL_SECRET` 优先，空字符串不算覆盖；否则 `zcode-credential-fallback:{platform}:{home}:{username}` 再 SHA-256。platform 跟 cockpit：`darwin` / `win32` / 其他用 `std::env::consts::OS`。

没有 `enc:v1:` 前缀时返回格式错误。cockpit 会把原文原样返回；这里不这样做，避免写回时把明文再加密一遍还当成解密成功。

路径是 `{zcode_home()}/v2/credentials.json`。`zcode_home()` 读 `setting.json`（不是 `settings.json`）的 `dataBaseDir`，根变成 `{dataBaseDir}/.zcode`。覆盖路径上的写回在临时目录完成：解密 → 改字段 → 加密 → `tool_store::atomic_json` → 再读。上一版字节由测试另存到旁边，没有新的备份框架。沙箱是 `SKILLSTAR_TOOL_SYNC_HOME`。
