// 账号添加弹窗与三路录入状态机（OAuth / 本地导入 / Token-JSON）。
// 由 `state.ts` 组合进 `AccountsState`，对外接口与导出均从 `state.ts` 统一暴露。

import { Clipboard, openUrl, signal, type Signal } from "nsh";
import { t } from "../../services/translation";
import type { AccountsBridge } from "./bridge";
import type { AccountView, AgentRow, LoginStart, StartLoginOptions } from "./types";

/** 空输入的表单错误键（渲染时 t()）。 */
export const FORM_NEED_CALLBACK = "accountsPage.needCallback";
export const FORM_NEED_TOKEN = "accountsPage.needToken";

export function isFormError(message: string | null): boolean {
  return message === FORM_NEED_CALLBACK || message === FORM_NEED_TOKEN;
}

/** 添加对话框顶部三 Tab。 */
export type AddStep = "oauth" | "token" | "local";
export type AddTab = AddStep;

/** 添加对话框的在途动作（非空 = 该动作的 spinner）。 */
export type AddBusy = "start" | "callback" | "finish" | "import" | "token" | null;

export interface AddDialogState {
  /** 非空 = 对话框为该 agent 打开。 */
  agent: Signal<string | null>;
  step: Signal<AddStep>;
  /** 等待中的登录（OAuth 段的全部渲染输入）。 */
  login: Signal<LoginStart | null>;
  /** 回调粘贴框（不是令牌框）。 */
  callback: Signal<string>;
  /** 令牌导入框。 */
  token: Signal<string>;
  /** 多路 OAuth 时选中的登录腿（如 Kiro portal/idc 或 ZCode zai/bigmodel）。 */
  oauthMode: Signal<string>;
  /** Kiro IdC Region 输入（留空由域层默认 us-east-1）。 */
  idcRegion: Signal<string>;
  /** Kiro IdC Start URL 输入（留空由域层默认 Builder ID）。 */
  idcStartUrl: Signal<string>;
  error: Signal<string | null>;
  busy: Signal<AddBusy>;
  /** 复制链接/配对码后的短暂确认。 */
  copied: Signal<boolean>;
}

export interface AddFlowController {
  readonly add: AddDialogState;
  openAdd(agent: string): void;
  selectAddTab(tab: AddStep): void;
  selectOauthMode(modeId: string): void;
  closeAdd(): void;
  startLogin(): void;
  submitCallback(): void;
  importLocal(): void;
  submitToken(): void;
  copyAuthText(text: string): void;
  reauth(id: string): void;
}

