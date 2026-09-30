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
- 显示名写在 `model_gateway.json` 的 `model_names`，键是 `provider/model`。不使用 `modelNames`。
- 空名字拒绝，不删已有的键。换行和回车拒绝，不折成空格。超过 80 个 Unicode 标量，或含 `://`、`sk-`，同样拒绝。
- 未知 id 和 `group/` 拒绝。目录缓存文件的字节不变。翻译函数不读 `model_names`。
- 选择器按钮仍用上游 id 保存。改名表单在该项外面。语言文件不动。
- 没有参照 `assets/magpie/35-rename.png`。1440×900 的裁剪是 `probe/m1`。1280×800 保存后的裁剪是「实验」。五问：这一裁是要选中的那一项；局域网开关不在裁剪里；没有密钥和厂商 URL；没有对照就不比较；第一下点这一项，保存的仍是上游 id。
