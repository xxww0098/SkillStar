# 09 — 严格启用

## 契约

在已持有的项目写锁里，把一组 Hub 技能以目录链接增量放到一个已注册项目。预检失败则零写入。不走宽松部署。

## 缝

`skillstar_skills::projects::strict::enable_project_skills_strict`。

入参：已注册的 `ProjectBinding`、一个 `agent_id`、技能名与期望 `content_hash`。函数自己再取 `shared_path_owner` 和 `inspect_project_skills`。未注册的 binding 直接拒绝，函数内部不调用 `register_canonical_project` 或 `register_project`。

预检，任一项失败则项目目录、清单、索引的字节都不变，并返回逐项状态：

| 状态 | 条件 |
| --- | --- |
| `missing` | Hub 中没有 |
| `stale` | `content::snapshot` 的哈希与期望不符 |
| `rejected_copy` | 该物理路径的 deploy mode 已是 copy |
| `conflict` | 真实目录，或链接指向别的 Hub 技能 |
| `already` | 链接（含 junction）已指向该 Hub 技能目录 |
| `create` | 目标不存在 |

全部只是 `already` 或 `create` 时，才对 `create` 调用 `create_symlink`。禁止 `deploy_skill_with_mode` 与 `create_symlink_or_copy`。`create_symlink` 不检查目标已存在，所以预检必须先看 `symlink_metadata`。

某次 `create_symlink` 失败：删掉本调用已经创建的链接，不写清单，返回错误。不复制。跨盘且 Windows 返回 1314 时，这就是失败。

全部链接成功之后才把新名字合并进清单，保留其他技能，不调用 `save_and_sync` 或 `full_sync`。新路径的模式只写 symlink。已有 owner 保持不变。空 `agent_id` 或空技能列表拒绝，不触发宽松函数的回退。

部署前用 `contained_child` 检查目标的已存在祖先。

## 人可以运行

`cargo test -p skillstar-skills strict_enable_`

## 验证

- `strict_enable_fails_closed_when_any_skill_is_missing_and_writes_nothing`
- `strict_enable_rejects_hash_mismatch`
- `strict_enable_rejects_copy_mode_without_converting`
- `strict_enable_does_not_replace_a_real_directory`
- `strict_enable_does_not_retarget_an_existing_symlink`
- `strict_enable_is_idempotent_when_the_link_is_already_correct`
- `strict_enable_uses_one_owner_for_agents_skills`
- `strict_symlink_failure_does_not_fall_back_to_copy`
- `strict_enable_rejects_a_link_parent_outside_the_project`

## 可改

报告结构的类型名。逐项状态用枚举，不用自由字符串。

## 不可改

不包装宽松部署。不在失败时留下本次新建链接。不把 junction 当成冲突，除非它指向别处。

## 必须保持绿

宽松增量测试仍表明缺失技能返回 `Ok(0)`、已有链接会刷新、copy 模式仍可用。那些断言留在旧函数上。

## 会改这一档的反馈

预检失败后清单或项目树有字节变化，或 symlink 失败后目录里出现了文件副本。
