# 36 — effort

## 契约

请求里的 effort 在发给上游之前，收成该模型目录里声明的等级。模型没有目录数据时，等级原样通过。Claude 进程桥上的 `xhigh` → `max` 仍只按 05 档发生，本档不重复改那条参数。

分组里某个成员可以固定 effort。固定值优先于请求里的值。用户另行保留的等级子集是 38 档，本档不读那份列表。

## 缝

收束函数在 `skillstar-gateway`，读 25 档的缓存。固定值存在分组成员上，与 12 档同一份 JSON。

## 人可以运行

```bash
cargo test -p skillstar-gateway effort_
```

界面上给一个成员固定 `high`，夹具请求带 `low`，假上游看到 `high`。

## 视觉

变量只有一个：成员上的 effort 固定控件。

裁剪：该控件。

screenshot-critique。无参照则跳过 compare-screenshots 并记入 `choices.md`。视口两档。preview-shots 约 5 分钟。

验收句：控件在为这一个成员固定 effort；裁剪里没有密钥和厂商 URL。

## 验证

- `effort_clamped_to_catalog`
- `effort_unknown_model_passes_through`
- `effort_member_fixed_wins`
- `effort_claude_bridge_xhigh_unchanged`

## 可改

控件的控件种类。等级词表来自目录，不在前端写死一份。

## 不可改

无目录数据时不自行删等级、固定值优先、05 档的 `xhigh` 映射不在这里再做一次。

## 必须保持绿

`claude_bridge_args_match_magpie_list`。`models_dev_` 缓存测试。

## 会改这一档的反馈

目录里没有该模型时，effort 被清空。

## 决定

- 显示名（35 档）不参与等级查找。查找键是上游 id。
