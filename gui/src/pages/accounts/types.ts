// 账号页 DTO 镜像 —— native/src/facades/accounts.rs 直传的 ss-accounts 视图
// 与额度快照。两侧 serde 都是 camelCase（AccountView 在
// crates/ss-accounts/src/lib.rs，UsageSnapshot/UsageWindow/Credits 在
// crates/ss-accounts/src/usage/snapshot.rs，LoginStart 在
// crates/ss-accounts/src/oauth/session.rs）；
// 桥是唯一事实源，这里只做收窄，不改名。

/** ss-accounts::Agent::id()：受管 agent 的 ss-skills 注册 id。 */
export type AgentId = string;

/** 单个受管 agent 支持的多路 OAuth 登录方式元数据（由桥接层下发）。 */
export interface OAuthModeOption {
  id: string;
  label: string;
  optionKey?: "leg" | "provider";
  fields?: string[];
}

/** Accounts.startLogin 可选配置（如 Kiro IdC 或 ZCode 上游）。 */
export interface StartLoginOptions {
  leg?: string;
  provider?: string;
  idcRegion?: string;
  idcStartUrl?: string;
  region?: string;
}

/** 桥 Accounts.agents() 的一行（能力与本地凭据来源元数据）。 */
export interface AgentRow {
  id: string;
  displayName: string;
  supportsOauth?: boolean;
  localSource?: string;
  oauthModes?: OAuthModeOption[];
}

/** ss-accounts UsageUnit（serde kebab-case）。 */
export type UsageUnit = "percent" | "count" | "usd-cents";

/** ss-accounts UsageWindow：remainingPercent 恒为「剩余」，0..=100。 */
export interface UsageWindow {
  /** five_hour / week / month / 供应商模型 id。 */
  key: string;
  /** 供应商对这一窗口的叫法（已知 key 由页面本地化）。 */
  label: string;
  remainingPercent: number;
  used?: number | null;
  total?: number | null;
  unit: UsageUnit;
  /** 重置时刻，Unix 秒。 */
  resetsAt?: number | null;
}

/** ss-accounts Credits：非窗口的点数/额度（unlimited 时 remaining 为空）。 */
export interface Credits {
  label: string;
  remaining?: number | null;
  unlimited: boolean;
  unit: UsageUnit;
}

/** ss-accounts UsageSnapshot：一次额度读取。 */
export interface UsageSnapshot {
  fetchedAt: number;
  windows: UsageWindow[];
  plan?: string | null;
  credits?: Credits | null;
  /** Codex 可用重置卡数量（rate_limit_reset_credits.available_count）。 */
  resetCredits?: number | null;
  /** 最近一次读取失败的原因（账号本身仍有效）。 */
  error?: string | null;
}

/** ss-accounts AccountView：UI 能看到的全部账号信息（凭据不出域）。 */
export interface AccountView {
  id: string;
  /** AgentId 之一（域侧是字符串，未知 id 由页面按空态处理）。 */
  agent: string;
  displayName: string;
  email?: string | null;
  accountId?: string | null;
  plan?: string | null;
  createdAt: number;
  lastUsedAt?: number | null;
  /** 存储的凭据已失效：卡面换「登录已失效」与重新授权行。 */
  requiresReauth: boolean;
  /** 客户端此刻真正在服务这个账号（磁盘真相）。 */
  isActive: boolean;
  usage?: UsageSnapshot | null;
}

/** ss-accounts AgentLive：每个 agent 此刻在服务谁（读不到时 identity 为空）。 */
export interface AgentLive {
  agent: string;
  identity?: string | null;
  displayName?: string | null;
  accountId?: string | null;
  source: string;
}

/** ss-accounts ActivateOutcome。 */
export interface ActivateOutcome {
  account: AccountView;
  previous?: string | null;
  target: string;
}

/** ss-accounts LoginFlow（serde kebab-case）。 */
export type LoginFlow = "local-callback" | "remote-poll" | "scheme-paste" | "immediate";

/** ss-accounts LoginStart：一次进行中的登录（对话框等待态渲染它）。 */
export interface LoginStart {
  loginId: string;
  flow: LoginFlow;
  authUrl: string;
  userCode?: string | null;
  verificationUri?: string | null;
  intervalSecs?: number | null;
  expiresAt?: number | null;
}

/** 该 flow 是否接受粘贴回调（remote-poll 只有配对码，immediate 无需动作）。 */
export function flowTakesPaste(flow: LoginFlow): boolean {
  return flow === "local-callback" || flow === "scheme-paste";
}
