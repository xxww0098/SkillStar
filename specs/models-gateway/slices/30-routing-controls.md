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
