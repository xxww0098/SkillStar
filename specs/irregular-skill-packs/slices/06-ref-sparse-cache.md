# 06 带 ref 的缓存也走稀疏检出

**先决条件：00 的 P0b 通过。** 如果 P0b 不通过，在 README 的 TODO 里把本档标为「不做」，在决策条目的「后果」里写上「带 ref 的 tree URL 会整仓浅克隆」，然后结束本规格。

## 解锁什么

用户在网页上复制的 tree URL 都带 ref（`/tree/main/...`）。这类 URL 安装时只下载子路径，不再整仓浅克隆 3000 多个文件。决定 4 里「只物化该子路径」到这里才在下载量上兑现；04 已经保证了结果正确。

## 现状

- `repo_scanner/cache.rs:196-203` 碰到带 ref 的新缓存，一律走完整浅克隆，存为独立缓存 `…--ref--…`。`:198` 的注释说理由是「要先有文件才能套 subpath filter」。

## 接缝

- 把 `clone_sparse_with_skills` 泛化为 `clone_sparse(repo_url, dest, git_ref: Option<&str>, extra: &[String])`，流程是：
  1. treeless 克隆；
  2. `sparse-checkout init --cone`；
  3. `fetch_and_reset_ref`；
  4. `inventory::apply(extra = [subpath])`。
- 同一个 ref 缓存可能被多个子路径共用，所以 extra 用 add（取并集），不用 set。02 的粘滞规则本来就保证只增不减。
- 失败时沿用 tarball 回退。`rebuild_cache_from_tarball` 已经能接收 `git_ref`。
- 删掉 `:198` 那条旧理由注释。

## 测试

命令：`cargo test -p skillstar-skills --locked ref_pinned_cache`

- `ref_pinned_cache_is_sparse`：在带 ref 的 partial 夹具上，工作区里没有 `crates/`，也没有其他 harness 的副本。
- `tree_url_install_never_materializes_outside_subpath`

必须保持绿：`deleted_lock_ref_does_not_block_install_and_retargets_to_default_branch`。

## 文档

- 决策条目补上「带 ref 的缓存同样稀疏检出，只拉 inventory 加子路径」。
- `docs/features/skills/README.md:41-42` 中 ref 缓存的描述。

## H5 审查点（真网络，不阻塞）

```bash
GIT_TRACE=1 skillstar add https://github.com/pbakaus/impeccable/tree/main/.claude/skills/impeccable -g -y
du -sh $SKILLSTAR_HUB_DIR/repos/*impeccable*--ref--*; git -C <cache> ls-files | wc -l
```

把数字写进 `choices.md`，与 00 的基线对比。

## 可改

- `clone_sparse` 的参数形状。
- ref 缓存的目录命名，沿用现有规则即可。

## 什么反馈会改变本档

- P0b 在 mirror 上不稳定：只对直连启用本档，mirror 继续走完整浅克隆，并在 `choices.md` 里记一笔。
