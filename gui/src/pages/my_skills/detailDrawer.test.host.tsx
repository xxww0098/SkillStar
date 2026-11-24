import { Text, View, effect, mount, signal, untracked, type JSX } from "nsh";
import { Button } from "nsh/base";
import { NoticeBand } from "../../shell/notify";
import { DialogHost } from "../../chrome/dialog";
import { translationPrefsChanged } from "../../shell/events";
import { DetailDrawerSlot } from "./detailDrawerSlot";
import type { DrawerOps } from "./detailDrawerActions";
import { baselineMissing } from "./detailDrawerActions";
import { createDrawerTranslateSession, type DrawerTranslateBridge } from "./drawerTranslate";
import {
  busy,
  descriptionChoices,
  loading,
  profiles,
  selectedSkill,
  setSnapshotLoaderForTest,
  skills,
} from "./state";
import type { AgentProfileRecord, SkillRecord } from "./filters";

// detailDrawer 的无头测试装置（deck 页 detailDrawer.test.host.tsx 的样
// 板）：测试 realm 与 app bundle 隔离，夹具经页面 state 信号注入（与
// DetailDrawerTrack 同一派生：按 selectedSkill 查卡），装置按钮切场景，
// 调用与链接镜像渲染成文本供 app.texts() 断言。
//
// 真实域写入由 native crate 的 cargo test 用隔离数据目录覆盖；这里验证
// 抽屉的数据流与交互编排（哪条命令打哪个 ops、乐观翻链/回滚、pending
// 门、翻译选择面）。
//
// 装置纪律：
// - state.busy 先置 hold——装载尾的后台更新检查（maybeCheck_updates 的
//   busy 门）被挡住，本机不再发真实网络检查；抽屉 props 的 busyToken 用
//   装置镜像（hold 掩为 null）。
// - 首次真实 refresh（loading→false）落地后再播种夹具并清空描述选择，
//   之后 ops.refresh 全为假件，夹具不再被真实数据冲掉。

type Scene = "drawer-demo" | "brand" | "moved" | "gone" | "edited" | "fresh" | "zh" | "coexist" | "cached";

const AGENTS: AgentProfileRecord[] = [
  { id: "claude", displayName: "Claude Code", enabled: true, targetable: true },
  { id: "codex", displayName: "Code", enabled: true, targetable: true },
  { id: "gemini", displayName: "Gemini", enabled: false, targetable: false },
];

function fixture(name: string, extra: Partial<SkillRecord>): SkillRecord {
  return {
    name,
    description: "",
    skillType: "hub",
    stars: 0,
    installed: true,
    updateAvailable: false,
    upstreamChange: null,
    lastUpdated: "",
    gitUrl: "",
    agentLinks: [],
    ...extra,
  };
}

const FIXTURES: SkillRecord[] = [
  fixture("drawer-demo", {
    description: "Build deep modules for agents.",
    gitUrl: "https://github.com/nextlevelbuilder/ui-ux-pro-max.git",
    source: "nextlevelbuilder/ui-ux-pro-max",
    author: "nextlevelbuilder",
    lastUpdated: "2026-10-01T08:30:00Z",
    updateAvailable: true,
    agentLinks: ["Claude Code"],
  }),
  fixture("brand", { skillType: "local", source: "local/cursor" }),
  fixture("moved", {
    description: "Rename case.",
    upstreamChange: { kind: "identity_changed", upstreamName: "moved-on" },
  }),
  fixture("gone", {
    description: "Removed case.",
    source: "acme/repo",
    upstreamChange: { kind: "removed" },
  }),
  fixture("edited", {
    description: "Local edits case.",
    updateAvailable: true,
    upstreamChange: { kind: "local_changes" },
  }),
  fixture("fresh", { description: "Fresh uncached line." }),
  fixture("zh", { description: "为代理构建深模块。" }),
  fixture("coexist", {
    called: "demo",
    source: "other/repo",
    description: "Coexisting alias with modified copy and siblings.",
  }),
  fixture("cached", {
    description: "Cached English description.",
    localizedDescription: "已缓存的英文描述。",
  }),
];

// ---- 翻译假桥 -----------------------------------------------------------------

/** 磁盘缓存：demo 与 cached 的描述已译；fresh 没有（排一次 fill）。 */
const LOOKUP: Record<string, string> = {
  "Build deep modules for agents.": "为代理构建深模块。",
  "Cached English description.": "已缓存的英文描述。",
};

/** 让下一笔 fill 挂起（扫描态的闸）：放行前描述一直未落缓存，按钮停在
 * icon_sweep 的扫描态——GPUI 语言按钮的 pending 门就这样读。 */
