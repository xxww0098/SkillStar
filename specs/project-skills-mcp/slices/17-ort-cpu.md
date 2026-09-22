# 17 — ort CPU

## 契约

`ort` 能在 CPU 上跑通一张随仓库提交的小图。Laya 权重不在时，推荐顺序与 `PassthroughReranker` 相同，推荐仍然成功。

## 缝

只在本档执行：

```bash
cargo add ort --package skillstar-app
```

然后把 `skillstar-app` 对 `ort` 的依赖改成 `default-features = false`，只留当时文档里编出 CPU Execution Provider 的 feature。不要启用 `cuda`、`coreml`、`directml`、`tensorrt` 或其他硬件 EP。三平台同一条 `Session` 构建代码。需要显式注册 EP 时只注册 CPU。

`OrtCpuReranker` 实现 10 档的 `SkillReranker`。`SKILLSTAR_LAYA_ONNX` 未设置，或目录里缺少 `laya.onnx` 时，行为等于直通。文件打不开或 `Session` 失败时同样直通，推荐返回成功。不下载，不把权重打进安装包。

推荐编排在直通与 ORT 之间只差这一处选择。计划哈希的输入不变。

随测试提交一张最小 ONNX，放在 `crates/skillstar-app/tests/fixtures/ort-cpu-identity.onnx`。测试用 CPU `Session` 跑一次固定输入。这张图不是 Laya，只证明运行时在本仓库的目标平台上能加载。

## 人可以运行

```bash
cargo test -p skillstar-app ort_cpu_
```

不设置 `SKILLSTAR_LAYA_ONNX`。再跑一次 10 档的推荐测试，确认默认顺序没变。

## 验证

- `ort_cpu_session_runs_the_checked_in_identity_graph`
- `missing_laya_onnx_keeps_bm25_order_and_recommend_succeeds`
- `ort_ranker_does_not_change_plan_hash_inputs`
- `session_builder_registers_only_the_cpu_execution_provider`

`cargo check --workspace --locked` 在当前平台通过。CI 的 Windows 与 Linux 任务也必须能编过；编不过就停在本档，01 到 16 仍是完整的 BM25 方案。

在 `docs/features/project-skills-mcp/README.md` 写明：PyTorch 只出现在导出说明里，应用依赖没有它。

## 可改

身份图是「单输入复制到输出」还是「两浮点相加」，只要图很小、不需要外部权重、CPU 上可重复。

## 不可改

不按操作系统切换 EP 或模型文件。不把直通做成错误。不在本档实现 Laya 的 tokenizer 或 `marker_pos` 打包。

## 必须保持绿

10 的直通推荐测试。`cargo test -p skillstar-app --locked` 不访问网络。

## 会改这一档的反馈

为了链接成功打开了硬件 EP，或模型缺失时推荐失败。
