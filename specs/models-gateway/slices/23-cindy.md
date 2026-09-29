# 23 — Cindy

## 契约

Cindy 只产生导入链接 `cindy://provider/import?v=1&data=...`。链接里的 provider 使用环回端点和占位 bearer `skillstar-cindy`。读取 Cindy 的数据库只为了显示「已经导入」。不写这个数据库。

## 缝

`skillstar-gateway`。链接编码与 magpie `cindy.go` 的 JSON 字段一致，品牌换成 skillstar。测试用临时目录里的只读 sqlite。写入尝试会使测试失败。

## 人可以运行

保存 Cindy 时界面给出链接。测试打印链接并解码 data，检查 base URL。

## 验证

- `cindy_link_roundtrip`
- `cindy_database_is_not_written`
- `cindy_bearer_is_token_for`

## 可改

链接展示在 UI 的哪一行。26 档以后的视觉档不评这一行，本档的探针只检查字符串。

## 不可改

scheme、只读、bearer 形式。

## 必须保持绿

临时目录的 sqlite 文件哈希在调用前后相同。

## 会改这一档的反馈

数据库 mtime 变化，或链接里出现厂商主机。

## 决定

- 用户是否在 Cindy 里确认，SkillStar 不等待。链接生成即这一档完成。
