# 25 — models.dev

## 契约

网关从 `https://models.dev/api.json` 取目录，缓存在 `<data_root>/cache/gateway-catalog/models.dev.json`。下载用 `probe_http_client`。新 Models 页只读这份缓存。不写 `ProviderEntryFlat.meta.model_catalog`，不写 `cache/model_catalog/`。

`SKILLSTAR_DATA_DIR` 改变数据根时，缓存跟着走。

## 缝

所有者 `skillstar-gateway`。`skillstar-app` 把缓存投影成不含密钥的 DTO。测试用假 HTTP 替换 URL，默认常量仍是上面的地址。

## 人可以运行

```bash
cargo test -p skillstar-gateway models_dev_
```

把 `SKILLSTAR_DATA_DIR` 指到临时目录再跑一次，确认文件不出现在真实数据目录。

## 验证

- `models_dev_cache_path`
- `models_dev_does_not_touch_provider_meta`
- `models_dev_respects_data_dir`

## 可改

缓存 JSON 以外的索引结构。磁盘上的文件是一份 api.json 的正文。

## 不可改

URL、路径、不双写旧 catalog、走 `probe_http_client`。

## 必须保持绿

`cargo test -p skillstar-models` 里旧 catalog 缓存测试。它们的目录不应出现 `models.dev.json`。

## 会改这一档的反馈

`meta.model_catalog` 被写上 models.dev 的内容。

## 决定

- 失败时沿用已有缓存。没有缓存时页面得到空目录，不插入一份手写模型表。
- 页面投影和打开页面时的同步调用留到 27 档。环回的 `/v1/models` 仍是空列表，不读这份缓存。
- 下载超时 30 秒，正文最多 64 MiB。空对象、数组和坏 JSON 都不覆盖已有文件。
- 磁盘文件是响应正文本身。不写 `model_providers.json`，不写 `cache/model_catalog/`。
