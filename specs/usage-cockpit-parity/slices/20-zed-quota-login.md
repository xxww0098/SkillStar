# 20 · zed — 配额 + 登录 + 导入（L0–L3）

> 依赖：02（local_server 参数 map）+ 07（RSA 解密已验证）。

## 解锁的契约

`zed` catalog 上线：`native_app_signin` OAuth（本地监听收 RSA 加密凭据）+ TokenImport
（`{user_id, access_token}` JSON 或裸 token）+ 本机导入（macOS Keychain 读）；
配额显示订阅状态 / Edit Predictions / Token Spend / Spend Limit / 账期结束。

## API 接缝

| 层 | 文件 | 改动 |
| --- | --- | --- |
| 域 | `catalog.rs` + `identity.rs` | `zed` 行 + identity |
| 域 | `fetchers/oauth/zed.rs` | `start_login`：生成一次性 RSA-2048 对 → `zed.dev/native_app_signin?native_app_port=<p>&native_app_public_key=<pk>` → loopback 收 `user_id`+加密 `access_token` → 解密（07 的成果）→ 建卡；quota=`cloud.zed.dev/client/users/me`，**`Authorization: "<user_id> <token>"` 自定义 scheme**；billing 端点一律不调（对有效桌面凭据也 401——误置 reauth 的坑） |
| 域 | 本机导入 | `security find-internet-password -s https://zed.dev`（meta 取 account=user_id，`-w` 取 token）；非 macOS 明确「仅支持 macOS 本机导入」 |

## 字段落法

`access_token_encrypted`=access token；`oauth_account_id`=user_id；无 refresh（token 长效）。

## 人能看见

Zed 卡点登录→浏览器授权→自动完成；本机导入读 Keychain；卡片显示 spend/limit。

## 验证

- RSA 回调解密测试（07 的 fixture 直接复用）；quota `Authorization` 头格式断言。
- 401→AuthRequired 分类；非 macOS 本机导入降级文案测试。
- 真实联调一次。
- **视觉门禁**：卡片截图 → screenshot-critique。

## 委托给实现者的决定

- 本地监听端口分配复用 `local_server` 还是独立 socket（倾向复用）。

## 必须保持绿

- 不调 `/frontend/billing/*`；不尝试刷新 zed token（无 refresh 腿，失效走 reauth）。

## 会改变本片的人类反馈

- 无。
