// 账号页的无头测试装置（githubSignIn.test.host.tsx 样板）：假桥注入
// AccountsWorkbench 的 state 工厂位，装置动作换脚本（切换 ok/err、刷新、
// OAuth 的 finish ok/err、本机导入、令牌导入），读数 signal 在假桥调用点
// 直接 set（波 1 教训：普通计数变量不触发重渲染）。
//
// nsh 0.6 test 的 press 做命中测试：添加对话框是 position:fixed 覆盖层，遮罩之下
// 的装置按钮会以 ERR_NSH_TEST_COVERED 拒绝——装置动作因此挂在 host 事件
// 通道上（host.on("harness")），测试用 app.host.send("harness", {action})
// 驱动。不触文件系统、不打真网、不写 $HOME。

import { Text, View, effect, host, mount, signal, type JSX } from "nsh";
import { DialogHost } from "../../chrome/dialog";
import { subscribeNotify } from "../../shell/events";
import { AccountsWorkbench } from "./index";
import { AccountsProviderMenu } from "./rail";
import { createAccountsState } from "./state";
import type { AccountsBridge } from "./bridge";
import type { AutoRefreshPrefs } from "./prefs";
import type {
  AccountView,
  ActivateOutcome,
  AgentLive,
  AgentRow,
  LoginStart,
} from "./types";

// ---- 假域件 ---------------------------------------------------------------------

const AGENTS: AgentRow[] = [
  { id: "codex", displayName: "Codex", supportsOauth: true, localSource: "~/.codex/auth.json" },
  {
    id: "claude-code",
    displayName: "Claude Code",
    supportsOauth: false,
    localSource: "~/.claude/.credentials.json",
  },
  { id: "cursor", displayName: "Cursor", supportsOauth: true, localSource: "Cursor state.vscdb" },
  {
    id: "devin-desktop",
    displayName: "Devin Desktop",
    supportsOauth: false,
    localSource: "Devin Desktop state.vscdb",
  },
  {
    id: "kiro",
    displayName: "Kiro",
    supportsOauth: true,
    localSource: "~/.aws/sso/cache/kiro-auth-token.json",
    oauthModes: [
      { id: "portal", label: "Kiro Portal", optionKey: "leg", fields: [] },
      {
        id: "idc",
        label: "AWS IdC",
        optionKey: "leg",
        fields: ["idcRegion", "idcStartUrl"],
      },
    ],
  },
  {
    id: "zcode",
    displayName: "ZCode",
    supportsOauth: true,
    localSource: "~/.zcode/v2/credentials.json",
    oauthModes: [
      { id: "zai", label: "Z.ai", optionKey: "provider", fields: [] },
      { id: "bigmodel", label: "BigModel", optionKey: "provider", fields: [] },
    ],
  },
  {
    id: "antigravity",
    displayName: "Antigravity",
    supportsOauth: true,
    localSource: "Antigravity Keychain / state.vscdb",
  },
];

const NOW = 1763200000;

function account(over: Partial<AccountView>): AccountView {
  return {
    id: "x",
    agent: "codex",
    displayName: "x",
    email: null,
    plan: null,
    createdAt: 0,
    lastUsedAt: null,
    requiresReauth: false,
    isActive: false,
    usage: null,
    ...over,
  };
}

interface FakeDomain {
  rows: AccountView[];
  live: AgentLive[];
}

