# 37 — 可见家族

## 契约

每个 Agent 可以有一份可见名单。名单里的项是家族标签、provider id 或 group id。家族标签写在 provider 或分组上，字段名 `family`。某个 Agent 没有名单时，它看到全部模型。

这份名单同时收窄两处：该 Agent 来拉的 `/v1/models`，以及 writer 写进该 Agent 文件的模型目录。被收窄掉的模型，Agent 若仍用 id 来请求，网关照常回答。路由顺序不读这份名单。已经保存的 model ref 不被清空，Agent 文件除了目录列表以外不因本档改写。

## 缝

名单存在 `model_gateway.json` 的 `visible`，形状是 Agent id 到字符串数组。空数组与缺省相同，都表示全显示。判定函数在 `skillstar-gateway`，投影和 writer 都调用它。

## 人可以运行

给 OpenCode 设 `visible = ["relay"]`，它的目录文件里只剩带这个家族或 id 的模型。再请求一个不在名单里的 `provider/model`，假上游仍然收到。

## 视觉

变量只有一个：该 Agent 打开选择器时的列表。

裁剪：选择器列表。

本档不评：显示名、effort、提供商行。

screenshot-critique。参照 `assets/magpie/37-families.png` 若存在，则 compare-screenshots，裁剪只盖住列表。视口 1440×900 与 1280×800。preview-shots 约 5 分钟，无回复则在 `choices.md` 记下决定并关掉窗口。不从 `usemagpie.ai` 下载。

验收句：列表只含这份名单允许的项；没有密钥和厂商 URL。

## 验证

- `visible_missing_agent_sees_all`
- `visible_empty_list_sees_all`
- `visible_filters_models_list_and_written_catalog`
- `visible_hidden_model_still_answers`
- `visible_does_not_change_saved_ref_or_route`

## 可改

家族标签在界面上的输入控件种类。

## 不可改

三种名单项、缺省为全显示、隐藏后仍可按 id 回答、不改路由。

## 必须保持绿

27、09、18 的测试。08 档不带 Agent 名单时的 `/v1/models` 仍是全量。

## 会改这一档的反馈

设置名单后，已保存的 model ref 被清掉，或隐藏的 id 被网关拒绝。

## 决定

- 名单匹配：项等于 provider id、等于 group id、或等于该 provider/分组的 `family` 标签，三者任一即可看见。不另做模糊匹配。