const holdFill = signal(false);
const blocked = signal(false);
const missNextFill = signal(false);
let releaseFill: (() => void) | null = null;

const bridge: DrawerTranslateBridge = {
  descriptionsEnabled: () => false,
  needsTranslation: (text) => !/[\u3400-\u9fff\u3000-\u303f\uff00-\uffef]/.test(text),
  lookup: (source) => LOOKUP[source.trim()] ?? null,
  blocked: () => blocked(),
  setChoices: (choices) => {
    record(`choices:${JSON.stringify(choices)}`);
    return Promise.resolve();
  },
  fill: (sources) => {
    record(`fill:${sources[0]}`);
    if (missNextFill()) {
      missNextFill.set(false);
      return Promise.resolve({ done: [], rejection: null });
    }
    // 真实引擎成功即写盘：假件同样落缓存，会话下一轮 lookup 命中即停。
    const text = `「${sources[0]}」的缓存译文`;
    const land = () => {
      LOOKUP[sources[0]] = text;
      return { done: [{ source: sources[0], text }], rejection: null };
    };
    if (holdFill()) {
      holdFill.set(false);
      return new Promise((resolve) => {
        releaseFill = () => resolve(land());
      });
    }
    return Promise.resolve(land());
  },
};
const session = createDrawerTranslateSession(bridge);

// ---- ops 假件 -----------------------------------------------------------------

const calls = signal<string[]>([]);
function record(entry: string): void {
  calls.set([...untracked(() => calls()), entry]);
}

/** toggle 下一次的结局：applied / skip / skip-unmanaged / skip-foreign-link / fail。 */
const toggleOutcome = signal<"applied" | "skip" | "skip-unmanaged" | "skip-foreign-link" | "fail">("applied");
/** master 下一次的报告。 */
const masterReport = signal({ total: 1, failed: 0, skipped: 0, canonical: 0 });
/** 让下一次 master 挂起，直到 release（pending 门的闸）。 */
const holdMaster = signal(false);
let releaseGate: ((report: { total: number; failed: number; skipped: number; canonical: number }) => void) | null =
  null;

const OPS: DrawerOps = {
  toggleAgent: (skill, agentId, enable) => {
    record(`toggle:${skill}:${agentId}:${enable}`);
    if (holdMaster()) {
      holdMaster.set(false);
      return new Promise((resolve) => {
        releaseGate = () => resolve(
          toggleOutcome() === "skip"
            ? { kind: "skipped", code: "canonical_root_agent", path: "/~/.agents/skills", reason: "canonical" }
            : toggleOutcome() === "skip-unmanaged"
              ? { kind: "skipped", code: "unmanaged_real_directory", path: "/Users/test/.gemini/antigravity/skills/drawer-demo", reason: "unmanaged" }
              : { kind: "applied" }
        );
      });
    }
    if (toggleOutcome() === "skip") {
      return Promise.resolve({
        kind: "skipped",
        code: "canonical_root_agent",
        path: "/~/.agents/skills",
        reason: "canonical",
      });
    }
    if (toggleOutcome() === "skip-unmanaged") {
      return Promise.resolve({
        kind: "skipped",
        code: "unmanaged_real_directory",
        path: "/Users/test/.gemini/antigravity/skills/drawer-demo",
        reason: "unmanaged",
      });
    }
    if (toggleOutcome() === "skip-foreign-link") {
      return Promise.resolve({
        kind: "skipped",
        code: "foreign_link",
        path: "/Users/test/.gemini/antigravity/skills/drawer-demo",
        reason: "foreign link",
      });
    }
    if (toggleOutcome() === "fail") {
      return Promise.reject(new Error("boom"));
    }
    return Promise.resolve({ kind: "applied" });
  },
  setAgentLinks: (skill, agentIds, enable) => {
    record(`master:${skill}:${agentIds.join("+")}:${enable}`);
    if (holdMaster()) {
      holdMaster.set(false);
      return new Promise((resolve) => {
        releaseGate = resolve;
      });
    }
    return Promise.resolve(masterReport());
  },
  uninstallSkill: (name) => {
    record(`uninstall:${name}`);
    return Promise.resolve();
  },
  upstreamNote: (name) => {
    record(`note:${name}`);
    return Promise.resolve(null);
  },
  inspectDrawerExtras: (name) => {
    record(`extras:${name}`);
    if (name !== "coexist") {
      return Promise.resolve({ called: null, besideSkills: [], modifiedCopies: [], unmanagedEntries: [] });
    }
    return new Promise((resolve) => {
      setTimeout(
        () =>
          resolve({
            called: "demo",
            modifiedCopies: [
              {
                skillName: "coexist",
                agentId: "claude",
                agentDisplayName: "Claude Code",
                targetDir: "/agents/claude/coexist",
              },
            ],
            unmanagedEntries: [
              {
                skillName: "coexist",
                agentId: "antigravity",
                agentDisplayName: "Antigravity",
                targetDir: "/Users/test/.gemini/antigravity/skills/coexist",
                isSymlink: false,
                canAdoptToHub: true,
              },
            ],
            besideSkills: [
              {
                name: "sib-one",
                source: "other/repo",
                folderPath: "skills/sib-one",
              },
            ],
          }),
        0,
      );
    });
  },
  takeAgentCopyEdits: (skill, agentId) => {
    record(`adoptCopy:${skill}:${agentId}`);
    return Promise.resolve();
  },
  overwriteAgentCopyWithBackup: (skill, agentId) => {
    record(`overwriteCopy:${skill}:${agentId}`);
    return Promise.resolve({ backupId: "bak-test-1", path: "/backups/bak-test-1" });
  },
  takeOverUnmanagedAgentEntry: (skill, agentId) => {
    record(`takeOverUnmanaged:${skill}:${agentId}`);
    return Promise.resolve({ backupId: "bak-unmanaged-1", path: "/backups/bak-unmanaged-1" });
  },
  adoptUnmanagedAgentEntry: (skill, agentId) => {
    record(`adoptUnmanaged:${skill}:${agentId}`);
    return Promise.resolve();
  },
  revealPath: (path) => {
    record(`reveal:${path}`);
  },
  refresh: () => {
    record("refresh");
    return Promise.resolve();
  },
};

