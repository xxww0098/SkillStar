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

## 结果

已落地，无新 crate、无新 Cargo 依赖。Zed 不设文件路径。

### JSON 是否已原子

是。`storage::write_json_unlocked` 本来就走
`skillstar_core::infra::fs_ops::atomic_write`（同目录临时文件 + fsync + rename）。
本片把它标成 `pub(crate)`，`tool_store::atomic_json::write` 只是这一个实现的薄包装，
没有改 storage 的调用方，也没有再写一套原子写。截断保护沿用 `fs_ops` 已有测试；
本片只加了「写完是完整 JSON、不留 `.tmp`」的回环测试。

### 路径表（相对 `SKILLSTAR_TOOL_SYNC_HOME`；未沙箱时 Windows 用 `APPDATA`，Linux 用 `XDG_CONFIG_HOME` 否则 `~/.config`）

| app | macOS | Windows（沙箱） | Linux（沙箱） |
| --- | --- | --- | --- |
| windsurf `state.vscdb` | `Library/Application Support/Windsurf/User/globalStorage/state.vscdb` | `AppData/Roaming/Windsurf/User/globalStorage/state.vscdb` | `.config/Windsurf/User/globalStorage/state.vscdb` |
| kiro 数据目录 | `Library/Application Support/Kiro` | `AppData/Roaming/Kiro` | `.config/Kiro` |
| aws sso cache（三平台相同） | `.aws/sso/cache` | `.aws/sso/cache` | `.aws/sso/cache` |
| qoder `state.vscdb`（无文件时的首选） | `Library/Application Support/Qoder/User/globalStorage/state.vscdb` | `AppData/Roaming/Qoder/User/globalStorage/state.vscdb` | `.config/Qoder/User/globalStorage/state.vscdb` |
| trae / trae-solo / trae-cn / trae-solo-cn `storage.json` | `Library/Application Support/{Trae,TRAE SOLO,Trae CN,TRAE SOLO CN}/User/globalStorage/storage.json` | `AppData/Roaming/{同上}/User/globalStorage/storage.json` | `.config/{同上}/User/globalStorage/storage.json` |
| codebuddy `state.vscdb` | `Library/Application Support/CodeBuddy/User/globalStorage/state.vscdb` | `AppData/Roaming/CodeBuddy/User/globalStorage/state.vscdb` | `.config/CodeBuddy/User/globalStorage/state.vscdb` |
| codebuddy-cn `state.vscdb` | `Library/Application Support/CodeBuddy CN/User/globalStorage/state.vscdb` | `AppData/Roaming/CodeBuddy CN/User/globalStorage/state.vscdb` | `.config/CodeBuddy CN/User/globalStorage/state.vscdb` |
| zcode home | `.zcode` | `.zcode` | `.zcode` |

Qoder 若已有文件，按 cockpit 顺序取第一个存在的：`User/globalStorage/state.vscdb`，然后 `globalStorage/state.vscdb`，然后 `state.vscdb`。

### 测试

`cargo test -p skillstar-usage --locked --lib -- tool_paths:: tool_store:: vscdb:: trae_platform::`：15 passed。
随后 `cargo test -p skillstar-usage --locked --lib`：199 passed。

- `tool_paths::tests::sandboxed_path_table_covers_every_app_on_every_os`
- `tool_paths::tests::qoder_state_db_prefers_the_first_existing_candidate`
- `tool_paths::tests::zcode_home_prefers_setting_json_database_dir`
- `trae_platform::tests::platform_table_matches_cockpit_product_paths`
- `trae_platform::tests::parse_accepts_kebab_case_and_defaults_empty_to_trae`
- `tool_store::vscdb_ext::tests::upsert_delete_and_multi_key_write_preserve_unrelated_rows`
- `tool_store::vscdb_ext::tests::multi_key_write_rolls_back_when_a_later_key_fails`
- `tool_store::vscdb_ext::tests::missing_database_error_uses_the_caller_label`
- `tool_store::atomic_json::tests::write_replaces_json_without_leaving_a_temp_file`
- `tool_store::keychain_cli::tests::sandboxed_internet_password_ops_do_not_spawn_security`
- `tool_store::keychain_cli::tests::parses_account_from_security_metadata`
- 既有 `vscdb::tests::writes_and_reads_unified_oauth_token`、`writes_cursor_session_and_preserves_unrelated_rows`、`refuses_to_create_a_missing_ide_database` 仍绿；另加 `missing_cursor_database_names_cursor`

### 静默决定

- `TraePlatformKind` 独立放在 `trae_platform.rs`。`app_support_dir_name()` 等于 cockpit 的 `display_name()`。`app_name()` 是 cockpit 的 `macos_app_name()`（`Trae.app` 等）。`catalog_id` 用连字符（`trae-solo`），`provider_key` 用下划线（`trae_solo`）。`region_hosts()` 是 cockpit `candidate_api_origins` 的静态源站，再加上尚未列入的 `TRAE_ACCOUNT_API_ORIGIN_*`（含 `grow-normal.traeapi.us`）。没有拷贝 client id / secret。
- 路径拼接收成 `DesktopOs` 纯函数，宿主 OS 用 `cfg` 选择，所以一个测试二进制能锁三平台。沙箱判断用 `is_tool_sync_sandboxed()`（空环境变量不算沙箱），对齐 Antigravity，而不是 Cursor 的 `var_os().is_some()`。
- 测试只改 `SKILLSTAR_TOOL_SYNC_HOME`（外加故意放毒的 `APPDATA` / `XDG_CONFIG_HOME`），不改 `HOME` / `USERPROFILE`，避免和 `local_import` 的 home 锁抢环境，也不读写真 home。覆盖断言是：结果落在沙箱根下，且不等于 `home_dir()` 拼出来的同一相对路径。
- Qoder 多候选只做存在性选择，不建目录、不复制默认库（那是注入行为）。
- ZCode 设置文件名跟 cockpit 的 `setting.json`，不读规格里写的 `settings.json`（测试锁了错误文件名会被忽略）。`zcode_home()` 返回数据根 `~/.zcode` 或 `{dataBaseDir}/.zcode`，覆盖值只从默认的 `~/.zcode/v2/setting.json` 读。
- `aws_sso_cache_dir()` 在三平台都是 `~/.aws/sso/cache`，不在 Kiro 用户数据目录下。
- `vscdb` 的 upsert/delete 共用一个事务 helper，错误串里的产品名是参数。Antigravity 缺库文案仍由 `write_antigravity_oauth_token` 自己返回（带 “Antigravity IDE”）。打开/提交失败时的文案改成同一模板（例如 `提交 Antigravity state.vscdb 事务失败`，不再是 `提交 Antigravity OAuth 失败`）。空 key 列表直接返回，不要求文件存在。
- `keychain_cli` 只包 internet-password（`find` / `add -U` / `delete`），不并进 generic-password。`add` 不先删同 server 的其它账号；Zed 的先清后写留到 provider 片。沙箱下三个操作都返回固定错误，不 spawn `/usr/bin/security`。
- `tool_store` 子模块：`vscdb_ext`、`atomic_json`、`keychain_cli`。给后续片用的 `pub(crate)` 入口在非测试构建上 `allow(dead_code)`，避免今天没人调用就报警。
- 集成时把 `find_internet_password` 收成 `(server, account)`：Zed 按账号读，避免按 server 猜第一个账号。
