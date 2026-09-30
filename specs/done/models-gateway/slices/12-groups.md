# 12 — 分组

## 契约

模型 id `group/<id>` 展开成分组成员。成员数组的字段名是 `members`。组成员可以是另一个分组。成环的写入被拒绝。嵌套深于 `maxNest`（8）被拒绝。同名模型的自动分组只在读取时推导，用户没有编辑过就不写入 `model_gateway.json`。

## 缝

分组存在 `model_gateway.json`，所有者 `skillstar-gateway`。展开是纯函数。写入函数在拒绝时不改文件。

## 人可以运行

```bash
cargo test -p skillstar-gateway group_
```

用临时 `SKILLSTAR_DATA_DIR` 写一份分组文件，再展开 `group/outer`。

## 验证

- `group_expands_members`
- `group_rejects_cycle`
- `group_rejects_depth_9`
- `group_auto_same_name_is_not_persisted_until_edit`

## 可改

无。成员字段名定为 `members`。

## 不可改

id 前缀 `group/`、深度 8、环拒绝时零写入、自动分组默认不落盘。

## 必须保持绿

`cargo test -p skillstar-gateway group_`。密钥表文件不被本档测试修改。

## 会改这一档的反馈

深度 8 的合法分组被拒绝，或环被写进文件。

## 决定

- 规则与分类器不在本档过滤成员。13、14 档在展开之后再选。
