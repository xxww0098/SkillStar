// 账号页的桥 —— native/src/facades/accounts.rs（Accounts 模块）的 TS 半边。
// 页面与测试装置只面对 AccountsBridge：真实现是薄薄一层 JSON→类型收窄，
// 测试装置注入假桥（projects/bridge.ts、settings/bridge.ts 同款纪律）。
//
// 方法切面与 ss-accounts 的 facade 一一对应：
//   list / reconcile / refresh / activate / remove / agents /
//   start_login / submit_oauth_callback / finish_login / cancel_login /
//   add_from_token / add_from_local。
// 同步方法（纯表 / 无 IO 的取消）保持同步；其余返回 Promise。

import { Accounts } from "skillstar-gui/native";
import type {
  AccountView,
  ActivateOutcome,
  AgentLive,
  AgentRow,
  LoginStart,
} from "./types";

// ---- 收窄（JSON → DTO；字段名两侧都是 camelCase，只验类型） ----------------

function asAgentRows(value: unknown): AgentRow[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((row) => {
    const raw = (row ?? {}) as Partial<AgentRow>;
    if (typeof raw.id !== "string" || typeof raw.displayName !== "string") return [];
    const entry: AgentRow = { id: raw.id, displayName: raw.displayName };
    if (typeof raw.supportsOauth === "boolean") entry.supportsOauth = raw.supportsOauth;
    if (typeof raw.localSource === "string") entry.localSource = raw.localSource;
    return [entry];
  });
}

function asAccounts(value: unknown): AccountView[] {
  if (!Array.isArray(value)) return [];
  return value.filter(
    (row): row is AccountView =>
      typeof (row as Partial<AccountView> | null)?.id === "string" &&
      typeof (row as Partial<AccountView>).agent === "string"
  );
}

function asAccount(value: unknown): AccountView {
  return value as AccountView;
}

function asActivateOutcome(value: unknown): ActivateOutcome {
  return value as ActivateOutcome;
}

function asLive(value: unknown): AgentLive[] {
  if (!Array.isArray(value)) return [];
  return value.filter(
    (row): row is AgentLive => typeof (row as Partial<AgentLive> | null)?.agent === "string"
  );
}

function asLoginStart(value: unknown): LoginStart {
  return value as LoginStart;
}

/** 页面与装置面对的全部账号能力。 */
export interface AccountsBridge {
  /** ss_accounts::list：全部账号（最旧在前）。 */
  list(): Promise<AccountView[]>;
  /** ss_accounts::reconcile：每个 agent 此刻在服务谁。 */
  reconcile(): Promise<AgentLive[]>;
  /** ss_accounts::refresh：重读令牌与额度，返回新视图。 */
  refresh(id: string): Promise<AccountView>;
  /** ss_accounts::activate：让客户端切到这个账号。 */
  activate(id: string): Promise<ActivateOutcome>;
  /** ss_accounts::remove：删账号，客户端保持原样。 */
  remove(id: string): Promise<null>;
  /** 桥的三个 agent 行（id + 展示名）。纯、同步。 */
  agents(): AgentRow[];
  /** ss_accounts::start_login：开一次 OAuth 登录，返回待办会话。 */
  startLogin(agent: string): Promise<LoginStart>;
  /** ss_accounts::submit_login_callback：喂入粘贴的回调链接/授权码。 */
  submitOAuthCallback(loginId: string, text: string): Promise<null>;
  /** ss_accounts::finish_login：等到登录完成并落库，返回新账号。 */
  finishLogin(loginId: string): Promise<AccountView>;
  /** ss_accounts::cancel_login：丢弃待办登录。同步。 */
  cancelOAuthLogin(loginId: string): void;
  /** ss_accounts::add_from_token：粘贴令牌/凭据 JSON 导入。 */
  importToken(agent: string, payload: string): Promise<AccountView>;
  /** ss_accounts::add_from_local：把本机客户端已有的登录收编成账号。 */
  importLocal(agent: string): Promise<AccountView>;
}

/** 真桥：native facade 直调（页面默认使用）。 */
export const realAccountsBridge: AccountsBridge = {
  async list() {
    return asAccounts(await Accounts.list());
  },
  async reconcile() {
    return asLive(await Accounts.reconcile());
  },
  async refresh(id) {
    return asAccount(await Accounts.refresh(id));
  },
  async activate(id) {
    return asActivateOutcome(await Accounts.activate(id));
  },
  async remove(id) {
    await Accounts.delete(id);
    return null;
  },
  agents() {
    return asAgentRows(Accounts.agents());
  },
  async startLogin(agent) {
    return asLoginStart(await Accounts.startLogin(agent));
  },
  async submitOAuthCallback(loginId, text) {
    await Accounts.submitOauthCallback(loginId, text);
    return null;
  },
  async finishLogin(loginId) {
    return asAccount(await Accounts.finishLogin(loginId));
  },
  cancelOAuthLogin(loginId) {
    Accounts.cancelOauthLogin(loginId);
  },
  async importToken(agent, payload) {
    return asAccount(await Accounts.importToken(agent, payload));
  },
  async importLocal(agent) {
    return asAccount(await Accounts.importLocal(agent));
  },
};
