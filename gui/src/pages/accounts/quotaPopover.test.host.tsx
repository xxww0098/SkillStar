// 全局账号额度下拉窗测试装置：纯内存假桥注入 QuotaPopoverPanel，不碰真实文件系统或网络。

import { Text, View, effect, host, mount, signal, type JSX, type ViewRef } from "nsh";
import type { AccountsBridge } from "./bridge";
import type { AutoRefreshPrefs } from "./prefs";
import { QuotaPopoverPanel } from "./quotaPopover";
import { createAccountsState } from "./state";
import type {
  AccountView,
  ActivateOutcome,
  AgentLive,
  AgentRow,
  Credits,
  LoginStart,
  UsageWindow,
} from "./types";

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
    id: "antigravity",
    displayName: "Antigravity",
    supportsOauth: true,
    localSource: "Antigravity keychain",
  },
  { id: "grok", displayName: "Grok", supportsOauth: true, localSource: "~/.grok/auth.json" },
  { id: "zcode", displayName: "Zcode", supportsOauth: true, localSource: "~/.zcode/auth.json" },
];

const NOW = 1763200000;

function seedRows(): AccountView[] {
  return [
    {
      id: "cx-active",
      agent: "codex",
      displayName: "Alice Dev",
      email: "alice@openai.example",
      plan: "pro",
      createdAt: 1,
      lastUsedAt: NOW - 60,
      requiresReauth: false,
      isActive: true,
      usage: {
        fetchedAt: NOW,
        plan: "pro",
        windows: [
          {
            key: "five_hour",
            label: "5h",
            remainingPercent: 84,
            unit: "percent",
            resetsAt: NOW + 7200,
          },
          { key: "week", label: "Weekly", remainingPercent: 48, unit: "percent" },
        ],
        resetCredits: 2,
      },
    },
    {
      id: "cx-low",
      agent: "codex",
      displayName: "Backup Low",
      email: "low@openai.example",
      plan: "plus",
      createdAt: 2,
      requiresReauth: false,
      isActive: false,
      usage: {
        fetchedAt: NOW,
        plan: "plus",
        windows: [{ key: "five_hour", label: "5h", remainingPercent: 30, unit: "percent" }],
      },
    },
    {
      id: "cx-full",
      agent: "codex",
      displayName: "Backup Ultra",
      email: "ultra@openai.example",
      plan: "ultra",
      createdAt: 3,
      requiresReauth: false,
      isActive: false,
      usage: {
        fetchedAt: NOW,
        plan: "ultra",
        windows: [
          { key: "five_hour", label: "5h", remainingPercent: 98, unit: "percent" },
          { key: "week", label: "Weekly", remainingPercent: 92, unit: "percent" },
        ],
      },
    },
    {
      id: "cc-expired",
      agent: "claude-code",
      displayName: "Work Claude",
      email: "claude@work.example",
      plan: "team",
      createdAt: 4,
      requiresReauth: true,
      isActive: true,
      usage: null,
    },
  ];
}