function seed(): FakeDomain {
  return {
    rows: [
      account({
        id: "codex-main",
        agent: "codex",
        displayName: "Primary Codex",
        email: "primary@example.com",
        plan: "pro",
        createdAt: 1,
        lastUsedAt: NOW - 120,
        isActive: true,
        usage: {
          fetchedAt: NOW,
          plan: "pro",
          windows: [
            {
              key: "five_hour",
              label: "5h",
              remainingPercent: 92,
              unit: "percent",
              resetsAt: NOW + 3 * 3600 + 4 * 60,
            },
            { key: "week", label: "Weekly", remainingPercent: 75, unit: "percent" },
          ],
          credits: { label: "Credits", remaining: 12.5, unlimited: false, unit: "count" },
          resetCredits: 2,
        },
      }),
      account({
        id: "codex-ultra",
        agent: "codex",
        displayName: "Backup Ultra",
        email: "ultra@example.com",
        plan: "ultra",
        createdAt: 2,
        lastUsedAt: NOW - 3600,
        isActive: false,
        usage: {
          fetchedAt: NOW,
          plan: "ultra",
          windows: [
            { key: "five_hour", label: "5h", remainingPercent: 64, unit: "percent" },
            { key: "week", label: "Weekly", remainingPercent: 25, unit: "percent" },
          ],
          resetCredits: 0,
        },
      }),
      account({
        id: "cursor-1",
        agent: "cursor",
        displayName: "Cursor Work",
        email: null,
        plan: "plus",
        createdAt: 3,
        requiresReauth: true,
        isActive: false,
        usage: {
          fetchedAt: NOW,
          plan: "plus",
          windows: [
            { key: "week", label: "Weekly", remainingPercent: 38, unit: "percent" },
          ],
          error: "quota endpoint refused the token",
        },
      }),
      account({
        id: "kiro-credits",
        agent: "kiro",
        displayName: "Kiro Credits",
        email: "kiro@example.com",
        plan: "pro",
        createdAt: 4,
        isActive: false,
        usage: {
          fetchedAt: NOW,
          plan: "pro",
          windows: [],
          credits: { label: "Credits", remaining: 12.5, unlimited: false, unit: "count" },
        },
      }),
      account({
        id: "ag-ultra",
        agent: "antigravity",
        displayName: "Example User",
        email: "user@example.com",
        plan: "G1-ULTRA-TIER",
        createdAt: 5,
        lastUsedAt: NOW - 60,
        isActive: true,
        usage: {
          fetchedAt: NOW,
          plan: "G1-ULTRA-TIER",
          windows: [
            {
              key: "gemini-weekly",
              label: "Gemini Models · Weekly Limit",
              remainingPercent: 63,
              unit: "percent",
              resetsAt: NOW + 6 * 86400 + 7 * 3600,
            },
            {
              key: "gemini-5h",
              label: "Gemini Models · Five Hour Limit",
              remainingPercent: 84,
              unit: "percent",
              resetsAt: NOW + 2 * 3600 + 40 * 60,
            },
            {
              key: "claude-weekly",
              label: "Claude and GPT models · Weekly Limit",
              remainingPercent: 99,
              unit: "percent",
              resetsAt: NOW + 6 * 86400 + 11 * 3600,
            },
            {
              key: "claude-5h",
              label: "Claude and GPT models · Five Hour Limit",
              remainingPercent: 100,
              unit: "percent",
              resetsAt: NOW + 5 * 3600,
            },
          ],
        },
      }),
    ],
    live: [
      { agent: "codex", identity: "primary@example.com", accountId: "codex-main", source: "cli" },
      { agent: "claude-code", identity: null, accountId: null, source: "cli" },
      { agent: "cursor", identity: null, accountId: null, source: "cli" },
      { agent: "devin-desktop", identity: null, accountId: null, source: "cli" },
      { agent: "kiro", identity: null, accountId: null, source: "cli" },
      { agent: "antigravity", identity: null, accountId: "ag-ultra", source: "keychain" },
    ],
  };
}

const domain = seed();

const calls: Record<string, number> = {
  list: 0,
  reconcile: 0,
  refresh: 0,
  activate: 0,
  remove: 0,
  start: 0,
  finish: 0,
  callback: 0,
  cancel: 0,
  importLocal: 0,
  importToken: 0,
};

/** 重渲染脉冲：假桥每次动作边界 +1，读数 JSX 借它重算（普通计数变量不
 * 触发重渲染——波 1 教训的装置版）。 */
const pulse = signal(0);
/** 最近一次调用的参数 dump。 */
const lastCall = signal("none");

let activateQueued: ((ok: boolean) => void) | null = null;
let finishQueued: ((ok: boolean) => void) | null = null;
let pendingLogin: LoginStart | null = null;
let lastCallback = "";
let lastToken = "";
let lastStartLogin = "";
let nextRefreshError: string | null = null;

