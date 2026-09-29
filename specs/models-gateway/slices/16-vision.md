# 16 — 视觉转述

## 契约

目标模型不能看图时，网关先请一个能看图的模型把图写成文字，再把文字交给目标。系统提示词是 magpie `vision.go` 的 `visionSystem`，逐字复制。转述请求的 User-Agent 是 `skillstar-vision/1`。超时 2 分钟，并行 4，描述缓存 256 条。关闭时，不能看图的目标拒绝带图请求，和以前一样。

## 缝

`skillstar-gateway`。能看图的模型 id 存在 `model_gateway.json` 的视觉字段。测试用两个假上游：一个描述，一个回答。

## 人可以运行

```bash
cargo test -p skillstar-gateway vision_
```

## 验证

- `vision_system_bytes_match`
- `vision_replaces_image_for_text_only_target`
- `vision_off_rejects_image`
- `vision_user_agent_and_caps`

## 可改

缓存的淘汰结构。

## 不可改

提示词字节、UA、2 分钟 / 4 / 256、关闭时不转述。

## 必须保持绿

图像路由在视觉关闭、目标能看图时仍按 08 档原样转发。

## 会改这一档的反馈

提示词差一个字，或关闭时仍然发出描述请求。

## 决定

- 描述请求自己不再进入视觉转述，避免循环。调用方标了 describing，或系统提示词就是转述提示词，都不再转述。描述请求发往厂商，不回到本机监听。
- 字段名是 `vision`。去掉空白后为空、为 `off`、缺字段、文件缺失或读不出来，都是关闭。这一档没有模型目录，不自动挑能看图的模型。
- 关闭时，监听仍按 08 档把正文原样转发。网关还不知道谁能看图，在这里拒绝会打断原来的图像路由。调用方明确目标不能看图时，当前这张图返回 400，正文含 `does not support image input`，不发出描述请求。更早一轮或工具结果里的图换成省略说明。
- `vision` 是模型 id 时，正文 `model` 与它相同就视为能看图。其它带 Chat `image_url` 的目标，先把图换成描述再转发。描述请求发到当前上游的 `/v1/chat/completions`。
- 只处理翻译之后的 Chat `image_url`。图像生成和 Responses 仍原样转发。成功的描述才进缓存，按写入顺序丢掉最旧的；同一张图同一次请求只问一次。