/** 长列表样本：5 个 Agent、卡片窗口多，内容高于内容视口上限（滚动盒回归用）。 */
function tallRows(): AccountView[] {
  const win = (key: string, label: string, remainingPercent: number): UsageWindow => ({
    key,
    label,
    remainingPercent,
    unit: "percent",
  });
  const points = (label: string, remaining: number): Credits => ({
    label,
    remaining,
    unlimited: false,
    unit: "count",
  });
  return [
    {
      id: "cx-tall",
      agent: "codex",
      displayName: "xie wei",
      email: "xie@openai.example",
      plan: "pro",
      createdAt: 21,
      requiresReauth: false,
      isActive: true,
      usage: {
        fetchedAt: NOW,
        windows: [win("week", "Weekly", 62)],
        credits: points("Credits", 36735.3),
      },
    },
    {
      id: "cur-tall",
      agent: "cursor",
      displayName: "Shefkie Etem",
      email: "shefkie@cursor.example",
      plan: "ultra",
      createdAt: 22,
      requiresReauth: false,
      isActive: true,
      usage: {
        fetchedAt: NOW,
        windows: [
          win("auto", "Auto + Composer", 99),
          win("api", "API", 40),
          win("total", "Total", 80),
        ],
      },
    },
    {
      id: "ag-tall",
      agent: "antigravity",
      displayName: "Kodram Dritken",
      email: "kodram@google.example",
      plan: "G1-ULTRA-LITE-TIER",
      createdAt: 23,
      requiresReauth: false,
      isActive: true,
      usage: {
        fetchedAt: NOW,
        windows: [
          win("gemini-week", "Gemini Models · Weekly", 31),
          win("gemini-5h", "Gemini Models · 5h", 93),
          win("claude-week", "Claude and GPT models · Weekly", 43),
          win("claude-5h", "Claude and GPT models · 5h", 100),
        ],
      },
    },
    {
      id: "grok-tall",
      agent: "grok",
      displayName: "Shefkie Etem",
      email: "shefkie@grok.example",
      plan: "free",
      createdAt: 24,
      requiresReauth: false,
      isActive: true,
      usage: {
        fetchedAt: NOW,
        windows: [win("five_hour", "5h", 77)],
        credits: points("Credits", 12),
      },
    },
    {
      id: "zc-tall",
      agent: "zcode",
      displayName: "xxww0098",
      email: "xxww@zcode.example",
      plan: "pro",
      createdAt: 25,
      requiresReauth: false,
      isActive: true,
      usage: {
        fetchedAt: NOW,
        windows: [win("five_hour", "5h", 51), win("week", "Weekly", 66)],
      },
    },
  ];
}

/** 单组样本（内容远低于视口上限）：面板应收缩到内容高度。 */
function singleRows(): AccountView[] {
  return [tallRows()[0]];
}

const TALL_LIVE: AgentLive[] = [
  { agent: "codex", identity: "xie@openai.example", accountId: "cx-tall", source: "cli" },
  { agent: "cursor", identity: "shefkie@cursor.example", accountId: "cur-tall", source: "vscdb" },
  {
    agent: "antigravity",
    identity: "kodram@google.example",
    accountId: "ag-tall",
    source: "cli",
  },
  { agent: "grok", identity: "shefkie@grok.example", accountId: "grok-tall", source: "cli" },
  { agent: "zcode", identity: "xxww@zcode.example", accountId: "zc-tall", source: "cli" },
];

const SINGLE_LIVE: AgentLive[] = [TALL_LIVE[0]];

let rows = seedRows();
let live: AgentLive[] = [
  { agent: "codex", identity: "alice@openai.example", accountId: "cx-active", source: "cli" },
  { agent: "claude-code", identity: "claude@work.example", accountId: "cc-expired", source: "cli" },
  { agent: "cursor", identity: "local-only@cursor.example", accountId: null, source: "vscdb" },
];

const actionLog = signal("idle");
const refreshCount = signal(0);
let nextActivateErr: string | null = null;
let nextRefreshErr: string | null = null;

function clone<T>(v: T): T {
  return JSON.parse(JSON.stringify(v)) as T;
}

const fakeBridge: AccountsBridge = {
  async list() {
    return clone(rows);
  },
  async reconcile() {
    return clone(live);
  },
  async refresh(id) {
    refreshCount.set(refreshCount() + 1);
    if (nextRefreshErr !== null) {
      const err = nextRefreshErr;
      nextRefreshErr = null;
      throw err;
    }
    const nowSecs = Math.floor(Date.now() / 1000);
    rows = rows.map((r) => {
      if (r.id !== id || r.usage === null || r.usage === undefined) return r;
      return {
        ...r,
        usage: {
          ...r.usage,
          fetchedAt: nowSecs,
        },
      };
    });
    const found = rows.find((r) => r.id === id) ?? rows[0];
    return clone(found);
  },
  async activate(id): Promise<ActivateOutcome> {
    if (nextActivateErr !== null) {
      const err = nextActivateErr;
      nextActivateErr = null;
      throw err;
    }
    const target = rows.find((r) => r.id === id)!;
    let previous: string | null = null;
    rows = rows.map((r) => {
      if (r.agent !== target.agent) return r;
      if (r.isActive) previous = r.id;
      return { ...r, isActive: r.id === id };
    });
    live = live.map((l) =>
      l.agent === target.agent ? { ...l, accountId: id } : l,
    );
    const updated = rows.find((r) => r.id === id)!;
    actionLog.set(`switched:${id}`);
    return { account: clone(updated), previous, target: target.agent };
  },
  async remove() {
    return null;
  },
  agents() {
    return AGENTS.map((a) => ({ ...a }));
  },
  async startLogin(): Promise<LoginStart> {
    return {
      loginId: "p-1",
      flow: "local-callback",
      authUrl: "https://example.com/auth",
    };
  },
  async submitOAuthCallback() {
    return null;
  },
  async finishLogin() {
    return clone(rows[0]);
  },
  cancelOAuthLogin() {},
  async importLocal() {
    return clone(rows[0]);
  },
  async importToken() {
    return clone(rows[0]);
  },
};

