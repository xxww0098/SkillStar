# 技能自带 Hook：落盘格式调研（冻结）

状态：historical

`skillstar-skills::skill_hooks` 从未被任何 Tauri command、CLI 子命令或前端调用触达，已在 2026-08-27 的死代码清理中删除（代码可由 `git show 5a288a7 -- crates/skillstar-skills/src/skill_hooks.rs` 取回）。

本文只保留当时实测得到、重新推导成本很高的落盘事实与设计裁决；它**不是**当前功能契约，`docs/features/skills/README.md` 不再描述该面。若日后重新实现，先复核下列实测结论是否仍成立。

设想中技能的第二种载荷：技能目录下的 `hooks/hooks.json` 使用 Claude Code hook 格式，由一个模块统一写入各 Agent 的落点。

- **一份载荷，不做翻译。** Claude 的 `~/.claude/settings.json` 的 `hooks` 与 Codex 的 `~/.codex/hooks.json` 的 `hooks` 使用同一套 PascalCase 事件名和同一种条目结构（`{matcher?, hooks: [{type, command, timeout?}]}`），只有落点不同，因此注册表只记录「哪个 Agent、哪个文件」两列。Codex `config.toml` 里的 `pre_tool_use` 一类 snake_case 是信任账本键，不是事件名，不得当作第二种方言。
- **Agent 入表的门槛是实测。** 只有其 hook 文件被真实读取、事件结构确认与上述格式一致的 Agent 才加一行；凭厂商文档加行会写出永不触发的 hook。当前两行：`claude`、`codex`。
- **写入只是提议，不等于启用。** Codex 按 `[hooks.state."<文件>:<事件>:<索引>:<索引>"].trusted_hash` 逐条记录信任，未被信任的条目不执行（绕过需要 `--dangerously-bypass-hook-trust`）。Hook 是可执行载荷而非惰性 Markdown，因此不得随技能安装静默下发，必须是逐技能的显式用户动作。
- **归属零状态推导**，与 [decisions.md](../decisions.md) D-024 同源：不写 sidecar 台账，命令必须经 `${SKILL_DIR}`（同时接受生态既有的 `${CLAUDE_PLUGIN_ROOT}`）展开为技能绝对路径，归属判据是「命令中出现该技能目录，且其后紧跟路径分隔符或字符串结尾」。判据是「后接分隔符」而非「后一个字符不像文件名」：POSIX 文件名几乎允许所有标点，排除法只是一张想得起来的字符清单，`…/skills/foo+bar/run` 会被 `foo` 误认领。技能目录的尾部分隔符在比较前归一，否则同步传 `…/foo`、卸载传 `…/foo/` 会让条目变成删不掉的孤儿。
- **写入即可移除**是不变量，两条 fail-closed 门禁守它：展开后不满足上述判据的命令拒绝写入；不含任何 `command` 的条目（空 `hooks` 数组或全是非 command handler）同样拒绝——它能被写进去却永远判不出归属。声明为空数组的事件不创建键，否则该键无主、清理时也不会被剪掉。
- **就地覆写而非追加**：信任账本的键含条目索引，移动邻居条目会静默作废其 `trusted_hash` 并让用户为已审过的 hook 重新授权。等量重同步不移动任何外部条目；增删条目仍会重编号，属已知上限（见模块内 `ponytail:` 注释）。
- 目标 Agent 的配置目录不存在时拒绝写入，不代为创建——否则 SkillStar 会让探测方误认为该 Agent 已安装。
- 同步报告返回实际写入的事件名。Agent 会静默忽略不认识的事件键，因此「写了什么」必须如实回报，不能让永不触发的 hook 看起来像安装成功。**不做 per-Agent 事件白名单过滤**：各 Agent 事件集确实不同（Claude 有 `PostToolUseFailure`/`StopFailure`/`TeammateIdle`，均不在 Codex 的 hook 迁移事件表中），但 Codex 公布的表同时漏掉了其信任账本证明会执行的 `Stop`，据此构造白名单会拒掉能用的 hook。误拒比如实回报一次空转更糟。
