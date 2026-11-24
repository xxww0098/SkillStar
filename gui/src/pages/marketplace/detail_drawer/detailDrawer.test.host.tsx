import { Text, View, batch, mount, signal, type JSX } from "nsh";
import { Button } from "nsh/base";
import { NoticeBand } from "../../../shell/notify";
import { DialogHost } from "../../../chrome/dialog";
import { translationPrefsChanged } from "../../../shell/events";
import { DETAIL_COLUMN_W } from "../../../layout";
import { parseSkill, type MarketSkill } from "../types";
import { createDetailReader, type SkillDetailPayload } from "./detailState";
import { createTranslateSession, type TranslateBridge } from "./translate";
import { MarketDetailColumn } from "./index";

// 市场详情列的无头测试装置（skill_cards/detailDrawer.test.host.tsx 样板）：
// 假 reader/syncer/翻译桥注入组件的注入位，装置按钮换卡、切 busy、预填
// 译文缓存、按脚本放行在途 Promise；调用与事件计数渲染成文本供
// app.texts() 断言。纯状态机（epoch 丢弃、回填刷新、翻译会话）在
// detailDrawer.test.tsx 直接 import 各模块自建实例断言，不经本装置；
// 真实域读写由 native crate 的 cargo test 用隔离数据目录覆盖。

export function detailPayload(
  overrides: Partial<SkillDetailPayload["data"]> = {},
  status = "fresh"
): SkillDetailPayload {
  return {
    data: {
      summary: null,
      readme: null,
      weeklyInstalls: null,
      githubStars: null,
      firstSeen: null,
      securityAudits: [],
      ...overrides,
    },
    snapshotStatus: status,
    snapshotUpdatedAt: null,
    error: null,
  };
}

// ---- 假域件 -----------------------------------------------------------------

/** 可控 reader：每次调用记一笔，装置按钮按脚本放行最旧一笔。 */
const readerCalls: {
  source: string;
  name: string;
  resolve: (payload: SkillDetailPayload) => void;
  reject: (reason: unknown) => void;
}[] = [];
const reader = (source: string, name: string) =>
  new Promise<SkillDetailPayload>((resolve, reject) => {
    readerCalls.push({ source, name, resolve, reject });
    readCount.set(readerCalls.length);
  });

const syncCalls: { scope: string; resolve: (v: void) => void; reject: (e: unknown) => void }[] = [];
const syncer = (scope: string) =>
  new Promise<void>((resolve, reject) => {
    syncCalls.push({ scope, resolve, reject });
  });

/** 假翻译桥：内存缓存 + 可控 fill。 */
const cache = new Map<string, string>();
const choices = new Map<string, boolean>();
const fillCalls: {
  sources: string[];
  resolve: (v: { done: { source: string; text: string }[]; rejection: { model: string; detail: string } | null }) => void;
  reject: (e: unknown) => void;
}[] = [];
/** fill 排队数（装置文本暴露，测试等它入队再放行）。 */
const fillCount = signal(0);
/** 在途读取排队数（同上；reader 微任务后才入队，测试等它）。 */
const readCount = signal(0);
const switchOn = signal(false);
const bridge: TranslateBridge = {
  descriptionsEnabled: () => switchOn(),
  descriptionChoice: (name) => (choices.has(name) ? (choices.get(name) as boolean) : null),
  needsTranslation: (text) => /[A-Za-z]/.test(text),
  lookup: (source) => cache.get(source) ?? null,
  blocked: () => false,
  setChoice: (name, translated) => {
    choices.set(name, translated);
  },
  fill: (sources) =>
    new Promise((resolve, reject) => {
      fillCalls.push({ sources, resolve, reject });
      fillCount.set(fillCount() + 1);
    }),
};

// ---- 场景 -------------------------------------------------------------------

function skillFixture(name: string, overrides: Partial<MarketSkill> = {}): MarketSkill {
  return {
    ...parseSkill({
      name,
      description: `${name} blurb`,
      stars: 0,
      source: "vercel-labs/skills",
    }),
    ...overrides,
  };
}

