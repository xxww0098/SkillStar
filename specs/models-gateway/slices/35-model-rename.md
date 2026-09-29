# 35 — 模型改名

## 契约

人可以给一个目录模型一个显示名。显示名出现在选择器里。出站请求的 `model` 字段仍是上游 id，不是显示名。

## 缝

显示名存在 `model_gateway.json` 的 `model_names`，键是 `provider/model`。目录缓存不被改写。`skillstar-gateway` 在投影 DTO 时附上显示名，翻译函数不读显示名。

## 人可以运行

把 `probe/m1` 显示成「实验」，发一条 Chat，假上游收到的 model 仍是 `probe/m1`。

## 视觉

变量只有一个：选择器里该项的显示名。

裁剪：那一项。

screenshot-critique。参照可选 `assets/magpie/35-rename.png`。视口两档。preview-shots 约 5 分钟。

验收句：该项能看到新的显示名；裁剪不需要出现上游 id 以外的主机名或密钥。

## 验证

- `rename_changes_label_only`
- `rename_upstream_model_field_unchanged`

## 可改

显示名允许的字符。拒绝空字符串和含换行的名字。

## 不可改

上游 id 不被显示名替换、不改 models.dev 缓存文件。

## 必须保持绿

01 档翻译夹具。27 档选择器仍能按 id 保存。

## 会改这一档的反馈

假上游的 model 字段变成了显示名。

## 决定

- 没有显示名时，选择器展示 id。
