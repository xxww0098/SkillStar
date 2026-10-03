# 05 sessions 地基 + claude 系 parser（P2）

usage 线；无 tree 前置（skillstar-usage 新模块）。

## 解锁的契约

`crates/skillstar-usage/src/sessions/` 落地解析器 trait + 增量 checkpoint 持久化 + claude-code/claude-desktop parser（projects JSONL 家族，magpie internal/sessions/claude.go 与 calls.go:440 先例）。只读，绝不写 agent 目录。

## 接缝（签名级）

```rust
// crates/skillstar-usage/src/sessions/mod.rs —— 窄 facade 只导出这些
pub struct SessionCall {
    pub at: i64, pub agent: String, pub session: String,
    pub model_asked: String, pub model_answered: String,
    pub tokens: SessionTokens,          // usage 侧自有四元组（不依赖 gateway 类型）
    pub effort: Option<String>, pub request_id: Option<String>,
    pub error_kind: Option<String>, pub latency_ms: Option<u64>,
    pub file: PathBuf, pub from: u64, pub to: u64,   // 定位回读（magpie File/From/To）
}
pub struct FileCheckpoint {              // 落 data_root()/sessions/index.json，atomic_write
    pub version: u32,                    // 解析器版本，变则全量重读
    pub head_hash: String,               // 前 256B sha256：判「替换 vs 增长」
    pub prefix_hash: String,             // 已读前缀采样哈希
    pub size: u64, pub offset: u64,      // 最后完整行之后
    pub agent_state: serde_json::Value,  // parser 私有（claude: msgs map / last tokens）
    pub calls_seen: u64,
}
pub trait SessionParser: Send + Sync {
    const AGENT: &'static str;
    fn discover(&self, home: &Path) -> Vec<SessionFile>;
    /// 增量：从 checkpoint.offset 续读；head/prefix 校验不符 → 从零重来（截断/替换检测）
    fn parse(&self, file: &SessionFile, prior: Option<FileCheckpoint>)
        -> (Vec<SessionCall>, FileCheckpoint);
}
pub fn read_calls(home: &Path, since: Option<i64>) -> Vec<SessionCall>;
```

claude 解析要点（magpie 实证教训，全要测试钉死）：

- 同 message id 多 block：后块 usage 覆盖前块、From 取首行（magpie claude.go:210-222 `ccCount`）；
- 跨文件 message-id 去重：resumed 会话拷旧文件内容，同 msg id 只计一次（最早文件优先）；
- 超大文件：行头嗅探再决定是否全行 decode（性能模式可照抄）；
- `SKILLSTAR_TOOL_SYNC_HOME` 尊重（`$CLAUDE_CONFIG_DIR`）。

claude-desktop：验证 Code 标签会话的实际路径；与 claude-code 同 parser 不同 discovery；验证失败则 desktop 出首期（记 choices，不阻塞）。

## 人能看/跑什么

```bash
cargo test -p skillstar-usage --locked sessions
# fixtures: tests/fixtures/sessions/claude/projects/...（从 magpie testdata 改写 + 合成边界样本）
# 增量测试：append 一行只产 delta；截断/替换（head hash 不符）→ 全量重读
# 人工：对本人 ~/.claude/projects 抽样对拍 token 数量级
```

## 必须保持绿

usage crate 全量既有测试（不碰 usage/service.rs、custody_tests.rs——后者正被 tree-08 拆）。

## 委托给实现者的自由

解析器内部结构；checkpoint 文件布局（每 agent 一文件或合一）；SessionTokens 字段命名。

## 会改变本片的反馈

用户若要求会话标题/摘要也进视图 → SessionCall 已有 file 定位，标题提取并入本片 discovery，不另立。