// ---- 装置状态 ------------------------------------------------------------------

// 挡住装载尾的后台更新检查（busy 非空即不查；见文件头）。
busy.set("host-hold");
// 防止后续 translationPrefsChanged 触发 state.ts 读写真实 $HOME 快照冲掉夹具。
setSnapshotLoaderForTest(async () => ({
  skills: untracked(skills).length > 0 ? untracked(skills) : FIXTURES,
  agents: untracked(profiles).length > 0 ? untracked(profiles) : AGENTS,
  favorites: [],
  favoritesLocked: false,
  descriptionChoices: untracked(descriptionChoices),
}));

const scene = signal<Scene>("drawer-demo");
const closed = signal(0);
/** 装置自有的抽屉开合（onClose 置 false；真实页里这是 selectedSkill）。 */
const hostOpen = signal(false);
let seeded = false;

// 首次真实 refresh 落地后播种夹具（此后不再有真实重载）。播种走微任务：
// loading 翻 false 发生在 refresh 的同步尾里，同步播种会被紧随的
// retainExisting(真实名单) 把 selectedSkill 清掉。
effect(() => {
  if (seeded || loading() !== false) return;
  seeded = true;
  queueMicrotask(() => {
    skills.set(FIXTURES);
    profiles.set(AGENTS);
    descriptionChoices.set({});
    selectedSkill.set("drawer-demo");
    hostOpen.set(true);
  });
});

// 场景切换走页面选择（DetailDrawerTrack 的同一派生）。
function setScene(next: Scene): void {
  scene.set(next);
  selectedSkill.set(next);
  hostOpen.set(true);
}

/** state.busy 的装置镜像：hold 掩为 null（GPUI busy 语义只认技能名/token）。 */
function liveBusy(): string | null {
  const value = busy();
  return value === "host-hold" ? null : value;
}