export function createAddFlow(deps: {
  bridge: AccountsBridge;
  agents: Signal<AgentRow[]>;
  accounts: Signal<AccountView[]>;
  selected: Signal<string | null>;
  succeed(msg: string): void;
  applyCreatedAccount(created: AccountView | null | undefined): void;
}): AddFlowController {
  const { bridge, agents, accounts, selected, succeed, applyCreatedAccount } = deps;

  const add: AddDialogState = {
    agent: signal<string | null>(null),
    step: signal<AddStep>("oauth"),
    login: signal<LoginStart | null>(null),
    callback: signal(""),
    token: signal(""),
    oauthMode: signal(""),
    idcRegion: signal(""),
    idcStartUrl: signal(""),
    error: signal<string | null>(null),
    busy: signal<AddBusy>(null),
    copied: signal(false),
  };

  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  function clearCopyFeedback(): void {
    if (copyTimer !== null) {
      clearTimeout(copyTimer);
      copyTimer = null;
    }
    add.copied.set(false);
  }

  function supportsOauthFor(agentId: string): boolean {
    return agents().find((a) => a.id === agentId)?.supportsOauth !== false;
  }

  function defaultOauthModeFor(agentId: string): string {
    return agents().find((a) => a.id === agentId)?.oauthModes?.[0]?.id ?? "";
  }

  function buildStartLoginOptions(agentId: string): StartLoginOptions | undefined {
    const modes = agents().find((a) => a.id === agentId)?.oauthModes ?? [];
    if (modes.length === 0) return undefined;
    const cur = add.oauthMode().trim();
    const active = modes.find((m) => m.id === cur) ?? modes[0];
    const opts: StartLoginOptions = {};
    if (active.optionKey === "provider") {
      opts.provider = active.id;
    } else {
      opts.leg = active.id;
    }
    if (active.fields?.includes("idcRegion")) {
      const region = add.idcRegion().trim();
      if (region.length > 0) opts.idcRegion = region;
    }
    if (active.fields?.includes("idcStartUrl")) {
      const startUrl = add.idcStartUrl().trim();
      if (startUrl.length > 0) opts.idcStartUrl = startUrl;
    }
    return opts;
  }

  function defaultTabFor(agent: string): AddStep {
    return supportsOauthFor(agent) ? "oauth" : "local";
  }

  function cancelPendingOAuth(): void {
    const pending = add.login();
    if (pending !== null) {
      try {
        bridge.cancelOAuthLogin(pending.loginId);
      } catch {
        // 会话已消失：同样无事可做。
      }
    }
  }

  function resetAdd(): void {
    clearCopyFeedback();
    add.agent.set(null);
    add.step.set("oauth");
    add.login.set(null);
    add.callback.set("");
    add.token.set("");
    add.oauthMode.set("");
    add.idcRegion.set("");
    add.idcStartUrl.set("");
    add.error.set(null);
    add.busy.set(null);
  }

  function closeAdd(): void {
    cancelPendingOAuth();
    resetAdd();
  }

  function openAdd(agent: string): void {
    resetAdd();
    selected.set(agent);
    add.agent.set(agent);
    add.oauthMode.set(defaultOauthModeFor(agent));
    add.step.set(defaultTabFor(agent));
  }

  function selectOauthMode(modeId: string): void {
    if (add.busy() !== null) return;
    add.oauthMode.set(modeId);
    add.error.set(null);
  }

  function selectAddTab(tab: AddStep): void {
    const curBusy = add.busy();
    if (curBusy !== null && curBusy !== "finish") return;
    if (tab !== add.step() && add.login() !== null) {
      cancelPendingOAuth();
      add.login.set(null);
      add.busy.set(null);
      add.callback.set("");
    }
    clearCopyFeedback();
    add.error.set(null);
    add.step.set(tab);
  }

  /** 登录器：起 OAuth → 等待段 → finish 落地。 */
  function startLogin(): void {
    const agent = add.agent();
    if (agent === null || add.busy() !== null || !supportsOauthFor(agent)) return;
    add.busy.set("start");
    add.error.set(null);
    add.step.set("oauth");
    const opts = buildStartLoginOptions(agent);
    const loginPromise = opts !== undefined ? bridge.startLogin(agent, opts) : bridge.startLogin(agent);
    loginPromise.then(
      (start) => {
        if (add.agent() === null) {
          try {
            bridge.cancelOAuthLogin(start.loginId);
          } catch {
            // 会话已消失：同样无事可做。
          }
          return;
        }
        add.busy.set("finish");
        add.login.set(start);
        const targetUrl = start.verificationUri || start.authUrl;
        if (targetUrl) {
          void openUrl(targetUrl);
        }
        bridge.finishLogin(start.loginId).then(
          (created) => {
            resetAdd();
            succeed(t("accountsPage.added"));
            applyCreatedAccount(created);
          },
          (err) => {
            if (add.agent() === null) return;
            // 后端 oauth::complete 已销毁会话：必须清 login，避免等待框与旋转图标永久空转。
            add.login.set(null);
            add.callback.set("");
            add.busy.set(null);
            add.error.set(String(err));
          },
        );
      },
      (err) => {
        add.busy.set(null);
        add.error.set(String(err));
        add.step.set("oauth");
      },
    );
  }

  function submitCallback(): void {
    const pending = add.login();
    if (pending === null || add.busy() !== "finish") return;
    const text = add.callback().trim();
    if (text.length === 0) {
      add.error.set(FORM_NEED_CALLBACK);
      return;
    }
    add.busy.set("callback");
    add.error.set(null);
    bridge.submitOAuthCallback(pending.loginId, text).then(
      () => {
        add.busy.set("finish");
      },
      (err) => {
        add.busy.set("finish");
        add.error.set(String(err));
      },
    );
  }

  function importLocal(): void {
    const agent = add.agent();
    if (agent === null || add.busy() !== null) return;
    add.busy.set("import");
    add.error.set(null);
    bridge.importLocal(agent).then(
      (created) => {
        resetAdd();
        succeed(t("accountsPage.importedLocal"));
        applyCreatedAccount(created);
      },
      (err) => {
        add.busy.set(null);
        add.error.set(String(err));
      },
    );
  }

  function submitToken(): void {
    const agent = add.agent();
    if (agent === null || add.busy() !== null) return;
    const payload = add.token().trim();
    if (payload.length === 0) {
      add.error.set(FORM_NEED_TOKEN);
      return;
    }
    add.busy.set("token");
    add.error.set(null);
    bridge.importToken(agent, payload).then(
      (created) => {
        resetAdd();
        succeed(t("accountsPage.importedToken"));
        applyCreatedAccount(created);
      },
      (err) => {
        add.busy.set(null);
        add.error.set(String(err));
      },
    );
  }

  function copyAuthText(text: string): void {
    const trimmed = text.trim();
    if (trimmed.length === 0) return;
    void Clipboard.writeText(trimmed).then(
      () => {
        clearCopyFeedback();
        add.copied.set(true);
        copyTimer = setTimeout(() => {
          copyTimer = null;
          add.copied.set(false);
        }, 1500);
      },
      () => {
        // 剪贴板不可用时静默忽略。
      },
    );
  }

  function reauth(id: string): void {
    const account = accounts().find((row) => row.id === id);
    if (account === undefined) return;
    openAdd(account.agent);
    if (supportsOauthFor(account.agent)) {
      add.step.set("oauth");
      startLogin();
    }
  }

  return {
    add,
    openAdd,
    selectAddTab,
    selectOauthMode,
    closeAdd,
    startLogin,
    submitCallback,
    importLocal,
    submitToken,
    copyAuthText,
    reauth,
  };
}
