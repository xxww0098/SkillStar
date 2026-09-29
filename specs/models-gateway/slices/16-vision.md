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

- 描述请求自己不再进入视觉转述，避免循环。用上下文标记，和 magpie 的 `describingKey` 一样。