function bump(): void {
  pulse.set(pulse() + 1);
}

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function startInfo(): LoginStart {
  return {
    loginId: "pending-1",
    flow: "local-callback",
    authUrl: "https://auth.example/oauth?redirect_uri=http%3A%2F%2F127.0.0.1%3A1455%2Fcallback",
    userCode: null,
    verificationUri: null,
    intervalSecs: null,
    expiresAt: null,
  };
}

const fakeBridge: AccountsBridge = {
  async list() {
    calls.list += 1;
    bump();
    return clone(domain.rows);
  },
  async reconcile() {
    calls.reconcile += 1;
    bump();
    return clone(domain.live);
  },
  async refresh(id) {
    calls.refresh += 1;
    bump();
    if (nextRefreshError !== null) {
      const err = nextRefreshError;
      nextRefreshError = null;
      throw err;
    }
    const row = domain.rows.find((entry) => entry.id === id);
    return clone(row ?? domain.rows[0]);
  },
  activate(id) {
    calls.activate += 1;
    bump();
    return new Promise<ActivateOutcome>((resolve, reject) => {
      activateQueued = (ok) => {
        if (!ok) {
          reject("switch refused by the client");
          return;
        }
        const agent = domain.rows.find((entry) => entry.id === id)?.agent ?? "codex";
        let previous: string | null = null;
        for (const row of domain.rows) {
          if (row.agent !== agent) continue;
          if (row.isActive) previous = row.id;
          row.isActive = row.id === id;
        }
        domain.live = domain.live.map((entry) =>
          entry.agent === agent ? { ...entry, accountId: id } : entry,
        );
        const target = domain.rows.find((entry) => entry.id === id)!;
        resolve({
          account: clone(target),
          previous,
          target: agent,
        });
      };
    });
  },
  async remove(id) {
    calls.remove += 1;
    bump();
    domain.rows = domain.rows.filter((entry) => entry.id !== id);
    return null;
  },
  agents() {
    return AGENTS.map((row) => ({ ...row }));
  },
  startLogin(agent, options) {
    calls.start += 1;
    bump();
    lastStartLogin = `${agent}|${JSON.stringify(options ?? null)}`;
    pendingLogin = startInfo();
    return Promise.resolve(clone(pendingLogin));
  },
  submitOAuthCallback(loginId, text) {
    calls.callback += 1;
    bump();
    lastCallback = `${loginId}|${text}`;
    return Promise.resolve(null);
  },
  finishLogin(loginId) {
    calls.finish += 1;
    bump();
    void loginId;
    return new Promise<AccountView>((resolve, reject) => {
      finishQueued = (ok) => {
        if (!ok) {
          reject("login was rejected by the provider");
          return;
        }
        const row = account({
          id: "codex-new",
          agent: "codex",
          displayName: "Fresh Login",
          email: "fresh@example.com",
          plan: "team",
          createdAt: 3,
        });
        domain.rows = [...domain.rows, clone(row)];
        resolve(row);
      };
    });
  },
  cancelOAuthLogin(loginId) {
    calls.cancel += 1;
    bump();
    void loginId;
  },
  async importLocal(agent) {
    calls.importLocal += 1;
    bump();
    const row = account({
      id: `${agent}-local`,
      agent,
      displayName: `${agent} (this device)`,
      createdAt: 4,
    });
    domain.rows = [...domain.rows, clone(row)];
    return row;
  },
  async importToken(agent, payload) {
    calls.importToken += 1;
    bump();
    lastToken = `${agent}|${payload}`;
    const row = account({
      id: `${agent}-token`,
      agent,
      displayName: "Token Import",
      createdAt: 5,
    });
    domain.rows = [...domain.rows, clone(row)];
    return row;
  },
};

// ---- 装置动作（host 事件通道） ---------------------------------------------------

