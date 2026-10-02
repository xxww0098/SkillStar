# 00 探针与夹具

## 解锁什么

两件事：
- 用真网络的数字，决定 02 用批量预取还是逐个懒取、06 做不做。
- 做一个离线、确定性的 impeccable 形态夹具，01–06 的测试和 CLI 探针都以它为准。

本档不改产品代码。

## 探针（人工跑，在 scratchpad 里做）

**P0a：批量取 `SKILL.md` blob**

```bash
git clone --filter=blob:none --depth 1 --no-checkout --sparse https://github.com/pbakaus/impeccable imp && cd imp
git ls-tree -r HEAD | awk '$4 ~ /\/SKILL\.md$/ {print $3}' | sort -u > oids; wc -l < oids
ls .git/objects/pack/*.promisor | wc -l          # 基线
time git -c fetch.negotiationAlgorithm=noop fetch --no-tags --no-write-fetch-head \
  --recurse-submodules=no --filter=blob:none origin $(cat oids)
ls .git/objects/pack/*.promisor | wc -l          # 期望只多 1 个
```

再做两组对照：
- 换一份新克隆，逐个 `git cat-file -s <oid>`，记录总耗时。
- 用用户配置的 mirror 重跑一遍：`-c url.<mirror>/https://github.com/.insteadOf=https://github.com/`。

**P0b：带 ref 加子路径的 treeless 克隆**

```bash
git clone --filter=blob:none --depth 1 --no-checkout --sparse https://github.com/pbakaus/impeccable imp2 && cd imp2
git fetch --depth 1 origin main && git reset --hard FETCH_HEAD
git sparse-checkout set .claude/skills/impeccable
ls .git/objects/pack/*.promisor | wc -l; git ls-files | wc -l; find . -path ./.git -prune -o -type f -print | wc -l
```

**P0c：tarball 大小**

`curl -sI https://codeload.github.com/pbakaus/impeccable/tar.gz/HEAD`

**判定：**
- **P0a 通过**：批量一次往返，直连 < 3 秒；mirror 上成功，或者干净地失败后回退到直连。通过则 02 实现 `prefetch_blobs_in_session`。
- **P0a 不通过**：02 按不同的 blob SHA 逐个懒取，总时限 15 秒；超时就让这一组退回旧规则。
- **P0b**：工作区里只有子路径的文件，而且没有拉取子路径以外的 blob，06 才做；否则 06 标记为不做，把「带 ref 的 URL 会整仓下载」记为已知上限。

数字和结论写进 `choices.md` 的 00 节。

## 夹具接缝

新文件 `crates/skillstar-skills/src/pack_fixture.rs`，在 `lib.rs` 里用 `#[cfg(test)] mod pack_fixture;` 引入。

```rust
pub(crate) const HARNESS_DIRS: &[&str];    // 每个 <dir>/skills/impeccable
pub(crate) const PLUGIN_COPIES: &[&str];   // plugin/、cursor-plugin/
pub(crate) const DECOY: &str;              // .windsurf/skills/impeccable，name 不同
pub(crate) const TEST_FIXTURES: &[&str];   // tests/ 下的夹具技能
pub(crate) fn published_copies() -> Vec<String>;
pub(crate) struct PackFixture { pub dir: TempDir } // 已 commit，可被 file:// partial clone
pub(crate) fn impeccable_like() -> PackFixture;
pub(crate) fn build_impeccable_like(root: &Path);
pub(crate) fn git(repo: &Path, args: &[&str]) -> String;
```

夹具内容：
- 发布副本：`.agent .agents .claude .cursor .dsh .gemini .github .grok .kiro .opencode` 下的 `skills/impeccable`，加上 `plugin/skills/impeccable`、`cursor-plugin/skills/impeccable`。
  - frontmatter 全部是 `name: impeccable`。正文各写一行自己的 `.<h>/skills/impeccable/scripts` 路径，保证 tree SHA 两两不同。
  - `.cursor` 版去掉 `user-invocable`，`.agents` 版把 version 放进 `metadata`。
  - 每份副本都带 `scripts/impeccable`，权限 0755。
- **陷阱副本**：`.windsurf/skills/impeccable`，`name: impeccable-classic`，用来测「basename 相同、name 不同」。
- 测试夹具：`tests/oracle/workspaces/ctx-pin/{.agents,.claude,.cursor}/skills/impeccable`（`description: fixture`），以及 `.../.claude/skills/audit`。
- `skill/SKILL.src.md`：不是技能。
- `.claude-plugin/marketplace.json`：`{"plugins":[{"name":"impeccable","source":"./plugin"}]}`；`.claude-plugin/plugin.json`：`{"skills":"./.claude/skills/"}`。
- `plugin/hooks/hooks.json`、`plugin/agents/a.md`。
- 重型的非技能内容：`crates/engine/src/lib.rs`。
- 远端配置 `uploadpack.allowFilter=true` 和 `uploadpack.allowAnySHA1InWant=true`，让 `file://` 克隆成为真正的 partial clone。仓库里目前没有任何测试这样配置。

这些 git、write、commit 小工具，和 `inventory/tests.rs:5-39`、`skill_install_harness_tests.rs:81-122` 里的是重复的。本档先放进 `pack_fixture.rs`；旧测试等后面的档改到时再顺手迁过来，不单独搬。

`#[ignore] fn write_impeccable_fixture()`：把夹具写到 `$SKILLSTAR_FIXTURE_OUT`，供下面的 CLI 探针使用。

## 离线 CLI 探针（每档都可以复用）

```bash
T=$(mktemp -d)
SKILLSTAR_FIXTURE_OUT=$T/remote cargo test -p skillstar-skills --locked write_impeccable_fixture -- --ignored
printf '[url "file://%s"]\n\tinsteadOf = https://github.com/pbakaus/impeccable.git\n' "$T/remote" > $T/gitconfig
E="env HOME=$T/home USERPROFILE=$T/home SKILLSTAR_DATA_DIR=$T/data SKILLSTAR_HUB_DIR=$T/hub \
   SKILLSTAR_TOOL_SYNC_HOME=$T/tool GIT_CONFIG_GLOBAL=$T/gitconfig GIT_CONFIG_NOSYSTEM=1"
$E cargo run -q -p skillstar -- install https://github.com/pbakaus/impeccable.git --agent cursor -g -y
find $T/hub/repos -name SKILL.md | sort
jq '.skills[] | {name, source_folder, pinned}' $T/hub/lock.json
```

- `insteadOf` 的键必须带 `.git`：它是前缀替换，不带 `.git` 会被替换成 `remote.git`。
- 本地 `file://` 证明不了网络上的节省，那由 H2 在真实仓库上验证。
- 在**当前 main** 上先跑一次，把基线记进 `choices.md`：预期 cache 里 impeccable 的 `SKILL.md` 大约 13 份。
- CLI 子命令的确切写法以 `crates/skillstar-app/src/cli/install.rs` 为准。

## 验证

`cargo test -p skillstar-skills --locked pack_fixture::`，其中：
- `impeccable_fixture_matches_upstream_shape`：注册表和磁盘一致；tree SHA 两两不同。
- `fixture_remote_serves_partial_clone`：`--filter=blob:none` 克隆后，promisor pack 存在。

## 可改

- 夹具里具体用哪些 harness 目录，只要覆盖 `.agent`、`.agents`、`.cursor`、`.gemini`、`.dsh`，外加 plugin 两份。
- 小工具的命名。

## 什么反馈会改变本档

- P0a 或 P0b 的结果：直接改写 02 和 06 的实现路线。
