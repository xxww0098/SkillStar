// 导入对话框的无头测试装置（githubSignIn.test.host.tsx 样板）：假
// ImportBridge/PackBridge 注入 ImportModalHost 的注入位，装置按钮驱动
// 脚本（扫描/安装结算、进度事件投放、迟到结算、分享码解析、Quick Pack
// 提交）并把调用计数、取消状态、groupsChanged 计数渲染成文本供
// app.texts() 断言。真实 120ms 折叠泵与真 git 链路由 native crate 的
// cargo test（my_skills_import.rs 本地目录回环）覆盖；本装置不触文件系
// 统、不打真网、不写 $HOME。
// nsh 0.6 test 的 press 会做命中测试：模态遮罩（importModalSlot 的
// position:fixed 覆盖层）之下的装置按钮会以 ERR_NSH_TEST_COVERED 拒绝，
// 因此同一批装置动作也挂在 host 事件通道上（见下方的装置动作表）。

import { Button } from "nsh/base";
import { Text, View, host, mount, signal, type JSX } from "nsh";
import {
  ImportModalHost,
  openImportModal,
  type ImportBridge,
  type PackBridge,
  type ScanView,
  type ShareView,
} from "./importModalSlot";
import { groupsChanged } from "../../shell/events";
import type { DomainJob } from "skillstar-gui/native";

// ---- 待结算调用 ----------------------------------------------------------------

interface PendingCall {
  kind: "scan" | "install" | "share" | "adopt" | "bundle";
  job: DomainJob;
  postEvent: (event: { phase: string; stage?: string | null; skill?: string | null }) => void;
  resolve: (value: unknown) => void;
  reject: (reason: string) => void;
}

const queue: PendingCall[] = [];
let scanCalls = 0;
let cancelSeen = "unknown";
let groupsSeen = 0;
const scanCount = signal(0);
const cancelledText = signal("unknown");
const groupsCount = signal(0);

/** 测试扫描结果：可装 demo-skill、internal 成员、带 frontmatter 问题的
 * 不可装成员（select.rs 三类行对位）。 */
function fakeScan(): ScanView {
  return {
    short: "owner/repo",
    repoUrl: "https://github.com/owner/repo",
    plugin: false,
    cacheHit: false,
    cachedAt: "2026-11-09T00:00:00Z",
    skills: [
      {
        id: "demo-skill",
        folderPath: "demo-skill",
        description: "A demo skill",
        alreadyInstalled: false,
        installable: true,
        internal: false,
        frontmatterIssues: [],
      },
      {
        id: "inner-tool",
        folderPath: "tools/inner-tool",
        description: "metadata.internal was true",
        alreadyInstalled: false,
        installable: true,
        internal: true,
        frontmatterIssues: [],
      },
      {
        id: "broken-skill",
        folderPath: "broken-skill",
        description: "frontmatter is not YAML",
        alreadyInstalled: false,
        installable: false,
        internal: false,
        frontmatterIssues: ["malformed_frontmatter"],
      },
    ],
  };
}

/** 分享码假语义（looks_like_share_code 的装置版）。 */
function looksLike(text: string): boolean {
  const trimmed = text.trim();
  return (trimmed.startsWith("ags-") || trimmed.startsWith("agd-")) && trimmed.length > 30;
}

function fakePreview(): ShareView {
  return {
    kind: "deck",
    name: "Pack",
    description: "Tools",
    icon: "",
    skills: [
      { n: "demo-skill", u: "https://github.com/owner/demo", c: null, p: null },
      { n: "embedded-one", u: "", c: "aGVsbG8=", p: null },
      { n: "secret-one", u: "ssh://git@corp.example.com:2222/team/secret.git", c: null, p: true },
    ],
  };
}

/** 待放行的分享码解析（装置按钮驱动——banner 时序可稳定断言）。 */
const parseQueue: ((preview: ShareView) => void)[] = [];

