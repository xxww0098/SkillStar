# 03 — 项目写锁

## 契约

所有会改 `projects.json`、`skills-list.json` 或项目技能目录的公开入口，在同一次调用期间持有 `state/project-write.lock`。宽松函数的返回值与写入内容不变。

## 缝

`skillstar_skills::projects::write_lock::with_project_write_lock`。

同一线程可重入：线程局部深度，只打开一个 fd。Linux 的 `flock` 按 open-file-description 计，同一进程再打开第二个 fd 会自锁。其他线程先抢进程内 mutex。mutex 中毒返回错误，不用 `into_inner`。

在入口持锁，而不是只包住 `save_skills_list`。`save_and_sync` 写完清单之后还会 `full_sync` 清链。

这些入口要持锁：`save_skills_list`、`save_skills_list_only`、`save_and_sync`、`add_skills_to_project_with_mode`、`full_sync`、`remove_skill_from_all_projects`、`import_scanned_skills`、`rebuild_skills_list_from_disk`、`update_project_path`、`refresh_stale_copies`。

不复用 `state/skill-update.lock`。`import_scanned_skills` 进入时已经持有更新锁。锁序只有「更新锁 → 项目锁」。禁止在持有项目锁时再拿更新锁。

## 人可以运行

`cargo test -p skillstar-skills project_write_lock_`

## 验证

- `project_write_lock_excludes_a_second_thread`
- `project_write_lock_reenters_on_the_owner_thread`
- `save_and_sync_holds_the_lock_across_full_sync`
- `import_can_take_project_lock_while_holding_update_lock`

第二进程或第二线程在持锁期间 `try_lock` 失败。Windows 上文件锁必须真的挡住另一个进程；只靠进程内 mutex 不算通过。

## 可改

守卫类型是否公开。`try_lock` 是方法还是同模块测试可见函数。

## 不可改

锁文件路径、单 fd 重入、锁序、不改变宽松部署的成功计数。

## 必须保持绿

`add_skills_to_project_does_not_create_dirs_for_empty_or_missing_skills` 与 `add_skills_to_shared_universal_path_uses_one_owner_and_honors_copy_mode`。

## 会改这一档的反馈

与 `save_and_sync` 不互斥，或 import 路径同线程死锁。