const memPrefs: AutoRefreshPrefs = {
  load: () => 120_000,
  save: () => {},
};

const state = createAccountsState(fakeBridge, memPrefs);

const panelH = signal("h:-");
const active = signal(true);
let panelRef: ViewRef | null = null;

host.on("popover-harness", (payload) => {
  const msg = (payload as { cmd?: string; tag?: string; active?: boolean } | null) ?? {};
  const cmd = msg.cmd;
  if (cmd === "measure") {
    // 面板外层包裹框的高度就是面板高度：整张卡片列表高于视口上限时它必须停在
    // QUOTA_POPOVER_MAX_H，说明列表在 <Scroll> 里滚，而不是被裁掉。
    const tag = msg.tag ?? "m";
    void (async () => {
      const m = await panelRef?.measure();
      const h = m === null || m === undefined ? -1 : Math.round(m.bounds.size.height);
      panelH.set(`h:${tag}:${h}`);
    })();
  } else if (cmd === "set-active") {
    active.set(msg.active !== false);
  } else if (cmd === "set-tall") {
    rows = tallRows();
    live = TALL_LIVE.map((entry) => ({ ...entry }));
    void state.load();
    state.reconcileLive();
  } else if (cmd === "set-single") {
    rows = singleRows();
    live = SINGLE_LIVE.map((entry) => ({ ...entry }));
    void state.load();
    state.reconcileLive();
  } else if (cmd === "clear-all") {
    rows = [];
    live = [];
    void state.load();
    state.reconcileLive();
  } else if (cmd === "refresh-stale") {
    state.refreshStale(60);
  } else if (cmd === "fail-activate") {
    nextActivateErr = "switch denied by client";
  } else if (cmd === "fail-refresh") {
    nextRefreshErr = "quota endpoint 503";
  } else if (cmd === "set-error") {
    state.error.set("popover list failed");
  } else if (cmd === "reset") {
    nextActivateErr = null;
    nextRefreshErr = null;
    state.dismissError();
    rows = seedRows();
    live = [
      { agent: "codex", identity: "alice@openai.example", accountId: "cx-active", source: "cli" },
      {
        agent: "claude-code",
        identity: "claude@work.example",
        accountId: "cc-expired",
        source: "cli",
      },
      { agent: "cursor", identity: "local-only@cursor.example", accountId: null, source: "vscdb" },
    ];
    state.setAutoRefreshInterval(120_000);
    state.autoRefreshSlots.set({});
    void (async () => {
      await state.load();
      state.reconcileLive();
      refreshCount.set(0);
      actionLog.set("idle");
    })();
  }
});

function App(): JSX.Element {
  effect(() => {
    state.start();
    state.reconcileLive();
  });

  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 8, padding: 12 }}>
      <Text testID="popover-log" style={{ fontSize: 10 }}>
        {`log:${actionLog()} refreshes:${refreshCount()}`}
      </Text>
      <Text testID="popover-panel-h" style={{ fontSize: 10 }}>
        {panelH()}
      </Text>
      <View
        ref={(node) => {
          panelRef = node;
        }}
      >
        <QuotaPopoverPanel
          state={state}
          active={active()}
          onOpenWorkbench={(agent) => actionLog.set(`workbench:${agent ?? "all"}`)}
          onOpenAdd={(agent) => actionLog.set(`add:${agent ?? "default"}`)}
          onReauthAccount={(id) => actionLog.set(`reauth:${id}`)}
        />
      </View>
    </View>
  );
}

export default App;

mount(App);