const bridge: ImportBridge = {
  looksLikeShareCode: (text) => looksLike(text),
  parseShareCode: (text) =>
    new Promise<ShareView>((resolve, reject) => {
      if (!looksLike(text)) {
        reject(new Error("Invalid share code prefix (expected ags- or agd-)"));
        return;
      }
      parseQueue.push(resolve);
    }),
  installedNames: () => Promise.resolve(["demo-skill"]),
  repoHistory: () => Promise.resolve(["owner/repo", "other/repo"]),
  scanRepo: (input, _depth, _refresh, job, onEvent) =>
    new Promise((resolve, reject) => {
      scanCalls += 1;
      scanCount.set(scanCalls);
      queue.push({
        kind: "scan",
        job,
        postEvent: onEvent as (event: { phase: string; stage?: string | null; skill?: string | null }) => void,
        resolve: (value) => resolve(value as ScanView),
        reject,
      });
    }),
  installSkills: (_short, _targets, job, onEvent) =>
    new Promise((resolve, reject) => {
      queue.push({
        kind: "install",
        job,
        postEvent: onEvent,
        resolve: (value) => resolve(value as { installed: string[]; short: string }),
        reject,
      });
    }),
  installShare: (_preview, job) =>
    new Promise((resolve, reject) => {
      queue.push({
        kind: "share",
        job,
        postEvent: () => {},
        resolve: (value) => resolve(value as never),
        reject,
      });
    }),
  adoptFolder: (_path, job) =>
    new Promise((resolve, reject) => {
      queue.push({
        kind: "adopt",
        job,
        postEvent: () => {},
        resolve: (value) => resolve(value as { names: string[] }),
        reject,
      });
    }),
  importBundle: (_path, _fallback, _label, job) =>
    new Promise((resolve, reject) => {
      queue.push({
        kind: "bundle",
        job,
        postEvent: () => {},
        resolve: (value) => resolve(value as { installed: string[]; deckCreated: boolean }),
        reject,
      });
    }),
  installAudits: () => Promise.resolve([]),
  pickFolder: () => Promise.resolve(null),
  pickBundleFile: () => Promise.resolve(null),
};

let packCalls = 0;
const packCount = signal(0);
const packBridge: PackBridge = {
  commitDeck: (_name, _members) =>
    new Promise((resolve, reject) => {
      packCalls += 1;
      packCount.set(packCalls);
      queue.push({
        kind: "adopt" as PendingCall["kind"],
        job: undefined as unknown as DomainJob,
        postEvent: () => {},
        resolve: () => resolve(),
        reject,
      });
    }),
};

// ---- 装置动作 -------------------------------------------------------------------

function settle(kind: PendingCall["kind"], action: (call: PendingCall) => void): void {
  const index = queue.findIndex((call) => call.kind === kind);
  if (index === -1) return;
  const [call] = queue.splice(index, 1);
  action(call);
}

function refreshCancelled(): void {
  const call = queue[0];
  if (call === undefined) {
    cancelledText.set(cancelSeen === "unknown" ? "no-call" : cancelSeen);
    return;
  }
  cancelSeen = String(call.job.cancelled);
  cancelledText.set(cancelSeen);
}

// ---- 装置动作表 ---------------------------------------------------------------

