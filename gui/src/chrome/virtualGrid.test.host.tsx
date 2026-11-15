import { mount, signal, View, Text, type JSX } from "nsh";
import { CARD_ROW_W } from "../layout";
import { MarketBoard } from "../pages/marketplace/board";
import { PublisherBoardPane } from "../pages/marketplace/publisher/board";
import type { PublisherRepoRow, PublisherSide } from "../pages/marketplace/publisher/readSide";
import type { MarketSkill, SnapshotStatusTag } from "../pages/marketplace/types";
import { SkillCard } from "../pages/my_skills/skillCard";
import type { SkillRecord } from "../pages/my_skills/filters";
import { RepoAvatar } from "./repoAvatar";

type SceneId = "market" | "pub-repos" | "pub-skills" | "skill-card";

const scene = signal<SceneId>("market");
const repoQuery = signal("");
const activeRepo = signal<string | null>(null);
const selectedMarket = signal<MarketSkill | null>(null);

function makeMarketSkill(name: string, index: number): MarketSkill {
  return {
    name,
    description: `Virtual grid test skill #${index}`,
    localizedDescription: null,
    skillType: "hub",
    stars: 100 - index,
    installed: false,
    updateAvailable: false,
    lastUpdated: "2026-01-01T00:00:00Z",
    gitUrl: "",
    category: "",
    author: null,
    rank: index + 1,
    source: "local",
    upstreamRemoved: false,
  };
}

function makeRepoRow(index: number): PublisherRepoRow {
  const tag = String(index).padStart(2, "0");
  return {
    repo: `repo-${tag}`,
    source: `local/repo-${tag}`,
    skillCount: 12,
    installs: 250 - index,
    installsLabel: "",
  };
}

const MARKET_SKILLS: MarketSkill[] = Array.from({ length: 90 }, (_, idx) =>
  makeMarketSkill(`skill-${String(idx).padStart(2, "0")}`, idx),
);

const PUB_REPOS: PublisherRepoRow[] = Array.from({ length: 90 }, (_, idx) => makeRepoRow(idx));

const PUB_SKILLS: MarketSkill[] = Array.from({ length: 90 }, (_, idx) =>
  makeMarketSkill(`pub-skill-${String(idx).padStart(2, "0")}`, idx),
);

const LOCAL_SKILL_CARD: SkillRecord = {
  name: "local-demo",
  description: "Local skill card render smoke fixture",
  skillType: "local",
  stars: 0,
  installed: true,
  updateAvailable: false,
  upstreamChange: null,
  lastUpdated: "2026-01-01T00:00:00Z",
  gitUrl: "",
  agentLinks: [],
  source: "local",
};

const noop = (): void => {};

const fakePublisherSide: PublisherSide = {
  publisherName: signal("local-pub"),
  repos: signal(PUB_REPOS),
  skills: signal(PUB_SKILLS),
  activeRepo,
  query: repoQuery,
  loadingRepos: signal(false),
  loadingSkills: signal(false),
  refreshing: signal(false),
  busy: signal<string | null>(null),
  snapshotStatus: signal<SnapshotStatusTag>("fresh"),
  snapshotError: signal<string | null>(null),
  refreshedFlash: signal(false),
  selected: selectedMarket,
  open: noop,
  onSearchInput: noop,
  openRepo: (source) => activeRepo.set(source),
  goUp: noop,
  syncSnapshot: noop,
  openSkill: (skill) => selectedMarket.set(skill),
  clearSelection: () => selectedMarket.set(null),
  toggleInstalled: noop,
};

function SceneTab(props: { id: string; label: string; onPress: () => void }): JSX.Element {
  return (
    <View
      testID={props.id}
      role="button"
      accessibilityLabel={props.label}
      style={{ paddingHorizontal: 10, paddingVertical: 4, cursor: "pointer" }}
      onPress={props.onPress}
    >
      <Text style={{ fontSize: 12 }}>{props.label}</Text>
    </View>
  );
}

function App(): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        width: "100%",
        height: "100%",
      }}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          height: 36,
          gap: 8,
          paddingHorizontal: 20,
          alignItems: "center",
        }}
      >
        <SceneTab id="vg-scene-market" label="market" onPress={() => scene.set("market")} />
        <SceneTab
          id="vg-scene-pub-repos"
          label="pub-repos"
          onPress={() => {
            activeRepo.set(null);
            scene.set("pub-repos");
          }}
        />
        <SceneTab
          id="vg-scene-pub-skills"
          label="pub-skills"
          onPress={() => {
            activeRepo.set("local/repo-00");
            scene.set("pub-skills");
          }}
        />
        <SceneTab id="vg-scene-skill-card" label="skill-card" onPress={() => scene.set("skill-card")} />
      </View>

      <View
        style={{
          display: "flex",
          flex: 1,
          minHeight: 0,
          width: "100%",
        }}
      >
        <If when={scene() === "market"}>
          <MarketBoard
            loading={false}
            tab="all"
            skills={MARKET_SKILLS}
            publishers={[]}
            query=""
            viewList={false}
            busy={null}
            selectedName={null}
            onOpenSkill={noop}
            onToggleInstall={noop}
            onOpenPublisher={noop}
            onClearSearch={noop}
          />
        </If>
        <If when={scene() === "pub-repos" || scene() === "pub-skills"}>
          <PublisherBoardPane side={fakePublisherSide} />
        </If>
        <If when={scene() === "skill-card"}>
          <View
            testID="vg-skill-card-stage"
            style={{
              display: "flex",
              flexDirection: "row",
              gap: 16,
              paddingHorizontal: 20,
              width: CARD_ROW_W + 40,
              alignItems: "center",
            }}
          >
            <SkillCard skill={() => LOCAL_SKILL_CARD} listMode={false} />
            <RepoAvatar testID="vg-avatar-branch" owner={null} local={false} />
            <RepoAvatar testID="vg-avatar-local" owner={null} local={true} />
          </View>
        </If>
      </View>
    </View>
  );
}

mount(App);
