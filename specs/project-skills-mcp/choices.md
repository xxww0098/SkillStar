# 选择

实现过程中规格没有写死、由代码决定的事。按档追加。银行里的条目不再重列，除非收尾时对照最终代码重写本文件。

## 01 stdio

### 已定，按这个做

- **进程入口先看 `argv[1] == "mcp"`，再看 askpass。** 判断函数放在 `skillstar_app::project_skills_mcp::is_mcp_invocation`，`main` 第一件事调用它。这样单测能证明 askpass 环境变量不会吞掉 `mcp`，而不必先编出桌面二进制。
- **依赖提前到本档。** 规格把 `cargo add rmcp` 写在 13 档，但 01 档要求 stdio 服务器就是 rmcp 3.4。版本在根 `Cargo.toml`，feature 用 13 档的列表。
- **测试传输多开 `transport-async-rw`。** 规格点名的 feature 没有这项。进程内 duplex 需要它，它不是 HTTP、client 或硬件后端。生产路径仍是 `rmcp::transport::stdio`。
- **`2026-07-28` 的 initialize 响应版本是 `2025-11-25`。** 这是 rmcp 的协商，不是我们改写的协议。stdout 仍然只有 JSON-RPC。

### 先这样，以后可改

- **Windows release 管道还没看过。** 当前机器是 macOS。失败时停在 01，不改 `windows_subsystem`。

## 02 绑定

### 已定，按这个做

