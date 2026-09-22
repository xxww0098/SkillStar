# 14 · qoder — 配额 + 登录 + 导入（L0–L3）

## 解锁的契约

`qoder` catalog 上线：device/selectAccounts + machine-token 轮询登录（flow=`RemotePoll`，
**无 user_code**——面板只显示「打开链接→网页确认→自动完成」）+ TokenImport（userInfo JSON）+
本机导入；配额显示 credits 使用/剩余 + 套餐原始值。

## API 接缝

| 层 | 文件 | 改动 |
| --- | --- | --- |
| 域 | `catalog.rs` + `identity.rs` | `qoder` 行 + identity |
| 域 | `fetchers/oauth/qoder.rs` | machine info/machine token → `qoder.com/device/selectAccounts?nonce&challenge&redirect_uri=qoder://…` → 轮询 `openapi.qoder.sh/api/v1/deviceToken/poll`（nonce+PKCE verifier+`Cosy-*` 机器头）；quota=`openapi.qoder.sh`：`userinfo`+`user/plan`+`quota/usage`+`v3/user/status`（**请求头也靠 machine blob**） |
| 域 | 本机导入 | `qoder_state_db_path()` 多候选路径；`secret://aicoding.auth.*` 键（明文键先读，secret:// 值依赖切片 05） |

## 内嵌 kill 实验

`deviceToken/poll` 是否强制校验 `Cosy-*`/机器指纹头：先无头/伪造头试一次；若服务端
校验不可伪造的设备证明 → qoder 锁 L1（TokenImport+本机导入仍可用）。结论写「结果」节。

## 字段落法

`provider_state_encrypted`=`{machineToken, machineId, machineType, hostname, os, cosy_version}`
（quota 请求头来源）；`access_token_encrypted`=qoder token；`oauth_account_id`=user id。

## 人能看见

Qoder 卡：登录面板显示授权链接+等待动画；TokenImport 粘贴 userInfo JSON 建卡。

## 验证

- poll 序列 mock 测试（pending→ready、超时、取消语义）。
- Cosy 头最小集合断言（若实验证明必需）。
- 多候选 state.vscdb 路径解析测试。
- 真实联调一次。
- **视觉门禁**：RemotePoll 面板在 qoder 场景的截图 → screenshot-critique。

## 委托给实现者的决定

- machine info 采集字段取舍（对齐 cockpit `QoderMachineInfo`）。

## 必须保持绿

- quota 请求头构造集中一处，不散落。

## 会改变本片的人类反馈

- 无。