function App(): JSX.Element {
  return (
    <View
      style={{
        width: "100%",
        height: "100%",
        display: "flex",
        flexDirection: "row",
        padding: 8,
        gap: 4,
      }}
    >
      <View style={{ display: "flex", flexDirection: "column", gap: 4, width: 190 }}>
        <Button testID="host-scene-demo" onPress={() => setScene("drawer-demo")}>
          <Text>scene-demo</Text>
        </Button>
        <Button testID="host-scene-brand" onPress={() => setScene("brand")}>
          <Text>scene-brand</Text>
        </Button>
        <Button testID="host-scene-moved" onPress={() => setScene("moved")}>
          <Text>scene-moved</Text>
        </Button>
        <Button testID="host-scene-gone" onPress={() => setScene("gone")}>
          <Text>scene-gone</Text>
        </Button>
        <Button testID="host-scene-edited" onPress={() => setScene("edited")}>
          <Text>scene-edited</Text>
        </Button>
        <Button testID="host-scene-fresh" onPress={() => setScene("fresh")}>
          <Text>scene-fresh</Text>
        </Button>
        <Button testID="host-scene-zh" onPress={() => setScene("zh")}>
          <Text>scene-zh</Text>
        </Button>
        <Button testID="host-scene-coexist" onPress={() => setScene("coexist")}>
          <Text>scene-coexist</Text>
        </Button>
        <Button testID="host-scene-cached" onPress={() => setScene("cached")}>
          <Text>scene-cached</Text>
        </Button>
        <Button testID="host-toggle-blocked" onPress={() => blocked.set(!blocked())}>
          <Text>toggle-blocked</Text>
        </Button>
        <Button testID="host-miss-fill" onPress={() => missNextFill.set(true)}>
          <Text>miss-fill</Text>
        </Button>
        <Button testID="host-prefs-changed" onPress={() => translationPrefsChanged.publish()}>
          <Text>prefs-changed</Text>
        </Button>
        <Button testID="host-busy-demo" onPress={() => (busy() === "drawer-demo" ? busy.set("host-hold") : busy.set("drawer-demo"))}>
          <Text>busy-demo</Text>
        </Button>
        <Button testID="host-toggle-skip" onPress={() => toggleOutcome.set("skip")}>
          <Text>toggle-skip</Text>
        </Button>
        <Button testID="host-toggle-unmanaged" onPress={() => toggleOutcome.set("skip-unmanaged")}>
          <Text>toggle-unmanaged</Text>
        </Button>
        <Button testID="host-toggle-foreign-link" onPress={() => toggleOutcome.set("skip-foreign-link")}>
          <Text>toggle-foreign-link</Text>
        </Button>
        <Button testID="host-toggle-fail" onPress={() => toggleOutcome.set("fail")}>
          <Text>toggle-fail</Text>
        </Button>
        <Button testID="host-master-partial" onPress={() => masterReport.set({ total: 2, failed: 1, skipped: 1, canonical: 0 })}>
          <Text>master-partial</Text>
        </Button>
        <Button testID="host-master-canonical" onPress={() => masterReport.set({ total: 2, failed: 0, skipped: 0, canonical: 2 })}>
          <Text>master-canonical</Text>
        </Button>
        <Button testID="host-hold-fill" onPress={() => holdFill.set(true)}>
          <Text>hold-fill</Text>
        </Button>
        <Button
          testID="host-release-fill"
          onPress={() => {
            const release = releaseFill;
            releaseFill = null;
            release?.();
          }}
        >
          <Text>release-fill</Text>
        </Button>
        <Button testID="host-hold-master" onPress={() => holdMaster.set(true)}>
          <Text>hold-master</Text>
        </Button>
        <Button
          testID="host-release"
          onPress={() => {
            const gate = releaseGate;
            releaseGate = null;
            gate?.(masterReport());
          }}
        >
          <Text>release</Text>
        </Button>
        <Button testID="host-reset-calls" onPress={() => calls.set([])}>
          <Text>reset-calls</Text>
        </Button>
        <Button
          testID="host-no-agents"
          onPress={() =>
            profiles.set(
              profiles().length === 0
                ? AGENTS
                : AGENTS.map((agent) => ({ ...agent, targetable: false }))
            )
          }
        >
          <Text>no-agents</Text>
        </Button>
        <Button testID="host-baseline-missing" onPress={() => baselineMissing.set({ edited: true })}>
          <Text>baseline-missing</Text>
        </Button>
        <Text testID="host-calls">{calls().join(" ; ") || "none"}</Text>
        <Text testID="host-closed">closed:{closed()}</Text>
        <Text testID="host-links">{(skills().find((record) => record.name === scene())?.agentLinks ?? []).join(",") || "-"}</Text>
        <Text testID="host-choices">{JSON.stringify(descriptionChoices())}</Text>
      </View>
      {/* 定宽轨道容器（index.tsx 的 DETAIL_COLUMN_W 轨道对位）：Sheet 的
          width:100% 需要定宽父级，直接挂 flex 行会把右缘控件推出视口。 */}
      <View style={{ width: 420, height: "100%", display: "flex", flexShrink: 0 }}>
      {/* Props 走调用点读取（组件体只执行一次）：skills/selectedSkill 变化
          时这里的表达式重算，DetailDrawerTrack 的同一派生。 */}
      <DetailDrawerSlot
        skill={hostOpen() ? (skills().find((record) => record.name === scene()) ?? null) : null}
        agents={profiles()}
        busyToken={liveBusy()}
        updatingBatch={new Set()}
        descriptionChoices={descriptionChoices()}
        onClose={() => {
          closed.set(closed() + 1);
          selectedSkill.set(null);
          hostOpen.set(false);
        }}
        ops={OPS}
        session={session}
        onOpenReader={(name) => record(`reader:${name}`)}
        onOpenFolder={(name) => record(`folder:${name}`)}
        onUpdate={(name) => record(`update:${name}`)}
      />
      </View>
      <NoticeBand />
      <DialogHost />
    </View>
  );
}

mount(App);