- **两条活记录落到同一规范路径时直接返回错误，`ObservedProject.ambiguous` 在成功结果里恒为 false。** 规格要求结构里有这个状态，也要求此时不写索引。错误字符串含 `ambiguous`，调用方不用再看那个布尔值。
- **macOS 夹具优先用 `/tmp` 与 `/private/tmp`。** 这台机器上 `/tmp` 是符号链接。若某环境没有这种根，测试改在临时目录里造一条等价符号链接。比较的始终是 canonicalize 之后的路径。
- **Windows 的越界夹具用已有的 `junction` 依赖造 junction，不新增 crate。** `\\?\` 前缀不单独比较；两边都先 canonicalize 再比。

## 03 写锁

### 已定，按这个做

- **入口函数自己拿锁，不把函数体再缩进进闭包。** `with_project_write_lock` 和 `lock_project_write` 是同一把锁。长函数用守卫，避免把几百行包进一个闭包。
- **第二把文件描述符上的 `try_lock` 失败，用来证明不是只靠进程内 mutex。** 同进程第二个线程也会失败。没有另起一个 Windows 进程；文件锁 API 就是 `File::try_lock`。
- **`refresh_stale_copies_strict` 也走这把锁。** 规格点名的是公开的 `refresh_stale_copies`。严格变体写的是同一份清单和目录，所以锁在内部函数上。

## 04 owner

### 已定，按这个做

- **DeepSeek 多读的目录记在 agent 定义旁的一张表里，不写进冻结的 8 字段 profile。** 表只有 `(deepseek, .agents/skills)`。披露名单会带上这张表，部署仍只写选中 Agent 自己的相对路径。
- **没有现有 owner、选择的 Agent 又是空字符串时，函数返回 `owner_id: None`。** 宽松部署仍在调用前做自己的空列表回退，不把那个回退搬进这个函数。

## 05 事实

### 已定，按这个做

- **没有清单项、目录也不存在的 Agent 路径不产生空行。** 有子目录或清单名字时才出现一行。未注册项目因此得到空事实，并且不创建 `projects.json`。
- **普通文件不是技能。** 技能目录里的 `notes.txt` 和项目根的其他文件都不进事实。只看目录、符号链接和 junction。

## 06 检索

### 已定，按这个做

- **检索复用原来的 `score_docs`，但语料只有技能文档，邻居列表是空的。** 这样不会写 `recall_events`，learning 也不进 IDF。`recall()` 仍走完整语料。
- **`limit` 夹在 1 到 12。** 空查询或只有停用词时返回空列表，不报错。

## 07 计划

### 已定，按这个做

- **哈希字段用长度前缀加一个 0 字节分隔，不是裸拼接。** 这样技能名里的字节不会和后面的哈希粘在一起。域前缀仍是 `skillstar.project-skill-plan.v1\0`。
- **读计划时重算哈希，不一致就拒绝，并且不改文件。** 规格只要求过期不改文件。内容被改过也按同样方式拒绝。
- **技能名和受影响 Agent 在写入前排序。** 哈希和文件用同一顺序。

## 08 批准

### 已定，按这个做

- **同一来源、同一哈希再次写入时返回文件里的原记录，不改写入时间。** 另一来源或另一哈希直接拒绝，文件字节不变。
- **计划 id 只接受十六进制和连字符。** 这和 UUID 文件名一致，避免路径跑出批准目录。

## 09 严格启用

### 已定，按这个做

- **函数内部再次拿项目写锁。** 调用方按规格已经持锁。锁可重入，所以直接调用的测试也不会和别的写入交叉。
- **预检不通过时返回 `Ok`，`committed` 为 false，并带上每一项状态。** 链接中途失败才是 `Err`，并且删掉这次新建的链接。
- **符号链接失败的夹具把 `.agents` 写成普通文件。** 创建目录失败，目录里不会出现技能副本。

## 10 推荐

### 已定，按这个做

- **预检状态复用 `classify_project_skill`，不在推荐里再写一套。** 推荐另外会调用 `inspect_project_skills`，但分类结果以严格启用的那个函数为准。
- **候选描述只取 frontmatter 的 description。** 没有 description 时摘录是空字符串，不用正文补。
- **计划里的 `scores` 写成空数组。** 分数可以出现在候选上，不抄进计划。重排器名字会记在计划里，但不进哈希。
- **推荐函数接收 `now`，用来计算计划过期时间。** 测试不用墙上时钟。

## 11 查询

### 已定，按这个做

- **加载提示是「相对路径 + 一句未验证说明」。** 路径用正斜杠，指向项目里的 `SKILL.md`，不指向 Hub 绝对路径。
- **`RuntimeVisibility` 只有 `Unverified` 一个值。** 没有布尔开关。

## 12 应用

### 已定，按这个做

- **没有批准时返回 `Ok(ApprovalRequired)`，不是错误。** 项目树和项目索引都不改。调用方能把它和部署失败分开。
- **应用前再对 Agent 的物理路径和 owner。** 计划里的路径必须等于该 Agent 的 `project_skills_rel`，owner 必须等于当时的 `shared_path_owner`。对不上就停，不建链接。
- **回执复用查询的 `RuntimeVisibility`，序列化成 `unverified`。** 不另做一份可见性枚举。
- **Windows 测试用 workspace 里已有的 `junction` 做目录别名。** 只加在 `skillstar-app` 的 `cfg(windows)` dev-dependency 上。生产代码不依赖它。

## 13 协议工具

### 已定，按这个做

- **未知字段返回 `isError: true` 的工具结果，不是 JSON-RPC error。** rmcp 3.4 把参数反序列化失败包在这次 `tools/call` 的结果里。正文含 unknown field。结构化结果不出现，领域函数不跑。
- **工具结果不带 Hub 绝对路径。** 查询事实里的 `link_target` 留在领域类型，不放进协议结果。磁盘种类仍用 `SkillDiskKind`。
- **短文本用英文。** 结构化字段才是契约。句子可以以后改。
- **`schemars` 作为 `skillstar-app` 的直接依赖，版本 1.2.1，与 rmcp 相同。** `JsonSchema` 派生宏按 crate 名找 `schemars`，只靠 rmcp 的 re-export 编不过。没有再次 `cargo add rmcp`，`transport-async-rw` 保留。
- **推荐结果里的计划包含确认所需的差异，不包含技能正文或内容哈希。** `plan_id` 和 `plan_hash` 都在。应用参数只有 `plan_id` 和 `idempotency_key`。
