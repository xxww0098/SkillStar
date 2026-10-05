# Accounts（多账号管理）

状态：active

Accounts 是顶层「账号」工作台：按平台管理多个登录账号、切换真实 CLI/IDE 正在使用的凭证。参考 cockpit-tools 的账号页模式，数据与凭证机制全部复用 Usage 域（订阅即账号），本 feature 只拥有管理布局与入口。

## 布局

- 侧栏 `AccountsNav`：平台目录 + 每平台账号计数 + 快速「添加账号」（带当前筛选进入新建对话框）。
- 页面 `AccountsPanel`：管理头（额度刷新、账号隐私开关）→ 花费/排序条 → 告警条 → 账号卡片网格 → 新建/编辑对话框。
- 账号卡片 = Usage 域的 `SubscriptionCard`：当前账号徽标（以 CLI custody reconcile 的磁盘真相为准，不是 active pin）、切换、重新同步到 CLI、编辑、删除、刷新。
- 「当前」徽标三态：`LinkedTo`（CLI 正在服务这个账号）/ `Diverged`（CLI 在服务一个不认识的登录）/ `Missing`。pin 只是缓存，reconcile 会修复。

## 添加账号

对话框按平台 auth mode 提供 OAuth 登录（浏览器回调 / 粘贴码 / 设备流，按平台而异）、API Key、Cookie、Token 导入、从本机导入。同一平台重复「添加」= 新账号行；对已有卡片重新授权 = 原位更新，不产生重复行。

## Claude Code

- 绑定：只读采用 Claude Code 自己的登录（macOS 钥匙串 / 其它平台凭证文件），登录路径不写回。
- 切换：对权威存储做 read-modify-write——只替换 `claudeAiOauth` 键、保留 `mcpOAuth` 等（macOS 钥匙串项缺失时先从明文文件迁移兄弟键）、回读校验通过才移动 pin；`adopt_before_refresh` 先吸收 CLI 轮转出的新 token。macOS 写钥匙串项 `Claude Code-credentials` 是 [D-083](../../decisions.md) 对 D-072 的定点豁免（唯一允许的钥匙串写入），验证成功后删除过期的明文镜像文件；其它平台写 `$CLAUDE_CONFIG_DIR/.credentials.json`。

## 边界

- 数据层零新增：全部走既有 IPC（订阅 CRUD、`set_active_subscription`、`switch_active_subscription_to_cli`、`reconcile_cli_accounts`、导入命令），命令面归 `usage_commands`。
- 跨 feature 依赖只经过 `src/features/usage/index.ts` 公开出口（`check_feature_imports.sh` 守卫）。
- Usage 工作台是只读消费视图；一切账号增删改切在 Accounts 完成。

## 验证

`src/features/accounts/`（面板/侧栏组件与测试）、`crates/skillstar-usage/src/usage_switch.rs`（切换引擎与平台注册表）。账号切换的事务契约（失败保留旧账号、pin 只在读回成功后移动）见 [usage README](../usage/README.md)。
