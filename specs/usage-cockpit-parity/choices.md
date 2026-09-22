# Choices — usage-cockpit-parity

静默决定。规格没写死、实现时定下来的事。按确信程度从低到高。

## 先看这些

### ZCode 的设置文件叫 `setting.json`

规格写的是 `settings.json`。cockpit 实际读的是 `~/.zcode/v2/setting.json`，`dataBaseDir` 指到 `{dir}/.zcode`。路径函数按 cockpit 来，测试锁了错误文件名会被忽略。

判定：就这么做。和参照实现一致，规格笔误。

### 钥匙串查询必须带账号

切片 03 先做成只按 server 找第一条。Zed 的钥匙串项是 `server=https://zed.dev` 加 `account=user_id`，不带账号会读错人。`find_internet_password(server, account)` 两个都传给 `security`。删除仍按 server 循环删光该 server 下的 internet-password，因为一条 `security delete` 只删一项。

判定：查询带账号。删除范围等切片 21 写回时再核对，现在没有调用方。

### `SubscriptionBuilder` 收明文再加密

`provider_state(json)` 跟 access token 一样，收明文，`build` 时用现有 AES-GCM 写进 `provider_state_encrypted`。JSON 形状留给各 provider。现在没有生产调用。

判定：就这么做。和同文件其它凭据字段同一条路。

## 已经定了

### 窄 patch 只写在 `apply_oauth_credentials`

`apply_fetcher_state` 本来就会调用它，所以两条 refresh 路径都会轮换 `provider_state_encrypted`。没有再写一遍。

判定：就这么做。

### 通用更新拒绝已有的 token-import 行

更新入参里没有 `auth_mode`。拒绝的是库里这行已经是 TokenImport，避免编辑表单改写导入行。创建路径拒绝传入的模式。

判定：就这么做。

### xAI 的 OAuth 检查不放宽

xAI 完成登录时仍要求 `auth_mode == OAuth`。这个 provider 没有 token-import。

判定：就这么做。

### `cursor.rs` 只补了一个字段初值

新字段没有 `Default`，Cursor 的手写结构体字面量必须写 `provider_state_encrypted: None`，否则编不过。登录逻辑没动。

判定：就这么做。这是编译要求，不是行为变更。

### 订阅 JSON 落盘本来就是原子的

`write_json_unlocked` 已经走 `fs_ops::atomic_write`。把它标成 `pub(crate)`，`tool_store::atomic_json::write` 只是这一个实现的薄包装。

判定：就这么做。不另写一套原子写。

### 三平台路径用纯函数测

`DesktopOs` 把 macOS / Windows / Linux 的相对路径做成不依赖宿主的函数，一个测试进程锁全部平台。沙箱只认非空的 `SKILLSTAR_TOOL_SYNC_HOME`。测试不改 `HOME`。

判定：就这么做。

### Qoder 只在已有文件里挑路径

候选顺序跟 cockpit：`User/globalStorage/state.vscdb`，然后 `globalStorage/state.vscdb`，然后 `state.vscdb`。没有文件就不建库。

判定：就这么做。建库是注入，不是路径解析。

### AWS SSO 缓存不在 Kiro 目录里

三平台都是 `~/.aws/sso/cache`。

判定：就这么做。跟 cockpit 一致。

### macOS Safe Storage 是 1003 轮，不是 1000

规格写 1000。cockpit 和 Chromium 在 macOS 上是 PBKDF2-SHA1 1003 轮，盐 `saltysalt`，IV 是 16 个空格，前缀 `v10`。Linux 的 `peanuts` 和空口令是 1 轮。Windows 的 AES-256-GCM 磁盘前缀也是 `v10`，不是 `v11`。`v11` 只出现在 Linux 的 CBC。

判定：跟 cockpit。规格数字是笔误。真钥匙串和 Windsurf 重启还没做，不据此降级。

### ZCode 密文是三段，不是两段

实际格式是 `enc:v1:{nonce}.{tag}.{ciphertext}`，URL-safe、无 padding。不是「nonce 加 ciphertext‖tag」两段。无前缀时直接报格式错误，不把原文原样返回。

判定：跟 cockpit 源码。规格那句合并了 tag 和 ciphertext。

### 设备签名不保证两次相同

`ring` 的 P-256 签名不是 RFC6979，同一段消息连签两次结果不同。测试只做验签。返回值是 ASN.1 签名的标准 base64，不是整段 DeviceProof JSON。

判定：就这么做。没打真的 ExchangeToken，Trae 不降级。

### OAuth 回调参数是一张字符串表

`local_server::wait` 返回 `HashMap<String, String>`。只收 query。fragment 仍由手动回调在重放前并进 query。没有 `code` 就继续等。`wait_for_callback` 仍返回 code 字符串，所以 `cursor.rs` 不用改。

判定：就这么做。

### 远程轮询分两种构造器

`device()` 带用户码和验证地址。`remote_poll()` 没有用户码。`immediate()` 给 Claude 本机采纳。倒计时用 `interval_secs`，不是会话过期时间。

判定：就这么做。四种面板有组件测试。这轮没有截图终审。

### 新应用先不提供多开

没有逐个启动官方应用并核对登录态隔离。这些应用在实例表里是 Pending，选择器里不出现。Zed 和 GitHub Copilot 是结构性不支持。Cursor、Grok Bot、Antigravity 维持原来的已验证集合。

判定：就这么做。实机通过后再把对应应用改成 Verified。

### 不做配额唤醒网关

向 Antigravity 语言服务发合成消息来提前重置配额窗口，和读配额、写本机凭据不是一类事。默认不做，也不做直连探活的半截替代。

判定：不做。要改这个决定就另立 spec。

### Windsurf 没有自定义 scheme

cockpit 里没有 `windsurf://`。登录用本地回调。共享 loopback 只认 `code`，所以 implicit 的 `access_token` 由 Windsurf 模块自己的监听器收。`provider_state` 是明文 JSON，里面是 apiKey，必要时还有 apiServerUrl 和 auth1Token。`auth1_` 不跑 Devin 交换。本机导入不刷新配额，只把行存下来。

判定：就这么做。真浏览器登录和 Windsurf 重启还没做，不降级。

### OAuth 完成后只重写 IDE 和 xAI 的本机凭据

已置顶的卡在登录完成时，原先只有 xAI 和 Antigravity 会再写一次本机文件。注册表落地时一度改成「凡是能切号的都重写」，那样 Codex 和 OpenCode 也会被再写一遍，但它们的登录过程自己已经写过 CLI 文件。收成：有 IDE 适配器（Antigravity、Cursor），或者 catalog 是 `xai`。

判定：就这么做。Cursor 是规格要的新适配器。xAI 保留旧行为，因为同一次登录可能把这张卡改绑到另一个账号。

### Zed 私钥用 PKCS#1 DER

回调解密先试 OAEP-SHA256，再退到 PKCS#1 v1.5。密文同时接受标准 base64 和 URL-safe。本机没有对测试 service 跑 `security` 写回。

判定：解密算法就这么做。钥匙串写回仍是未验证，不是失败。
