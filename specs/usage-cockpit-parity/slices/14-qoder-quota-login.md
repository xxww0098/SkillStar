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

## 结果

`qoder` 已进 catalog（OAuth + TokenImport，品牌色 `2ADB5C`，续费页 `https://qoder.com`）和 identity。配额、device poll 登录、令牌导入、本机导入都在 `fetchers/oauth/qoder/`。Logo 用 lobe 已导出的 `QoderColor`。登录面板没有 user code：`OAuthStartInfo::remote_poll`，不是 `device()`。

测试里登录和配额都走到了，没有打真实 `qoder.com` / `openapi.qoder.sh`：

- 授权 URL：`nonce`、`challenge`、`challenge_method=S256`、`redirect_uri=qoder://aicoding.aicoding-agent/login-success`。有 machine token 时 query `machine_id` 用的是这份 token（cockpit 同款），否则用 `SharedClientCache/cache/id`。没有 `client_id`，没有 `user_code`。
- Poll：`GET /api/v1/deviceToken/poll?nonce&verifier&challenge_method`。404 或没有 token 继续等；随后 200 带 token 完成。超时文案是「Qoder 登录已超时」；取消（pending 会话被摘掉，或轮询中途取消）是「用户取消登录」。401 / `invalid_grant` 立刻 AuthRequired。HTTP 200 且 JSON `code` 不是 0/200/ok/success 是 Fetcher，不是 Transient。403 不是 AuthRequired。429/5xx 先记住，到截止时间仍以 Transient 结束。
- Cosy 头只在 `header_pairs` 构造，poll 和配额都走 `apply_headers`。机器信息齐全时带上 cockpit 那一组：`Cosy-Version`、`Cosy-MachineToken`、`Cosy-MachineType`、`Cosy-MachineCode`、`Cosy-MachineId`、`Cosy-MachineHostname`、`Cosy-MachineOS`、`Cosy-ClientType=0`。没有机器字段时仍带 `Cosy-MachineOS`（本进程 arch_os）和 `Cosy-ClientType`。Poll 不带 Authorization。
- 配额：`/api/v1/userinfo`、`/api/v3/user/status`、`/api/v2/user/plan`、`/api/v2/quota/usage`，Bearer + 同一组 Cosy 头。Credits 窗口只在真有 `used` 时出现；缺 used 就不建窗口，不补 0。API 自己给的 0 保留。`total` 缺失但 `used` 和 `remaining` 都在时，total = used + remaining。套餐名用原始字符串。`LoginExpire` → AuthRequired。plan/usage 的 404 省略该段，不失败。
- 令牌导入接受 userInfo / `auth_user_info_raw` / `{accounts:[...]}` JSON，也接受 cockpit 那种 token 字符串（至少 20 字符、无空白、含字母）。垃圾拒绝。
- 本机导入走 `tool_paths::qoder_state_db_path()`（多候选，先存在者优先）。明文键 `aicoding.auth.userInfo|userPlan|creditUsage` 先于 `secret://`。`secret://` 只用注入的 `KeyMaterial` 解密；没有密钥时报错并写明不会读系统钥匙串。测试不碰真实 home。

`provider_state` 是明文 JSON `{machineToken, machineId, machineType, machineCode, hostname, os, cosy_version}`，由登录和导入管线加密。不写 `platform_token_encrypted`。`oauth_account_id` 是 user id。access token 是 qoder token，有 refresh 则进 `refresh_token_encrypted`。

内嵌实验（poll 是否强制 Cosy 头）没有打真实服务，未降级到 L1。TokenImport 和本机导入仍在。

真实登录未联调。

静默决定：

- 不探测本机 `product.json`，也不读钥匙串。没有 `cosy_version` 就不发 `Cosy-Version`。
- `machineCode` 比切片字段表多一项，因为 cockpit 的 `Cosy-MachineCode` 要用它。
- Poll 也带头。cockpit 的 `poll_device_token_once` 本身不带，切片写了要带。
- 配额只有一个月度 Credits 窗口，不拆周期。窗口标签 `Credits` 复用已有 i18n，没有新文案。
- 不用用户显示名之类的泛 `name` 当套餐名。
- 刷新没有单独的 refresh-token 交换（cockpit 也没有这条腿）。401 就是重新登录。
- 登录时 plan/usage 的非鉴权失败不丢掉刚换到的 token；刷新路径则严格失败。`LoginExpire` 和 401 两边都是 AuthRequired。
- JSON 数组只建第一张可用卡。
- 浏览器 dev mock 在 `catalogId=qoder` 且未指定 flow 时返回无 user code 的 remote-poll，避免预览成设备码面板。
- 货币用 USD，和其他 IDE OAuth 行一致。
