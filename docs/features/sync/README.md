# SSH 远端同步

状态：active

本文件维护 SSH 远端技能传输、凭证边界、进度事件和恢复语义。技能本地安装/部署规则见 [../skills/README.md](../skills/README.md)。

> S3 云同步已移除（2025）：跨设备/组织协作统一走 GitHub 共享频道（`skillstar-channels`），SSH 保留为个人服务器部署路径。决策记录见 [../../decisions.md](../../decisions.md)。

## 所有权

- `skillstar-sync::ssh` 拥有 SSH dial/auth/TOFU、SFTP、远端 discovery、hub 操作和 host store。
- `src-tauri/src/commands/ssh_hosts/` 只注入事件 sink、State 和 DTO。
- Sync 可以消费 `skillstar-skills` 的公开安装/skill contract，不反向拥有本地技能规则。

## SSH 凭证与 Host Key

- 非敏感 host metadata 写 `~/.skillstar/config/ssh_hosts.toml`。
- 密码和 passphrase 存系统 keyring，兼容服务名固定为 `skillstar-ssh`；不能改成模块路径字符串，否则已有凭证不可见。
- 连接分为 dial/handshake → host-key gate → authenticate。未信任或 mismatch 主机（包括连接测试流程）不得收到认证材料；测试流程在密钥接受前只返回指纹与延迟，接受密钥后重试才发送密码/私钥。
- 已接受 fingerprint 存 `ssh_known_hosts.json`；mismatch fail closed。
- 系统 `~/.ssh/config` host 只读发现，导入后才写 SkillStar store。

## 连接和远端路径

- keepalive、inactivity、dial retry、SFTP open 和远端 exec 均有有界超时。
- shell home 使用 `"$HOME"`；SFTP 不展开 `~`，因此 session 建立后用 `canonicalize(".")` 得到绝对 home。
- skill name 进入 shell 前必须校验，拒绝 `/`、`..`、`~` 和换行。
- mutating script 使用 `set -e` 并检查远端 exit status；echo marker 不能代替成功状态。
- 远端 Git 禁止交互 prompt，失败快速返回，而不是占满 exec timeout。
- destructive delete 拒绝 root、home、整个 skills dir 和 hub content root。

## 远端 Hub 与 Discovery

- UI push 先把本地技能上传到 `$HOME/.skillstar/hub/content/<name>`，再为目标 Agent 建 link。
- 每个命令只打开一次 SFTP channel；批量 push 复用该 channel，并逐项收集错误。
- discovery 扫描 `$HOME/.*` 下的 skills 目录并识别普通目录和 hub symlink；固定路径表只作为新服务器无发现时的 seed。
- 旧的字面 `$HOME/~/.skillstar` 布局由幂等 heal script 搬回真实 hub 并重指 link；修复数量通过 progress event 报告。
- standalone 技能可以迁移进 hub。远端技能的 pull、link toggle 和 update check 没有实装：命令层曾有包装但前端从未接线，已于 2026-08-27 删除，需要时从 git 历史取回并补 UI。

## SSH 事件与 UI

- 每次命令有唯一 `session_id`，事件 phase/status 由 `SshProgressEvent` 定义。
- console 显示 dial、handshake、host_key、auth、sftp、scan、done/error；host-key pending 在 console 内暂停并提供 trust action。
- My Skills remote scope 复用 `SkillGrid`/`SkillCard` 展示，但 host/console/push/migrate/delete 状态归 remote content 自己所有。
- `src/features/ssh/` 拥有 host CRUD、连接事件 hook 与远程 mutation/query 公共接口；`src/features/my-skills/remote/` 拥有 remote content、Skill 投影、详情抽屉和迁移/删除交互。依赖方向固定为 `my-skills → ssh`，禁止 SSH UI 回读 My Skills 私有组件。

## 验证

```bash
cargo test -p skillstar-sync
bun run test -- src/features/ssh
```
