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
