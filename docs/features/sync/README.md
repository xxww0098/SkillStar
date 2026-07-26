# SSH 与 S3 Sync

状态：active

本文件维护远端技能传输、凭证边界、进度事件和恢复语义。技能本地安装/部署规则见 [../skills/README.md](../skills/README.md)。

## 所有权

- `skillstar-sync::ssh` 拥有 SSH dial/auth/TOFU、SFTP、远端 discovery、hub 操作和 host store。
- `skillstar-sync` 根模块拥有 S3 target、manifest、tarball 和设备状态。
- S3 调用方只消费 `skillstar-sync` crate root facade；client/store/manifest/types 等实现模块保持私有。SSH 作为较大子域保留 `skillstar-sync::ssh` 命名入口。
- `src-tauri/src/commands/ssh_hosts/` 与 `s3_sync.rs` 只注入事件 sink、State 和 DTO。
- Sync 可以消费 `skillstar-skills` 的公开安装/skill contract，不反向拥有本地技能规则。

## SSH 凭证与 Host Key

- 非敏感 host metadata 写 `~/.skillstar/config/ssh_hosts.toml`。
- 密码和 passphrase 存系统 keyring，兼容服务名固定为 `skillstar-ssh`；不能改成模块路径字符串，否则已有凭证不可见。
- 连接分为 dial/handshake → host-key gate → authenticate。除明确的“测试新凭证”流程外，未信任或 mismatch 主机不得收到认证材料。
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
- standalone 技能可以迁移进 hub；hub-managed 技能支持 pull、link toggle 和 update check。

## SSH 事件与 UI

- 每次命令有唯一 `session_id`，事件 phase/status 由 `SshProgressEvent` 定义。
- console 显示 dial、handshake、host_key、auth、sftp、scan、done/error；host-key pending 在 console 内暂停并提供 trust action。
- My Skills remote scope 复用 `SkillGrid`/`SkillCard` 展示，但 host/console/push/migrate/delete 状态归 remote content 自己所有。
- `src/features/ssh/` 拥有 host CRUD、连接事件 hook 与远程 mutation/query 公共接口；`src/features/my-skills/remote/` 拥有 remote content、Skill 投影、详情抽屉和迁移/删除交互。依赖方向固定为 `my-skills → ssh`，禁止 SSH UI 回读 My Skills 私有组件。

## S3 同步

- S3 是跨设备技能同步，不是全文件备份；目标支持任意兼容 S3 endpoint。
- target metadata 写 `s3_targets.toml`，secret access key 存 keyring 服务 `skillstar-sync`；设备 id 写 `state/sync_device.json`。
- push 枚举全部已安装技能并上传 authoritative manifest。
- git-backed skill 只记录安装来源；local skill 使用 content-addressed tar.gz，上传前 HEAD 去重。
- pull 只添加或更新用户选择的技能，不传播删除。
- manifest restore 对 hub/local entry 分别复用 Skills install 与 local adopt 语义，返回逐项 summary。
- S3 event 同样使用唯一 `session_id` 和结构化 phase/status。
- S3 target 的设置区块与表单归 `src/features/s3/` 所有，由 `src/pages/Settings.tsx` 组合；`features/settings` 不直接消费 S3 hooks。

## 验证

```bash
cargo test -p skillstar-sync
bun run test -- src/features/ssh src/features/s3
```