const ALPHA = skillFixture("alpha", {
  stars: 1200,
  author: "vercel",
  lastUpdated: "2026-02-04",
  gitUrl: "https://github.com/vercel-labs/skills",
});
const BETA = skillFixture("beta", {
  source: "mattpocock/skills",
  description: "Build deep modules for agents.",
});
const NO_SOURCE = skillFixture("local-only", { source: null, gitUrl: "" });

const picked = signal<MarketSkill | null>(ALPHA);
const pageBusy = signal(false);
const closed = signal(0);
const toggles = signal<string[]>([]);
/** 在途读取的放行脚本（装置按钮 host-resolve-first 消费）。 */
const script = signal<"fresh-summary" | "stale-summary" | "miss-empty" | "remote-error-empty" | "reject">("fresh-summary");

const detailReaderRaw = createDetailReader({ reader, syncer });
const detailReader = detailReaderRaw;
const session = createTranslateSession(bridge);

/** 按脚本放行最旧一笔在途读取。 */
function resolveFirst(): void {
  const call = readerCalls.shift();
  if (call === undefined) return;
  switch (script()) {
    case "fresh-summary":
      call.resolve(
        detailPayload(
          {
            summary: "Fresh alpha summary",
            readme: "# alpha full body",
            weeklyInstalls: "103.7K",
            githubStars: 42,
            firstSeen: "Feb 4, 2026",
            securityAudits: [{ name: "secrets", result: "pass" }],
          },
          "fresh"
        )
      );
      break;
    case "stale-summary":
      call.resolve(detailPayload({ summary: "Stale alpha summary" }, "stale"));
      break;
    case "miss-empty":
      call.resolve(detailPayload({}, "miss"));
      break;
    case "remote-error-empty":
      call.resolve({ ...detailPayload({}, "remote_error"), error: "remote blew up" });
      break;
    case "reject":
      call.reject(new Error("boom"));
      break;
  }
}

/** 放行最旧一笔在途同步。 */
function resolveFirstSync(): void {
  const call = syncCalls.shift();
  call?.resolve(undefined);
}

/** 放行最旧一笔在途翻译：域语义是成功即写盘（fill → remember），所以
 * resolve 同时把译文放进假缓存，列的下一次重算就能读到。 */
function resolveFirstFill(): void {
  const call = fillCalls.shift();
  if (call === undefined) return;
  const done = call.sources.map((source) => ({ source, text: "为代理构建深模块。" }));
  for (const item of done) {
    cache.set(item.source, item.text);
  }
  call.resolve({ done, rejection: null });
}

/** 放行最旧一笔在途翻译为一次模型拒绝。 */
function rejectFirstFill(): void {
  const call = fillCalls.shift();
  call?.resolve({ done: [], rejection: { model: "demo-model", detail: "refused" } });
}

/** 装置复位：nsh test 同文件多次 open() 共享本模块的状态（deck 装置的
 * host-reset-calls 同理）。不碰读取状态机的当前身份——把它留给下一个
 * pick 去换（换身份 = 全新打开 + 全新读取）；只放行遗留的在途 Promise
 * （它们应用到旧身份上，随即被弃）并把场景信号归位。picked 归位到无
 * source 的卡，保证每个测试随后的 pick 必然是身份变化。 */
function reset(): void {
  for (const call of readerCalls.splice(0)) {
    call.resolve(PENDING_DROP);
  }
  readCount.set(0);
  for (const sync of syncCalls.splice(0)) {
    sync.resolve(undefined);
  }
  for (const fill of fillCalls.splice(0)) {
    fill.resolve({ done: [], rejection: null });
  }
  fillCount.set(0);
  cache.clear();
  choices.clear();
  switchOn.set(false);
  pageBusy.set(false);
  script.set("fresh-summary");
  toggles.set([]);
  closed.set(0);
  picked.set(NO_SOURCE);
}

/** 复位时给遗留读取的空载荷：应用到旧身份上随即被弃（下一个 pick 换身
 * 份）；fresh 状态不触发回填同步。 */
