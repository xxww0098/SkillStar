import { Text, View, mount, signal } from "nsh";
import { Button } from "nsh/base";
import { NoticeBand } from "../../shell/notify";
import { DialogHost } from "../../chrome/dialog";
import { groupsChanged } from "../../shell/events";
import { DeckDetailDrawer } from "./detailDrawer";
import type { DeckDrawerOps, DrawerDeck, LinkReport } from "./detailDrawerActions";

// detailDrawer 的无头测试装置（createGroup.test.host.tsx 的样板：测试
// realm 与 app bundle 隔离，数据经假 ops 注入，装置按钮切场景，调用与
// 事件计数渲染成文本供 app.texts() 断言）。真实域写入由 native crate 的
// cargo test 用隔离数据目录覆盖；这里验证的是抽屉的数据流与交互编排。

type Scene = "rich" | "fifty" | "empty" | "noagents" | "mixed";

/** 2 成员（1 已安装 1 缺装）、2 个可链接 agent、claude 已上链。 */
const RICH: DrawerDeck = {
  id: "g1",
  name: "演示卡组",
  description: "两组技能",
  icon: "🚀",
  skills: ["alpha", "beta"],
  agentLinks: ["claude"],
};

/** 50 成员：超出任何视口，走 List 虚拟化分支（member-42 视口外）。 */
const FIFTY: DrawerDeck = {
  id: "g2",
  name: "长卡组",
  description: "",
  icon: "",
  skills: Array.from({ length: 50 }, (_, ix) => `member-${String(ix).padStart(2, "0")}`),
  agentLinks: null,
};

const EMPTY: DrawerDeck = {
  id: "g3",
  name: "空卡组",
  description: "",
  icon: "⭐",
  skills: [],
  agentLinks: [],
};

const MIXED: DrawerDeck = {
  id: "g4",
  name: "漂移卡组",
  description: "",
  icon: "⚡",
  skills: ["alpha", "beta"],
  agentLinks: ["claude"],
  memberAgentLinks: {
    alpha: ["Claude Code"],
    beta: [],
  },
};

const INSTALLED = new Set(["alpha"]);

const scene = signal<Scene>("rich");
const pageBusy = signal(false);
const calls = signal<string[]>([]);
const closed = signal(0);
const edited = signal(0);
const shared = signal(0);
const groupEvents = signal(0);
/** 让下一次 deploy-all 挂起，直到 release（busy 语义测试的闸门）。 */
const holdNext = signal(false);
let releaseGate: (() => void) | null = null;

groupsChanged.subscribe(() => groupEvents.set(groupEvents() + 1));

const EMPTY_REPORT: LinkReport = { applied: [], skipped: [], failed: [] };
/** deploy-all 的带跳过结果：NoticeBand 必须出现 partial 文案。 */
const SKIPPED_REPORT: LinkReport = {
  applied: [["claude", "alpha"]],
  skipped: [{ agentId: "codex", skill: "alpha", reason: "name occupied" }],
  failed: [],
};

const OPS: DeckDrawerOps = {
  setDeckAgent: (deckId, agentId, enable) => {
    calls.set([...calls(), `set:${deckId}:${agentId}:${enable}`]);
    return Promise.resolve(EMPTY_REPORT);
  },
  linkDeckToEnabledAgents: (deckId) => {
    calls.set([...calls(), `all:${deckId}`]);
    if (holdNext()) {
      holdNext.set(false);
      return new Promise<LinkReport>((resolve) => {
        releaseGate = () => resolve(SKIPPED_REPORT);
      });
    }
    return Promise.resolve(SKIPPED_REPORT);
  },
  duplicateDeck: (deckId) => {
    calls.set([...calls(), `dup:${deckId}`]);
    return Promise.resolve("copy-of-" + deckId);
  },
  deleteDeck: (deckId) => {
    calls.set([...calls(), `del:${deckId}`]);
    return Promise.resolve();
  },
};

function deckFor(scene: Scene): DrawerDeck {
  switch (scene) {
    case "fifty":
      return FIFTY;
    case "empty":
      return EMPTY;
    case "mixed":
      return MIXED;
    default:
      return RICH;
  }
}


function App() {
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
      <View style={{ display: "flex", flexDirection: "column", gap: 4, width: 160 }}>
        <Button testID="host-scene-rich" onPress={() => scene.set("rich")}>
          <Text>scene-rich</Text>
        </Button>
        <Button testID="host-scene-fifty" onPress={() => scene.set("fifty")}>
          <Text>scene-fifty</Text>
        </Button>
        <Button testID="host-scene-empty" onPress={() => scene.set("empty")}>
          <Text>scene-empty</Text>
        </Button>
        <Button testID="host-scene-noagents" onPress={() => scene.set("noagents")}>
          <Text>scene-noagents</Text>
        </Button>
        <Button testID="host-scene-mixed" onPress={() => scene.set("mixed")}>
          <Text>scene-mixed</Text>
        </Button>
        <Button testID="host-page-busy" onPress={() => pageBusy.set(!pageBusy())}>
          <Text>page-busy</Text>
        </Button>
        <Button testID="host-hold-next" onPress={() => holdNext.set(true)}>
          <Text>hold-next</Text>
        </Button>
        <Button
          testID="host-release"
          onPress={() => {
            const gate = releaseGate;
            releaseGate = null;
            gate?.();
          }}
        >
          <Text>release</Text>
        </Button>
        <Button testID="host-reset-calls" onPress={() => calls.set([])}>
          <Text>reset-calls</Text>
        </Button>
        <Text testID="host-calls">{calls().join(" ; ") || "none"}</Text>
        <Text testID="host-closed">closed:{closed()}</Text>
        <Text testID="host-edited">edited:{edited()}</Text>
        <Text testID="host-shared">shared:{shared()}</Text>
        <Text testID="host-group-events">groups:{groupEvents()}</Text>
      </View>
      {/* Props stay call-site reads (components run once): scene()/pageBusy()
          inside the JSX re-read on every change, the way Shell's page switch
          reads currentPage() in its style. */}
      <DeckDetailDrawer
        deck={deckFor(scene())}
        installedSkills={INSTALLED}
        agents={
          scene() === "noagents"
            ? []
            : [
                { id: "claude", displayName: "Claude Code" },
                { id: "codex", displayName: "Code" },
              ]
        }
        busy={pageBusy()}
        onClose={() => closed.set(closed() + 1)}
        onEdit={() => edited.set(edited() + 1)}
        onShare={() => shared.set(shared() + 1)}
        ops={OPS}
      />
      <NoticeBand />
      <DialogHost />
    </View>
  );
}

mount(App);
