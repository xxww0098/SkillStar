# 26 — 提供商行

## 契约

Providers 栏的一行显示名称和掩码后的凭据摘要。行内没有明文密钥，没有厂商 base URL。

## 缝

行组件只接收 `skillstar-app` DTO 里已有的 `credential_summary`。不新开一个把密钥送到渲染进程的命令。

## 人可以运行

桌面应用打开 Models，Providers 栏至少一行。临时目录里的密钥不出现在界面文本里。

## 视觉

变量只有一个：提供商行上有什么。

裁剪：一行，含名称和凭据摘要。

本档不评：三栏是否还在（应仍在，但不是本档要改的）、选择器、路由控件、空态、行与行的间距体系。

接受前跑 screenshot-critique。批评者只看 1440×900 与 1280×800 的这行裁剪和验收句。参照作物 `specs/models-gateway/assets/magpie/26-provider-row.png`，有图则 compare-screenshots，裁剪只盖住这一行。没有就从本机 magpie 截，截不到则在 `choices.md` 记一笔并用 README 五问。不从 `usemagpie.ai` 下载。preview-shots 打开后大约等 5 分钟，无回复就记下决定并关掉窗口。

验收句：这一行能认出是哪个提供商，凭据是掩码；看不到完整密钥，看不到厂商 URL。

## 验证

- 渲染测试：DTO 里不存在密钥字段时，行仍能画出摘要。
- 在界面上搜索测试密钥的明文，结果为 0。

## 可改

掩码的圆点字符，只要摘要仍来自后端。

## 不可改

明文密钥不到达前端、厂商 URL 不作为这一行的主信息。

## 必须保持绿

07 档三栏仍在。Settings 的提供商编辑若仍显示密钥输入，那是 Settings，本档不改它。

## 会改这一档的反馈

截图里出现完整密钥或 `https://api.` 一类厂商主机。

## 决定

- 编辑密钥的表单不在本档。本档的行是只读摘要。
- 摘要用已有的 `Credential::summary`。行组件只接收名称和这串摘要。
- 名称在左，摘要在右。窄列里名称截断，摘要保持原样。不显示厂商主机。
- Agents 和 Gateway 的 `credential_summary` 是空字符串。三栏仍用同一个行 DTO。
- 参照作物 `26-provider-row.png` 不在仓库里，本机 magpie 页面没在听。screenshot-critique 技能不在磁盘上。按 README 五问接受这一行。
