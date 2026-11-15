import { Text, View, mount, signal } from "nsh";
import { Button } from "nsh/base";
import { groupsChanged } from "../../shell/events";
import {
  CreateGroupDialogHost,
  openCreateGroup,
  openEditGroup,
  type CreateGroupBridge,
} from "./createGroup";
import type { DeckDraft, DeckSkillRow } from "./createGroup.logic";

// createGroup 的无头测试装置（App.test.tsx 的 open("./…") 入口形态）。
// 测试 realm 与 app bundle 隔离、set signal 改不到 app 状态，所以数据
// 经由这里的假桥注入：装置按钮切场景、打开对话框，提交记录渲染成文本
// 供 app.texts() 断言（样板 noticeBoard 的「纯逻辑直测 + UI 走 press/texts」）。
// 真实域写入由 native crate 的 cargo test 用隔离数据目录覆盖。

/** 12 行候选：超过 PICKER_VISIBLE(5)，走固定 160px 视口的虚拟化分支。 */
const MANY: DeckSkillRow[] = Array.from({ length: 12 }, (_, ix) => ({
  name: `skill-number-${String(ix).padStart(2, "0")}`,
  description: ix === 7 ? "doc tools" : "",
}));

/** 3 行候选：自然高度分支（无滚动盒）。 */
const FEW: DeckSkillRow[] = [
  { name: "alpha", description: "first" },
  { name: "beta", description: "" },
  { name: "gamma", description: "" },
];

/** 已存在的卡组名（重名 guard 的输入）。 */
const EXISTING = ["taken"];

/** 编辑态的卡组：skill-number-00 已安装，missing 是 orphan 成员。 */
const EDIT_DRAFT: DeckDraft = {
  id: "g1",
  name: "demo",
  description: "old description",
  icon: "🚀",
  skills: ["skill-number-00", "missing"],
};

type Mode = "many" | "few" | "empty";

const mode = signal<Mode>("many");
const failNextCommit = signal(false);
const holdCommit = signal(false);
let releaseHeld: (() => void) | null = null;
const commits = signal<string[]>([]);
const createdCalls = signal(0);
const groupEvents = signal(0);

// 页面直开形态成功后要发的刷新频道（卡组页刷新挂它）。
groupsChanged.subscribe(() => groupEvents.set(groupEvents() + 1));

const bridge: CreateGroupBridge = {
  installedSkillRows: () =>
    Promise.resolve(mode() === "many" ? MANY : mode() === "few" ? FEW : []),
  existingDeckNames: () => Promise.resolve(mode() === "few" ? [] : EXISTING),
  commitDeck: (editingId, name, description, icon, members) => {
    if (failNextCommit()) {
      failNextCommit.set(false);
      return Promise.reject(new Error("A group with the name 'demo' already exists"));
    }
    commits.set([
      ...commits(),
      [editingId ?? "new", name, description, icon, members.join(",")].join("|"),
    ]);
    if (holdCommit()) {
      // 挂起提交，测试 busy 期间的遮罩拦截；release 后 resolve。
      return new Promise<void>((resolve) => {
        releaseHeld = resolve;
      });
    }
    return Promise.resolve();
  },
};

function openWith(next: Mode, options: { seed?: string; initialMembers?: string[] } = {}) {
  mode.set(next);
  openCreateGroup({ ...options, onCreated: () => createdCalls.set(createdCalls() + 1), bridge });
}

function App() {
  return (
    <View
      style={{
        width: "100%",
        height: "100%",
        display: "flex",
        flexDirection: "column",
        padding: 8,
        gap: 4,
      }}
    >
      <Button testID="host-open-new" onPress={() => openWith("many")}>
        <Text>open-new</Text>
      </Button>
      <Button
        testID="host-open-seeded"
        onPress={() => openWith("many", { seed: "orca", initialMembers: ["skill-number-00"] })}
      >
        <Text>open-seeded</Text>
      </Button>
      <Button testID="host-open-few" onPress={() => openWith("few")}>
        <Text>open-few</Text>
      </Button>
      <Button testID="host-open-empty" onPress={() => openWith("empty")}>
        <Text>open-empty</Text>
      </Button>
      <Button
        testID="host-open-edit"
        onPress={() => {
          mode.set("many");
          openEditGroup(EDIT_DRAFT, {
            onSaved: () => createdCalls.set(createdCalls() + 1),
            bridge,
          });
        }}
      >
        <Text>open-edit</Text>
      </Button>
      <Button testID="host-fail-next" onPress={() => failNextCommit.set(true)}>
        <Text>fail-next</Text>
      </Button>
      <Button
        testID="host-hold-commit"
        onPress={() => {
          holdCommit.set(true);
        }}
      >
        <Text>hold-commit</Text>
      </Button>
      <Button
        testID="host-release-commit"
        onPress={() => {
          holdCommit.set(false);
          releaseHeld?.();
          releaseHeld = null;
        }}
      >
        <Text>release-commit</Text>
      </Button>
      <Text testID="host-commits">{commits().join(" ; ")}</Text>
      <Text testID="host-created">created:{createdCalls()}</Text>
      <Text testID="host-group-events">groups:{groupEvents()}</Text>
      <CreateGroupDialogHost />
    </View>
  );
}

mount(App);
