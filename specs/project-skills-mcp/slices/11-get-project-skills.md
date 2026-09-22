# 11 — 查询编排

## 契约

按路径返回项目技能事实和加载说明。不注册，不部署。运行时可见性永远是未验证。

## 缝

`skillstar_app::project_skills_mcp::inspect::get_project_skills`。

`observe_project` 之后调用 `inspect_project_skills`。未注册时清单为空，`registered: false`，不写索引。

返回里 `runtime_visibility` 的类型只有 `unverified` 一个值。不要 `bool loaded`。加载说明包含物理相对路径和「当前会话未验证，需要该 Agent 自己重新发现」。不探测进程，不读技能正文，不用 MCP Roots。

## 人可以运行

`cargo test -p skillstar-app get_project_skills_`

## 验证

- `get_unregistered_project_writes_nothing`
- `get_runtime_visibility_is_unverified`
- `get_uses_inspect_project_skills`
- `get_load_hint_names_the_physical_rel`

## 可改

加载说明的中文或英文文案。路径分隔符固定为正斜杠。

## 不可改

不把 `scan_project_skills` 当作这个函数的实现。不返回 Hub 绝对路径作为给 Agent 的加载位置；可以在事实里保留链接目标供冲突判断，加载提示用项目内相对路径。

## 必须保持绿

05 的事实测试。

## 会改这一档的反馈

查询创建了项目注册项，或把 `unverified` 算成可切换的布尔值。
