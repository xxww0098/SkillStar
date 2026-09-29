# 实现期间的选择

已定事项在 [README.md](README.md)。这里只记规格没写死、由实现定下来的事。一条记录不是改规格的放行：规格已经写了的，先改规格再改代码。银行里的条目不再重列，收尾时对照最终代码重写本文件。

信心低的排在前面。

## 01 协议夹具

### 已定，按这个做

- **上游回复的夹具是一条 Chat JSON，不是 magpie 真正收到的 SSE。** 本档的文件必须是 JSON。magpie 翻译时会强制上游流式，非流式正文它直接报错，所以抄不到一份「上游 HTTP 正文」。请求侧的上游字节是对 magpie 的实测；回复侧是按同一次对话拼出的 `chat.completion`，译回 Anthropic 后与 magpie 非流式渲染的字节一致（思考、正文、一次工具调用、用量 10/5）。信心：中。以后的流式解码不要把这份 JSON 当成上游线上的事件序列。

- **缺了的 id 留空，不按时钟现编。** magpie 会给空的工具调用 id 编 `call_`、给空的消息 id 编 `msg_`。本档夹具里的 id 都有值。现编会让同一份输入每次字节不同。Agent 没给 id 时，译出的 id 就是空字符串。信心：中。

- **一条回复只译 `choices` 里的第一条。** magpie 的翻译也只锁定第一条能读的 choice。夹具只有一条。后面的 choice 被丢掉。信心：中。

- **Anthropic 译成的 Chat 请求总是要求流式，并带上用量。** 不看 Agent 有没有写 `stream`。上游正文带 `stream: true` 和 `stream_options.include_usage`。这是 magpie `translate()` 拼上游之前的做法；入站有没有 `stream`，实测上游字节相同，也和本档夹具一致。信心：高。后面的保持写者可以假定上游回的是流。

- **工具的 `input` 和 `input_schema` 按 Agent 写下的字节复制。** 用的是 `serde_json` 的 `raw_value`，没有开 `preserve_order`。后者会改掉整个 workspace 的对象键顺序。`raw_value` 只是解析接口，不会让别的 crate 换一种编码。不能把 `RawValue` 放进 untagged enum，serde 会先缓冲成 `Value`，原始字节就丢了。信心：高。

### 琐碎

- 夹具文件末尾多一个换行会在比较前丢掉。允许表目前只实现 `brand`（先替换 `Magpie`，再替换 `magpie`）。未知规则直接让测试失败。这些 JSON 是字节夹具，不进 biome 格式化，和 `tool_sync` 的 golden 一样。
