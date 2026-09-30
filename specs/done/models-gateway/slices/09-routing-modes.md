# 09 — 四种路由

## 契约

候选顺序支持 smart、order、rotate、usage。配置里的空字符串就是 smart。03 档的那一条 smart 决定仍然成立。本档把四种模式各自用一份夹具钉死，并说明 usage 模式读的是注入的快照，不是一次新的配额请求。

## 缝

纯函数仍在 `skillstar-gateway`。模式存在 `model_gateway.json` 里该 provider 或分组的字段，缺省 smart。函数测试不读文件；文件缺省由另一条只测解码的测试覆盖。

## 人可以运行

```bash
cargo test -p skillstar-gateway route_mode_
```

## 验证

- `route_mode_empty_is_smart`
- `route_mode_order_is_listed_order`
- `route_mode_rotate_advances`
- `route_mode_usage_reads_snapshot_only`

时钟与轮转计数由测试注入。

## 可改

轮转计数存在内存里的结构。

## 不可改

四种名字、空字符串的含义、usage 不发网络请求。

## 必须保持绿

`route_smart_` 与本档测试。

## 会改这一档的反馈

magpie 对 order 或 rotate 的下一名和夹具不一致。

## 决定

- 亲和不在本档改变顺序。10 档在这个顺序之前插入「留下」。
