# 18 — Laya 图（休眠）

不要在 01 到 17 做完之前开工。不要在 `SKILLSTAR_LAYA_ONNX` 未指向本机目录时开工。缺模型不是失败，回到 17 的直通即可。

## 契约

当目录里有 `laya.onnx`、`laya.onnx.data`、`laya_config.json` 和 `tokenizer/` 时，`OrtCpuReranker` 用这张图对至多 12 个候选分别打分，并按分数重排。它仍不增删候选，不写 selection、计划或批准。

中文任务只有在该目录是 multilingual 导出时才允许加载。英文 `receptron/laya-onnx` 不能冒充 multilingual。配置里看不出语种时拒绝加载并直通。

## 缝

打包规则抄 [receptron/laya](https://github.com/receptron/laya) 的导出与加载，不另发明输入布局。图的输入名是 `input_ids`、`attention_mask`、`marker_pos`、`marker_mask`、`qtype`。

每个候选单独提问「该技能是否适用于这次任务」，用 `noul` 或等价的 yes/no 头。禁止把多个技能名塞进同一次 `choice`。候选描述只用名称和 frontmatter 描述，不送 `SKILL.md` 正文。

参考权重是 Hugging Face `receptron/laya-onnx`（英文，约 1.7GB external data）。multilingual 没有现成 ONNX。需要中文时，用上游导出脚本把 `convaiinnovations/laya` 的 `multilingual` 子目录导出成同一输入接口，路径仍由 `SKILLSTAR_LAYA_ONNX` 指向。导出用的 PyTorch 不进 workspace。

## 人可以运行

```bash
SKILLSTAR_LAYA_ONNX=/path/to/export cargo test -p skillstar-app -- --ignored laya_graph_
```

没有这个环境变量时，忽略测试不要让 `cargo test` 失败。

## 验证

- `laya_graph_reranks_without_changing_the_candidate_set`
- `laya_graph_refuses_english_bundle_for_chinese_query`
- `laya_logits_match_the_onnx_runtime_reference_within_1e_4`

第三条用一条冻住的英文 token 向量。阈值相对 ONNX Runtime 参考输出，不是相对重新跑起来的 PyTorch。上游导出与 PyTorch 的差大约是 1e-5，不在应用测试里复现。

stdout 在 `RUST_LOG=trace` 下仍只有协议帧。加载放在第一次推荐，不放在 `initialize`。

## 可改

分数从高到低时，平分的稳定次序沿用 BM25 原序。

## 不可改

不下载权重。不把分数写进 `plan_hash`。不让低分删掉候选。加载失败则直通。

## 必须保持绿

17 在未设置环境变量时的全部测试。

## 会改这一档的反馈

动态 shape 或算子在 CPU 上跑不通，或 logits 超出 `1e-4`。出现时把本档标成未接入，17 继续直通。不要为了跑通改用 GPU EP。
