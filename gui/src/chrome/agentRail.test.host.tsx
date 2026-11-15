import { Text, View, mount, signal, type JSX } from "nsh";
import { Button } from "nsh/base";
import { AgentFooterBar, AgentRail, type AgentRailSlot } from "./agentRail";

const ALL_PROFILES = [
  { id: "antigravity", displayName: "Antigravity" },
  { id: "claude", displayName: "Claude Code" },
  { id: "codex", displayName: "Codex" },
  { id: "cursor", displayName: "Cursor" },
  { id: "trae", displayName: "Trae" },
  { id: "zed", displayName: "Zed" },
  { id: "windsurf", displayName: "Windsurf" },
  { id: "opencode", displayName: "OpenCode" },
  { id: "cline", displayName: "Cline" },
];

const slotCount = signal(7);
const linkedIds = signal<ReadonlySet<string>>(new Set(["claude"]));
const parentClicks = signal(0);
const slotClicks = signal<string[]>([]);

function slotsFor(count: number, linked: ReadonlySet<string>): AgentRailSlot[] {
  return ALL_PROFILES.slice(0, count).map((profile) => ({
    profile,
    linked: linked.has(profile.id),
    pending: false,
    label: profile.displayName,
  }));
}

function App(): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 12,
        padding: 16,
        width: 400,
        height: 300,
      }}
    >
      <View style={{ display: "flex", flexDirection: "row", gap: 8 }}>
        <Button testID="host-set-7" onPress={() => slotCount.set(7)}>
          <Text>7-slots</Text>
        </Button>
        <Button testID="host-set-9" onPress={() => slotCount.set(9)}>
          <Text>9-slots</Text>
        </Button>
      </View>

      <View
        testID="parent-card"
        onPress={() => parentClicks.set(parentClicks() + 1)}
        style={{
          display: "flex",
          flexDirection: "column",
          justifyContent: "space-between",
          width: 336,
          height: 160,
          borderRadius: 16,
          borderWidth: 1,
          borderColor: "#485a7a",
        }}
      >
        <View testID="card-body-target" style={{ padding: 14 }}>
          <Text>demo-card</Text>
        </View>
        <AgentFooterBar>
          <AgentRail
            scopeKey="card-1"
            railPrefix="skill-rail"
            slotPrefix="skill-slot"
            slots={slotsFor(slotCount(), linkedIds())}
            pending={false}
            onSlotClick={(slot) => {
              const next = new Set(linkedIds());
              if (next.has(slot.profile.id)) {
                next.delete(slot.profile.id);
              } else {
                next.add(slot.profile.id);
              }
              linkedIds.set(next);
              slotClicks.set([...slotClicks(), slot.profile.id]);
            }}
          />
        </AgentFooterBar>
      </View>

      <Text testID="parent-clicks">{`parent:${parentClicks()}`}</Text>
      <Text testID="slot-clicks">{`slots:${slotClicks().join(",") || "none"}`}</Text>
      <Text testID="linked-ids">{`linked:${[...linkedIds()].sort().join(",") || "none"}`}</Text>
    </View>
  );
}

mount(App);
