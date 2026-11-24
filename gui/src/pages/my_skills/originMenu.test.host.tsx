import { Text, View, effect, mount, signal, untracked, type JSX } from "nsh";
import { Button } from "nsh/base";
import { OriginMenuButton } from "./originMenu";
import {
  busy,
  descriptionChoices,
  favoriteRepos,
  favoritesLocked,
  loading,
  profiles,
  repoFilter,
  setSnapshotLoaderForTest,
  skills,
  sourceFilter,
} from "./state";
import type { SkillRecord } from "./filters";

// 来源/仓库菜单的无头测试装置（agentRail.test.host.tsx 样板）：OriginMenuButton
// 无 props，仓库名单从页面 state 信号派生（repoSources(skills)），装置经
// skills.set 注入夹具。两个装置按钮换场景：12 仓 = 13 行超出 8 行视口（滚动
// 盒出现）/ 3 仓 = 4 行，高度跟内容走（不出现滚动盒）。选中结果与名单规模
// 镜像成文本供 app.texts() 断言。
//
// 装置纪律：state.ts 在导入时挂了一次真实装载（模块级 effect → refresh），
// 夹具必须等这次装载落地（loading→false）再播种，否则被真实快照冲掉
// （detailDrawer.test.host.tsx 同款）。
//
// 装置状态先置 busy hold，快照读取换成回当前名单的假件
// （detailDrawer.test.host.tsx 同款）。装载尾的 maybeCheckUpdates 读的是刚播种
// 的 hub 夹具：全新安装（空库）上真实装载没有上游、节流窗口从未开过，后台
// 检查立即放行，其 finally 的 refresh 把夹具冲回真实快照（干净 $HOME 下两条
// 用例稳定超时，2026-11-24）；开发机上真实检查走网络，冲掉的时刻晚于用例
// 结束，只是碰巧没暴露。
busy.set("host-hold");
setSnapshotLoaderForTest(async () => ({
  skills: untracked(skills),
  agents: untracked(profiles),
  favorites: [],
  favoritesLocked: false,
  descriptionChoices: untracked(descriptionChoices),
}));

function fixture(name: string, source: string): SkillRecord {
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
    source,
  };
}

const repoCount = signal(12);

/** Seeds `count` repos named repo-00 … (each with one skill of its own). */
function seed(count: number): void {
  const list: SkillRecord[] = [];
  for (let index = 0; index < count; index += 1) {
    const id = String(index).padStart(2, "0");
    list.push(fixture(`skill-${id}`, `repo-${id}`));
  }
  skills.set(list);
  sourceFilter.set("all");
  repoFilter.set(null);
  favoriteRepos.set(new Set());
  favoritesLocked.set(false);
}

function App(): JSX.Element {
  effect(() => {
    if (loading()) return;
    seed(repoCount());
  });
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "flex-start",
        gap: 10,
        padding: 12,
        width: 720,
        height: 760,
      }}
    >
      <View style={{ display: "flex", flexDirection: "row", gap: 8 }}>
        <Button testID="host-many-repos" onPress={() => repoCount.set(12)}>
          <Text>12-repos</Text>
        </Button>
        <Button testID="host-few-repos" onPress={() => repoCount.set(3)}>
          <Text>3-repos</Text>
        </Button>
      </View>
      <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 12 }}>
        <OriginMenuButton />
        <Text testID="host-skills">{`skills:${skills().length}`}</Text>
        <Text testID="host-repo-filter">{`repo:${repoFilter() ?? "none"}`}</Text>
      </View>
    </View>
  );
}

mount(App);