// 0.5 起 nsh test 的 `press` 会做命中测试：模态遮罩（见 importModalSlot 的
// position:fixed 覆盖层）之下的装置按钮会以 ERR_NSH_TEST_COVERED 拒绝。装置
// 因此把同一批动作同时挂到 host 事件通道（nsh 给测试的驱动面，见
// nsh/test 的 AppHost.send），测试用 app.host.send("harness", {action}) 驱动，
// 按钮保留给看得见的路径与同在装置内的断言。
const harnessActions: Record<string, () => void> = {
  open: () => openImportModal(),
  "parse-resolve": () => {
    const resolve = parseQueue.shift();
    resolve?.(fakePreview());
  },
  "scan-resolve": () => settle("scan", (call) => call.resolve(fakeScan())),
  "scan-reject": () => settle("scan", (call) => call.reject("network: dns broke")),
  "scan-reject-cancelled": () => settle("scan", (call) => call.reject("cancelled: by test")),
  "emit-progress": () => {
    const call = queue[0];
    call?.postEvent({ phase: "running", stage: "discovering", skill: "demo-skill" });
  },
  "install-resolve": () =>
    settle("install", (call) =>
      call.resolve({ installed: ["demo-skill"], short: "owner/repo" }),
    ),
  "share-resolve": () =>
    settle("share", (call) =>
      call.resolve({
        requested: 3,
        existing: ["demo-skill"],
        installed: ["embedded-one"],
        skipped: [{ name: "secret-one", reason: "unsupported_source" }],
        deckCreated: true,
      }),
    ),
  "pack-resolve": () => settle("adopt", (call) => call.resolve(undefined)),
  "pack-reject": () => settle("adopt", (call) => call.reject("deck name already exists")),
  cancelled: refreshCancelled,
  reset: () => {
    queue.length = 0;
    parseQueue.length = 0;
    scanCalls = 0;
    packCalls = 0;
    cancelSeen = "unknown";
    scanCount.set(0);
    packCount.set(0);
    cancelledText.set("unknown");
  },
};

host.on("harness", (data) => {
  const action = (data as { action?: string } | null)?.action;
  if (action !== undefined) harnessActions[action]?.();
});

// ---- 装置 ---------------------------------------------------------------------

function App(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", height: "100%", gap: 6 }}>
      <View style={{ display: "flex", flexDirection: "row", gap: 6, flexWrap: "wrap" }}>
        <Button testID="host-open" onPress={harnessActions["open"]}>
          <Text>open</Text>
        </Button>
        <Button testID="host-parse-resolve" onPress={harnessActions["parse-resolve"]}>
          <Text>parse-ok</Text>
        </Button>
        <Button testID="host-scan-resolve" onPress={harnessActions["scan-resolve"]}>
          <Text>scan-ok</Text>
        </Button>
        <Button testID="host-scan-reject" onPress={harnessActions["scan-reject"]}>
          <Text>scan-err</Text>
        </Button>
        <Button
          testID="host-scan-reject-cancelled"
          onPress={harnessActions["scan-reject-cancelled"]}
        >
          <Text>scan-cancelled</Text>
        </Button>
        <Button testID="host-emit-progress" onPress={harnessActions["emit-progress"]}>
          <Text>progress</Text>
        </Button>
        <Button testID="host-install-resolve" onPress={harnessActions["install-resolve"]}>
          <Text>install-ok</Text>
        </Button>
        <Button testID="host-share-resolve" onPress={harnessActions["share-resolve"]}>
          <Text>share-ok</Text>
        </Button>
        <Button testID="host-pack-resolve" onPress={harnessActions["pack-resolve"]}>
          <Text>pack-ok</Text>
        </Button>
        <Button testID="host-pack-reject" onPress={harnessActions["pack-reject"]}>
          <Text>pack-err</Text>
        </Button>
        <Button testID="host-cancelled" onPress={harnessActions["cancelled"]}>
          <Text>cancelled</Text>
        </Button>
        <Button testID="host-reset" onPress={harnessActions["reset"]}>
          <Text>reset</Text>
        </Button>
      </View>
      <Text testID="host-counters" style={{ fontSize: 12 }}>
        {`scan:${scanCount()} cancelled:${cancelledText()} pack:${packCount()} groups:${groupsCount()}`}
      </Text>
      <View style={{ flex: 1, minHeight: 0 }}>
        <ImportModalHost bridge={bridge} packBridge={packBridge} />
      </View>
    </View>
  );
}

groupsChanged.subscribe(() => {
  groupsSeen += 1;
  groupsCount.set(groupsSeen);
});

mount(App);
