import { Text, View, mount, signal, type JSX } from "nsh";
import { beginSave } from "../../chrome/saveStatus";
import { DeckCard, type DeckCardHandle } from "./deckCard";
import type { DeckRecord } from "./deckQuery";

// 卡组卡的无头装置：卡面不放保存状态行（与技能卡同一约定）。夹具把卡会
// 读到的那个保存 id 置成进行中——卡身不该因此多出「正在保存」，翻链成功
// 后也不该多出「已保存」。底栏开关照旧派发卡组命令。

const AGENT_ID = "claude";
const AGENT_NAME = "Claude Code";

const DECK: DeckRecord = {
  id: "deck-fixture",
  name: "BeautifulRust",
  description: "",
  icon: "💻",
  skills: ["rust-best-practices", "rust-port", "rust-review"],
  skillSources: {},
  defaultAgent: "",
  agentLinks: [],
  createdAt: "2026-01-01T00:00:00Z",
  updatedAt: "2026-01-01T00:00:00Z",
};

const lastToggle = signal("none");

const handle: DeckCardHandle = {
  onOpen: () => {},
  onShare: () => {},
  onDuplicate: () => {},
  onDelete: () => {},
  onToggleAgent: (deck, agentId) => lastToggle.set(`${deck.id}:${agentId}`),
};

beginSave(`deck:${DECK.id}`);

function App(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 12, padding: 16 }}>
      <View style={{ display: "flex", flexDirection: "row" }}>
        <DeckCard
          deck={DECK}
          selected={false}
          viewList={false}
          rowBusy={false}
          linkPending={{}}
          installedSkills={new Set(DECK.skills)}
          agents={[{ id: AGENT_ID, displayName: AGENT_NAME, icon: "" }]}
          handle={handle}
        />
      </View>
      <Text testID="deck-toggle">{lastToggle()}</Text>
    </View>
  );
}

mount(App);
