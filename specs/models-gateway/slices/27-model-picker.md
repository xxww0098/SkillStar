# 27 — 模型选择器

## 契约

为一个 Agent 选模型时，列表项的 id 是 `provider/model` 或 `group/<id>`。选中并保存后，走该 Agent 已落地的 writer。尚未落地的 id 保存返回 `agent_not_managed` 且不写文件。

## 缝

列表来自网关的目录投影（25 档的缓存加分组）。命令在 `skillstar-app`。

## 人可以运行

打开选择器，选中一条 `group/` id，保存一个已经实现的 Agent，读文件看到该 id。

## 视觉

变量只有一个：打开的选择器里 id 怎么写。

裁剪：选择器弹出层。

本档不评：提供商行的掩码、次要字段、空列表的句子、家族过滤。

screenshot-critique 与 compare-screenshots 的做法同 26 档。参照 `assets/magpie/27-picker.png`。视口 1440×900 与 1280×800。preview-shots 约 5 分钟，无回复则记录并继续。

验收句：每一项都能读成 `provider/model` 或 `group/` 开头；弹出层里没有厂商密钥和厂商 URL。

## 验证

- 选择器夹具含两种前缀
- 保存路径使用 06 或 18 档的 writer，不新写一套

## 可改

弹出层的最大高度。

## 不可改

id 的两种形状、保存走已有 writer。

## 必须保持绿

writer 测试。类型生成。

## 会改这一档的反馈

列表项是裸模型名，或保存绕过 gateway writer。

## 决定

- 目录为空时选择器可以没有项。空态文案是 31 档。