const PENDING_DROP: SkillDetailPayload = {
  data: {
    summary: null,
    readme: null,
    weeklyInstalls: null,
    githubStars: null,
    firstSeen: null,
    securityAudits: [],
  },
  snapshotStatus: "fresh",
  snapshotUpdatedAt: null,
  error: null,
};

function App(): JSX.Element {
  return (
    <View style={{ width: "100%", height: "100%", display: "flex", flexDirection: "row", padding: 8, gap: 4 }}>
      <View style={{ display: "flex", flexDirection: "column", gap: 4, width: 180 }}>
        <Button testID="host-reset" onPress={reset}>
          <Text>reset</Text>
        </Button>
        <Button testID="host-pick-alpha" onPress={() => picked.set(ALPHA)}>
          <Text>pick-alpha</Text>
        </Button>
        <Button testID="host-pick-beta" onPress={() => picked.set(BETA)}>
          <Text>pick-beta</Text>
        </Button>
        <Button testID="host-pick-none" onPress={() => picked.set(NO_SOURCE)}>
          <Text>pick-none</Text>
        </Button>
        <Button testID="host-close-skill" onPress={() => picked.set(null)}>
          <Text>close-skill</Text>
        </Button>
        <Button testID="host-page-busy" onPress={() => pageBusy.set(!pageBusy())}>
          <Text>page-busy</Text>
        </Button>
        <Button
          testID="host-remember"
          onPress={() => cache.set("Build deep modules for agents.", "为代理构建深模块。")}
        >
          <Text>remember</Text>
        </Button>
        <Button
          testID="host-switch-on"
          onPress={() => {
            // 真实设置保存 = 写配置 + 发频道；用 batch 合并使效果在 prefsEpoch 更新后单次触发。
            batch(() => {
              switchOn.set(true);
              translationPrefsChanged.publish();
            });
          }}
        >
          <Text>switch-on</Text>
        </Button>
        <Button testID="host-mode-fresh" onPress={() => script.set("fresh-summary")}>
          <Text>mode-fresh</Text>
        </Button>
        <Button testID="host-mode-stale" onPress={() => script.set("stale-summary")}>
          <Text>mode-stale</Text>
        </Button>
        <Button testID="host-mode-reject" onPress={() => script.set("reject")}>
          <Text>mode-reject</Text>
        </Button>
        <Button testID="host-resolve-first" onPress={resolveFirst}>
          <Text>resolve-first</Text>
        </Button>
        <Button testID="host-resolve-sync" onPress={resolveFirstSync}>
          <Text>resolve-sync</Text>
        </Button>
        <Button testID="host-resolve-fill" onPress={resolveFirstFill}>
          <Text>resolve-fill</Text>
        </Button>
        <Button testID="host-reject-fill" onPress={rejectFirstFill}>
          <Text>reject-fill</Text>
        </Button>
        <Text testID="host-calls">{toggles().join(" ; ") || "none"}</Text>
        <Text testID="host-closed">closed:{closed()}</Text>
        <Text testID="host-fills">fills:{fillCount()}</Text>
        <Text testID="host-reads">
          reads:{readCount()}:{detailReader.phase().kind}:{detailReader.current()?.name ?? "-"}
        </Text>
      </View>
      {/* 固定宽轨道与页面同款（../index.tsx 的 DETAIL_COLUMN_W 轨道）：
          nsh 的 flex 子项默认不收缩，无轨道时 Sheet 的 100% 宽会把列撑
          出视口右缘、关闭钮被裁掉。 */}
      <View style={{ width: DETAIL_COLUMN_W, flexShrink: 0, height: "100%" }}>
        <MarketDetailColumn
          skill={picked()}
          identity={{
            source: picked() === null ? null : ((picked() as MarketSkill).source ?? null),
            name: picked() === null ? "" : (picked() as MarketSkill).name,
          }}
          busy={pageBusy()}
          onClose={() => closed.set(closed() + 1)}
          onToggleInstall={(skill) => toggles.set([...toggles(), `toggle:${skill.name}`])}
          reader={detailReader}
          session={session}
        />
      </View>
      <NoticeBand />
      <DialogHost />
    </View>
  );
}

mount(App);
