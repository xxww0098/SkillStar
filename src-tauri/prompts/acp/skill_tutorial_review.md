请对你上一轮的 HTML 做一次发布前审查，并重写完整最终版。逐项检查：

1. inventory 中每个相对路径都在“文件覆盖附录”中精确出现，各自拥有唯一的 `<tr data-skillstar-file=\"...\">`；行内“路径、角色、教程位置、证据/限制”四格均非空，首格可见文字精确等于路径，并且教程正文确实吸收了所有与使用有关的文件。
2. 每条能力、参数、步骤和成功标准都有文件证据；推断已明确标记；没有声称执行过任何命令。
3. 新用户能沿最短路径成功，也能从排错章节恢复；图示传达真实结构或流程，不是装饰。
4. HTML 从 doctype 到闭合标签完整，使用指定语言，响应式、可打印、键盘与读屏可理解。
5. 没有 JavaScript、事件属性、表单、嵌套浏览上下文、外链资源、可点击外部导航、网络请求、meta refresh、危险 SVG 或疑似秘密值；文件有证据的 endpoint/命令 URL 只作为不可点击文本或代码保留。
6. `<head>` 前三个元素依次且精确为 UTF-8 meta、`width=device-width, initial-scale=1` viewport meta 和 CSP meta，没有其他 meta；CSP 早于 `title`/`style` 且精确限制为：`default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:`。实际 CSS 没有 `@import`、外部 URL、CSS expression、隐藏内容规则或反斜杠转义。
7. 最终答案是可直接保存到本机 `tutorial.html` 的完整内容，没有用在线链接、发布地址或外部资源代替任何部分。

无论初稿是否需要修改，都只输出一个全量替换用的 fenced code block，语言标签精确为 `skill-tutorial-html`。代码块外不要解释，不得省略任何 HTML。
