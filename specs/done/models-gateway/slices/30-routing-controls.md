# 30 — 路由控件

## 契约

一个控件改 provider 或分组的路由模式（smart、order、rotate、usage）和亲和（auto、session、turn、off）。空值保存后文件里可以缺省，读回来仍是 smart 与 auto。保存写 `model_gateway.json`，不改 `model_providers.json` 的版本和列。

## 缝

命令在 `skillstar-app`，写入在 `skillstar-gateway`。控件只发模式枚举。

## 人可以运行

在界面上把一条改成 rotate，再读 `model_gateway.json`。把 v4 store 复本的哈希比较一遍，应相同。

## 视觉

变量只有一个：这个控件。

裁剪：控件本身。

本档不评：最近请求表、提供商行、分组里有哪些成员。

screenshot-critique。参照 `assets/magpie/30-routing.png` 时 compare-screenshots。视口两档。preview-shots 约 5 分钟。

验收句：控件只在四种路由和四种亲和里选择；裁剪里没有密钥和厂商 URL。

## 验证

- `routing_control_persists_in_gateway_json`
- `routing_control_does_not_rewrite_provider_store`
- 09 与 10 档的缺省测试仍然成立

## 可改

控件是分段按钮还是下拉。枚举值不可改。

## 不可改

四种加四种、缺省含义、不改 v4 文件结构。

## 必须保持绿

`cargo test -p skillstar-gateway route_mode_` 与 `affinity_`。

## 会改这一档的反馈

保存后 `model_providers.json` 的版本或字段变了。

## 决定

- 控件放在 Gateway 栏里该 provider 或分组的详情，不放进 Settings。
- 控件是分段按钮。选中的提供商一块，`model_gateway.json` 里每个已保存的分组再一块。分组只显示 id，不显示成员。按钮文字就是 smart、order、rotate、usage 与 auto、session、turn、off。
- smart 和 auto 不写字段。空和未知在读文件时仍是这两项。控件提交未知词时拒绝保存，文件字节不动。
- 保存保留这一行的其它字段和文件顶层的其它键。`group/` 前缀去掉后再写入。不打开 `model_providers.json`。
- 页面打开时查询。保存后只让这一页的路由查询失效。带 `://`、`sk-` 或 `api.openai.com` 的 id 不画出来。
- 参照作物 `30-routing.png` 不在仓库里。screenshot-critique 技能不在磁盘上。1440×900 与 1280×800 上，裁剪是 `p-deepseek` 与 `fast` 两块。前者按下 rotate 和 auto，后者按下 smart 和 auto。八个词都在，没有换行挤掉。裁剪里没有 `https://`，没有 `sk-`。五问：这一屏是在改这条 provider 或这个分组的路由和亲和；上一档的最近请求表还在 Gateway 栏下方一行，环回地址还在 Agent 名下，掩码还在提供商列，都不在这块裁剪里；裁剪没有厂商 URL 和密钥；没有参照作物，八个词就是契约；第一下点的是其中一个模式词。按此接受。