const harnessActions: Record<string, () => void> = {
  "activate-ok": () => activateQueued?.(true),
  "activate-err": () => activateQueued?.(false),
  "finish-ok": () => finishQueued?.(true),
  "finish-err": () => finishQueued?.(false),
  "loading-on": () => {
    state.loading.set(true);
  },
  "loading-off": () => {
    state.loading.set(false);
  },
  "set-error": () => {
    state.error.set("list load failed from domain");
  },
  "refresh-err-401": () => {
    nextRefreshError = "login required: credential rejected: HTTP 401";
  },
  "refresh-err-403": () => {
    nextRefreshError = "usage unavailable: HTTP 403 Forbidden";
  },
  "last-call": () => {
    lastCall.set(
      `callback=${lastCallback === "" ? "none" : lastCallback} token=${
        lastToken === "" ? "none" : lastToken
      } pending=${pendingLogin?.loginId ?? "none"} start=${
        lastStartLogin === "" ? "none" : lastStartLogin
      }`,
    );
  },
  reset: () => {
    const fresh = seed();
    domain.rows = fresh.rows;
    domain.live = fresh.live;
    for (const key of Object.keys(calls)) calls[key] = 0;
    lastCallback = "";
    lastToken = "";
    lastStartLogin = "";
    nextRefreshError = null;
    pendingLogin = null;
    activateQueued = null;
    finishQueued = null;
    lastCall.set("none");
    bump();
  },
};

host.on("harness", (data) => {
  const action = (data as { action?: string } | null)?.action;
  if (action !== undefined) harnessActions[action]?.();
});

// ---- 装置 ---------------------------------------------------------------------

// 自动刷新周期的持久化在装置里走内存：测试不碰真实 gui_prefs.json（不写
// $HOME），只留「本次会话最后一次保存的值」，由 prefs 读数行断言。
const savedAutoRefreshMs = signal<number | null>(null);
const autoRefreshPrefs: AutoRefreshPrefs = {
  load: () => savedAutoRefreshMs(),
  save: (ms) => savedAutoRefreshMs.set(ms),
};

const state = createAccountsState(fakeBridge, autoRefreshPrefs);

/** 读数行：计数（借 pulse 重算）+ 对话框切面（直接读 state 的 signal）。 */
function Readout(): JSX.Element {
  return (
    <Text testID="host-readout" style={{ fontSize: 10 }}>
      {`list:${calls.list} reconcile:${calls.reconcile} activate:${calls.activate} refresh:${calls.refresh} start:${calls.start} finish:${calls.finish} callback:${calls.callback} cancel:${calls.cancel} import:${calls.importLocal} token:${calls.importToken} dialog:${
        state.add.agent() ?? "closed"
      }:${state.add.step()}:${state.add.busy() ?? "idle"} p:${pulse()}`}
    </Text>
  );
}

/** 添加/导入的成功反馈走 notify 频道的 toast（页面横幅只留切换与错误）。
 * 装置把它镜像成文本供 texts() 断言：不挂 NoticeBand——它的卡片盖在页面
 * 顶部，会挡住工具栏按钮的命中测试。 */
function NoticeMirror(): JSX.Element {
  const message = signal("none");
  effect(() => subscribeNotify((request) => message.set(request.message)));
  return (
    <Text testID="host-notice" style={{ fontSize: 10 }}>
      {message()}
    </Text>
  );
}

function App(): JSX.Element {
  return (
    <View style={{ position: "relative", display: "flex", flexDirection: "column", height: "100%" }}>
      <Readout />
      <Text testID="host-auto-refresh-pref" style={{ fontSize: 10 }}>
        {`prefs:${savedAutoRefreshMs() ?? "off"}`}
      </Text>
      <NoticeMirror />
      <Text testID="host-last" style={{ fontSize: 10 }}>
        {lastCall()}
      </Text>
      <View style={{ display: "flex", flexDirection: "row", flex: 1, minHeight: 0 }}>
        <View style={{ width: 176, padding: 8, flexShrink: 0 }}>
          <AccountsProviderMenu collapsed={false} />
        </View>
        <View style={{ display: "flex", flex: 1, minWidth: 0, minHeight: 0 }}>
          <AccountsWorkbench state={state} />
        </View>
      </View>
      <DialogHost />
    </View>
  );
}

export default App;

mount(App);
