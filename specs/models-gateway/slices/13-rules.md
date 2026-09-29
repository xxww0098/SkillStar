# 13 — 规则

## 契约

分组上的规则按顺序匹配：token 数、是否带图像、effort、来源 Agent。规则对象的字段是 `use`、`tokens`、`images`、`effort`、`agents`、`intent`。第一条匹配的规则决定使用哪个成员。本档的分类器不运行。带 intent 的规则在本档永不匹配，原样留给 14 档。

## 缝

纯函数：展开后的成员、规则、请求特征进，选中的成员出。规则存在分组对象里，同一份 `model_gateway.json`。

夹具从 magpie `grouprule_test.go` 的表抄，去掉 intent 用例。

## 人可以运行

```bash
cargo test -p skillstar-gateway rules_
```

## 验证

- `rules_first_match_wins`
- `rules_images_tokens_effort_agent`
- `rules_with_intent_do_not_match_yet`

## 可改

请求特征结构的字段顺序。

## 不可改

匹配顺序、本档不发分类器 HTTP、intent 规则不误伤。

## 必须保持绿

`group_` 测试。

## 会改这一档的反馈

图像规则选中了不能看图的成员，或空规则改变了 12 档的展开顺序。

## 决定

- 来源 Agent 用占位 bearer 的 `skillstar-<agent-id>` 识别，识别不了再用 User-Agent。表与 magpie `agentOf` 对应，品牌换成 skillstar。
