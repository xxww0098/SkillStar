# 03 · tool_paths 扩展 + 存储基元层

## 解锁的契约

所有新 provider 的本地存储定位与读写基元一次到位，后续 provider 片只声明 key schema。

## API 接缝

| 层 | 文件 | 改动 |
| --- | --- | --- |
| 域 | `crates/skillstar-usage/src/tool_paths.rs` | 新增：`windsurf_state_db_path()`、`kiro_data_dir()`+`aws_sso_cache_dir()`、`qoder_state_db_path()`（多候选路径解析）、`trae_storage_path_for(TraePlatformKind)`、`codebuddy_state_db_path()`、`codebuddy_cn_state_db_path()`、`zcode_home()`（`settings.json` 的 `dataBaseDir` 覆盖优先）、`zed` 无文件路径（keychain）。全部走 `SKILLSTAR_TOOL_SYNC_HOME` 沙箱优先 + 三平台 cfg |
| 域 | `crates/skillstar-usage/src/tool_store/`（新 module，纯 IO/crypto，无 provider 语义） | `vscdb_ext`（在现有 `vscdb.rs` 上提 `pub(crate)` 通用 `upsert_item`/`delete_item`/多 key 事务写，错误串参数化 label 去掉写死的 "Cursor"）；`atomic_json`（**现场确认 `storage.rs` 的 JSON 落盘是否已原子**，是则复用否则补 helper）；`keychain_cli`（`/usr/bin/security` internet-password 封装，zed 用；与 anthropic/codex 的 generic-password helper 并存不合并） |
| 域 | `crates/skillstar-usage/src/trae_platform.rs` 或 tool_paths 内表 | `TraePlatformKind`（catalog_id→platform 映射：provider_key/display/app_support_dir/app_name/region hosts），逐一对齐 cockpit `trae_account_core_product_paths.rs` 的 `app_support_dir_name()` |

## 人能看见

纯内部接缝；证据 = 路径解析单测（沙箱 HOME 下每平台每 app 的期望路径表）。

## 验证

- 每 app × 每平台的路径单测（临时 HOME + `SKILLSTAR_TOOL_SYNC_HOME` 双覆盖断言）。
- `vscdb_ext` upsert/delete/多 key 事务在临时 db 上的回环测试（保留无关行）。
- 原子写 helper 测试（若需新增）：并发写不产生截断文件。

## 委托给实现者的决定

- `tool_store` 的子模块命名与文件粒度。
- TraePlatformKind 放 `tool_paths.rs` 还是独立文件（行数压力决定）。

## 必须保持绿

- `vscdb.rs` 现有 cursor/antigravity 读写行为不变（通用化只是开放可见性+参数化 label）。

## 会改变本片的人类反馈

- 无（纯机械接缝）。若评审发现某 app 路径与真机不符，改路径表即可。
