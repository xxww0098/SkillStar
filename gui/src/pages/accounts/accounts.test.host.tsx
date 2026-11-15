// 账号页的无头测试装置（githubSignIn.test.host.tsx 样板）：假桥注入
// AccountsWorkbench 的 state 工厂位，装置动作换脚本（切换 ok/err、刷新、
// OAuth 的 finish ok/err、本机导入、令牌导入），读数 signal 在假桥调用点
// 直接 set（波 1 教训：普通计数变量不触发重渲染）。
//
// nsh 0.5 起 press 做命中测试：添加对话框是 position:fixed 覆盖层，遮罩之下
// 的装置按钮会以 ERR_NSH_TEST_COVERED 拒绝——装置动作因此挂在 host 事件
// 通道上（host.on("harness")），测试用 app.host.send("harness", {action})
// 驱动。不触文件系统、不打真网、不写 $HOME。

import { Text, View, host, mount, signal, type JSX } from "nsh";
import { DialogHost } from "../../chrome/dialog";
import { AccountsWorkbench } from "./index";
import { AccountsProviderMenu } from "./rail";
import { createAccountsState } from "./state";
import type { AccountsBridge } from "./bridge";
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
    ],
    live: [
      { agent: "codex", identity: "primary@example.com", accountId: "codex-main", source: "cli" },
      { agent: "claude-code", identity: null, accountId: null, source: "cli" },
      { agent: "cursor", identity: null, accountId: null, source: "cli" },
      { agent: "devin-desktop", identity: null, accountId: null, source: "cli" },
      { agent: "kiro", identity: null, accountId: null, source: "cli" },
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
        resolve({ account: clone(target), previous, target: agent });
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
  startLogin(agent) {
    calls.start += 1;
    bump();
    void agent;
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
  "last-call": () => {
    lastCall.set(
      `callback=${lastCallback === "" ? "none" : lastCallback} token=${
        lastToken === "" ? "none" : lastToken
      } pending=${pendingLogin?.loginId ?? "none"}`,
    );
  },
  reset: () => {
    const fresh = seed();
    domain.rows = fresh.rows;
    domain.live = fresh.live;
    for (const key of Object.keys(calls)) calls[key] = 0;
    lastCallback = "";
    lastToken = "";
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

const state = createAccountsState(fakeBridge);

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

function App(): JSX.Element {
  return (
    <View style={{ position: "relative", display: "flex", flexDirection: "column", height: "100%" }}>
      <Readout />
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
