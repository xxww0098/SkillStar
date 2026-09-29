# 14 — 分类器

## 契约

带 intent 的规则在回合开始时问一次分类器：模型 id 是 `provider/model` 或另一个 `group/<id>`。置信度达到 `jevSure`（0.4）才算数。超时 8 秒、失败后 30 秒内不再问、相同消息缓存 10 分钟。回合中途不再问。问的时候 User-Agent 是 `skillstar-router/1`。失败、超时或答非所问时，没有 intent 命中。

分类器是网关里的一次模型调用。它不加载 `skillstar-decision`，也不读 AgentJev 的权重。

## 缝

`skillstar-gateway` 内的函数。测试用假上游。`Cargo.toml` 仍然只有 `skillstar-core`。依赖守卫已禁止 `gateway → decision`。

## 人可以运行

```bash
cargo test -p skillstar-gateway classify_
```

## 验证

- `classify_accepts_at_0_4`
- `classify_rejects_below_0_4`
- `classify_timeout_8s_skips_intent`
- `classify_once_per_turn`
- `classify_cache_10m_and_rest_30s`
- `classify_user_agent`
- `gateway_manifest_has_no_decision_dep`

## 可改

问句的内部拼装，只要假上游收到的模型 id 和意图列表与夹具一致。

## 不可改

0.4、8 秒、10 分钟、30 秒、每回合一次、不链接决策 crate。

## 必须保持绿

`rules_with_intent_do_not_match_yet` 改为「分类器关闭时仍不匹配」。其余 `rules_` 保持。

## 会改这一档的反馈

分类器调用打到了 AgentJev 的本地接口，或 0.39 被当成命中。

## 决定

- 分类器模型 id 由分组配置给出。没有配置时，intent 规则不匹配，不找默认本地模型。
