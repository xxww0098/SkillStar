// 我的技能页测试 — filters.rs / updates.rs 纯语义对位 + 列数硬关系 +
// 页面 UI 行为。UI 断言从真实快照推导期望（本机有技能→逐卡可见；全新
// 安装→空库态），不写真实 $HOME（只读 + 导航/筛选/排序切换）。
import { expect, open, test, type App } from "nsh/test";
import { MySkills } from "skillstar-gui/native";
import {
  CARD_ROW_W,
  DETAIL_COLUMN_W,
  PAGE_PAD,
  PANEL_BORDER_X,
  SHELL_CHROME_W,
  WINDOW_W,
  WINDOW_H,
} from "../../layout";
import { columnsFor } from "./grid";
import {
  compareNames,
  filteredSkills,
  needsAttention,
  orderRepos,
  repoSources,
  sortSkills,
  updatedAtMs,
  type AgentProfileRecord,
  type SkillRecord,
} from "./filters";
import {
  BACKGROUND_CHECK_INTERVAL_MS,
  canArmBatch,
  reportLines,
  reportNeedsAttention,
  reportNotification,
  skillUpdateInFlight,
  shouldBackgroundCheck,
  type UpdateReport,
} from "./updates";
import { skillSource, skillSubtitle } from "./skillCard";

// ---- filters.rs 语义（逐条对位） -------------------------------------------

function skill(name: string, updated: string): SkillRecord {
  return {
    name,
    description: "",
    skillType: "hub",
    stars: 0,
    installed: true,
    updateAvailable: false,
    upstreamChange: null,
    lastUpdated: updated,
    gitUrl: "",
    agentLinks: [],
  };
}

function names(skills: SkillRecord[]): string[] {
  return skills.map((record) => record.name);
}

/**
 * Press `target` until the view `expectation` names shows (or is gone with
 * `state:"gone"`). A press right after its target first mounts can land on
 * a layout that has not committed yet — `waitUntil` sees the view on the
 * frame it mounts while the hit test still reads the previous layout under
 * load — and the press is then lost without an error. The next press, on
 * the committed layout, lands, so probe with a short timeout and press
 * again. Toggle semantics stay convergent: an odd number of presses ends
 * where a single press would have.
 */
async function pressUntil(
  app: App,
  target: string,
  expectation: string,
  state: "shown" | "gone" = "shown"
): Promise<void> {
  for (let i = 0; i < 3; i++) {
    await app.press(target);
    try {
      await app.waitFor(expectation, { timeout: 800, state });
      return;
    } catch {
      // Lost press or a slow frame: the committed layout lands next time.
    }
  }
  // Honest failure (with the screen) if it never took.
  await app.waitFor(expectation, { state });
}

test("smart puts attention above recency (filters.rs)", () => {
  const fresh = skill("fresh", "2026-10-09T10:00:00Z");
  const stale = skill("stale-alert", "2020-01-01T00:00:00Z");
  stale.updateAvailable = true;
  expect(names(sortSkills([fresh, stale], "smart"))).toEqual(["stale-alert", "fresh"]);
});

test("smart counts upstream change as attention (filters.rs)", () => {
  const plain = skill("plain", "2026-10-09T10:00:00Z");
  const renamed = skill("renamed", "");
  renamed.upstreamChange = { kind: "identity_changed", upstreamName: "moved" };
  expect(names(sortSkills([plain, renamed], "smart"))).toEqual(["renamed", "plain"]);
});

test("updated orders by parsed instant, not bytes (filters.rs)", () => {
  // +08:00 形式字节序更大但时刻更早：字节序会排第一，时刻序把正午的排前。
  const byteGreater = skill("byte-greater", "2026-10-09T18:00:00+08:00"); // 10:00 UTC
  const noon = skill("noon", "2026-10-09T12:00:00Z");
  expect(names(sortSkills([byteGreater, noon], "updated"))).toEqual(["noon", "byte-greater"]);
});

test("undated and unparseable sink in time order (filters.rs)", () => {
  const empty = skill("empty", "");
  const garbage = skill("garbage", "yesterday");
  const dated = skill("dated", "2026-01-01T00:00:00Z");
  expect(names(sortSkills([empty, garbage, dated], "updated"))).toEqual([
    "dated",
    "empty",
    "garbage",
  ]);
  // Date.parse 会吃掉的宽松形式在这里必须当作无日期（chrono 拒收）。
  expect(updatedAtMs(skill("d", "2026-01-01"))).toBe(null);
  expect(updatedAtMs(skill("d", "2026-01-01T00:00:00Z"))).not.toBe(null);
});

test("equal keys fall through to name (filters.rs)", () => {
  const beta = skill("beta", "2026-10-09T10:00:00Z");
  const alpha = skill("alpha", "2026-10-09T10:00:00Z");
  const emptyB = skill("empty-b", "");
  const emptyA = skill("empty-a", "");
  expect(names(sortSkills([beta, alpha, emptyB, emptyA], "updated"))).toEqual([
    "alpha",
    "beta",
    "empty-a",
    "empty-b",
  ]);
  expect(names(sortSkills([beta, alpha, emptyB, emptyA], "smart"))).toEqual([
    "alpha",
    "beta",
    "empty-a",
    "empty-b",
  ]);
});

test("name sort ignores time and attention (filters.rs)", () => {
  const zeta = skill("zeta", "2026-10-09T10:00:00Z");
  const alpha = skill("alpha", "1999-01-01T00:00:00Z");
  alpha.updateAvailable = true;
  expect(names(sortSkills([zeta, alpha], "name"))).toEqual(["alpha", "zeta"]);
});

test("name comparison follows code points (Rust String Ord)", () => {
  expect(compareNames("a", "b")).toBeLessThan(0);
  // U+10000 大于 U+FFFD（码点序）；UTF-16 码元序会把代理对排到 U+E000 前。
  expect(compareNames("\uFFFD", "\u{10000}")).toBeLessThan(0);
  expect(compareNames("same", "same")).toBe(0);
});

test("scope predicate matches query fields, source, repo and agent links (filters.rs)", () => {
  const hub = { ...skill("hub-skill", ""), source: "acme/skills", author: "octocat", description: "Build deep modules." };
  const local = { ...skill("local-skill", ""), skillType: "local" as const };
  hub.agentLinks = ["Cursor"];
  const agents: AgentProfileRecord[] = [
    { id: "cursor", displayName: "Cursor", enabled: true, targetable: true },
  ];

  // query 命中 名/描述/来源/作者。
  expect(
    names(
      filteredSkills([hub, local], [], {
        query: "deep",
        sourceFilter: "all",
        repoFilter: null,
        agentFilterId: null,
        onlyUpdates: false,
        sortBy: "name",
      })
    )
  ).toEqual(["hub-skill"]);
  expect(
    names(
      filteredSkills([hub], [], {
        query: "OCTO",
        sourceFilter: "all",
        repoFilter: null,
        agentFilterId: null,
        onlyUpdates: false,
        sortBy: "name",
      })
    )
  ).toEqual(["hub-skill"]);

  // source：Hub 排 Local、Local 排非 Local。
  expect(
    names(
      filteredSkills([hub, local], [], {
        query: "",
        sourceFilter: "hub",
        repoFilter: null,
        agentFilterId: null,
        onlyUpdates: false,
        sortBy: "name",
      })
    )
  ).toEqual(["hub-skill"]);
  expect(
    names(
      filteredSkills([hub, local], [], {
        query: "",
        sourceFilter: "local",
        repoFilter: null,
        agentFilterId: null,
        onlyUpdates: false,
        sortBy: "name",
      })
    )
  ).toEqual(["local-skill"]);

  // repo 精确匹配 source。
  expect(
    names(
      filteredSkills([hub], [], {
        query: "",
        sourceFilter: "all",
        repoFilter: "other/repo",
        agentFilterId: null,
        onlyUpdates: false,
        sortBy: "name",
      })
    )
  ).toEqual([]);

  // agent 以 display_name 匹配 agent_links。
  expect(
    names(
      filteredSkills([hub], agents, {
        query: "",
        sourceFilter: "all",
        repoFilter: null,
        agentFilterId: "cursor",
        onlyUpdates: false,
        sortBy: "name",
      })
    )
  ).toEqual(["hub-skill"]);

  // only_updates 走 needs_attention 谓词。
  const pending = skill("pending", "");
  pending.updateAvailable = true;
  expect(
    names(
      filteredSkills([pending, hub], [], {
        query: "",
        sourceFilter: "all",
        repoFilter: null,
        agentFilterId: null,
        onlyUpdates: true,
        sortBy: "name",
      })
    )
  ).toEqual(["pending"]);
  expect(needsAttention(pending)).toBe(true);
});

test("repo sources dedupe sorted; favorites lead in name order (origin_menu.rs)", () => {
  const a = { ...skill("a", ""), source: "b/other" };
  const b = { ...skill("b", ""), source: "m/skills" };
  const c = { ...skill("c", ""), source: "m/skills" };
  const d = { ...skill("d", ""), source: "a/kept" };
  expect(repoSources([a, b, c, d])).toEqual(["a/kept", "b/other", "m/skills"]);
  expect(orderRepos(["b/other", "m/skills", "a/kept", "c/last"], new Set(["m/skills", "a/kept"]))).toEqual([
    "a/kept",
    "m/skills",
    "b/other",
    "c/last",
  ]);
});

// ---- updates.rs 语义 --------------------------------------------------------

function reportOf(partial: Partial<UpdateReport>): UpdateReport {
  return {
    updated: [],
    failed: [],
    skipped: [],
    channelManaged: [],
    identityChanged: [],
    notUpdatable: [],
    projectsUpdated: [],
    projectFailures: [],
    ...partial,
  };
}

const i18n = {
  t: (key: string) => key,
  tf: (key: string, vars: Record<string, string | number>) =>
    `${key}:${Object.entries(vars)
      .map(([k, v]) => `${k}=${v}`)
      .join(",")}`,
};

test("partial updates name every copy left behind (updates.rs)", () => {
  const report = reportOf({
    updated: [
      {
        name: "alpha",
        agentLinkFailures: ["codex"],
        projectsUpdated: [],
      },
    ],
    failed: [{ name: "beta", error: "network" }],
    projectFailures: ["proj-a: locked"],
    notUpdatable: ["mine"],
  });
  const text = reportLines(report, i18n).join("\n");
  for (const needle of ["beta: network", "codex", "proj-a: locked", "mine"]) {
    expect(text).toContain(needle);
  }
  expect(reportNeedsAttention(report)).toBe(true);
  expect(reportLines(reportOf({}), i18n)).toEqual([]);
});

test("successful project copies do not turn the update into a warning (updates.rs)", () => {
  const report = reportOf({
    updated: [{ name: "alpha", agentLinkFailures: [], projectsUpdated: [] }],
    projectsUpdated: ["project-a", "project-b"],
  });
  expect(reportLines(report, i18n).length).toBe(2);
  expect(reportNeedsAttention(report)).toBe(false);
  const text = reportLines(report, i18n).join("\n");
  expect(text).toContain("project-a");
  expect(text).toContain("project-b");
});

test("the one-toast summary tone follows the report (report_notification)", () => {
  expect(reportNotification(reportOf({}), i18n)).toBe(null);
  const clean = reportNotification(
    reportOf({ updated: [{ name: "a", agentLinkFailures: [], projectsUpdated: [] }] }),
    i18n
  );
  expect(clean?.tone).toBe("success");
  const allFailed = reportNotification(reportOf({ failed: [{ name: "a", error: "x" }] }), i18n);
  expect(allFailed?.tone).toBe("error");
  const mixed = reportNotification(
    reportOf({
      updated: [{ name: "a", agentLinkFailures: [], projectsUpdated: [] }],
      failed: [{ name: "b", error: "x" }],
    }),
    i18n
  );
  expect(mixed?.tone).toBe("warning");
  const pinned = reportNotification(reportOf({ projectFailures: ["p: x"] }), i18n);
  expect(pinned?.pinned).toBe(true);
});

test("the card ellipsis follows the click-time batch snapshot (updates.rs)", () => {
  const pending = skill("alpha", "");
  const other = skill("beta", "");
  const current = skill("gamma", "");
  const batch = new Set(["alpha", "beta"]);

  expect(skillUpdateInFlight(pending, "update_all", batch)).toBe(true);
  expect(skillUpdateInFlight(other, "update_all", batch)).toBe(true);
  expect(skillUpdateInFlight(current, "update_all", batch)).toBe(false);
  // 第二次点击在 busy 时被拒（一次只跑一个变更）。
  expect(canArmBatch(["alpha"], "update_all")).toBe(false);
  expect(canArmBatch([], null)).toBe(false);
  expect(canArmBatch(["alpha"], null)).toBe(true);

  // 清空选择不影响在飞卡片的省略号（busy 换 token 快照）。
  expect(skillUpdateInFlight(pending, "batch_update", batch)).toBe(true);
  expect(skillUpdateInFlight(pending, "alpha", new Set())).toBe(true);
  expect(skillUpdateInFlight(other, "alpha", new Set())).toBe(false);
  expect(skillUpdateInFlight(pending, null, batch)).toBe(false);
});

test("the background check throttles to thirty minutes (updates.rs)", () => {
  const upstream = [{ ...skill("a", ""), skillType: "hub" as const }];
  const now = 1_000_000;
  expect(shouldBackgroundCheck(upstream, null, now, null)).toBe(true);
  expect(shouldBackgroundCheck(upstream, now - 1000, now, null)).toBe(false);
  expect(
    shouldBackgroundCheck(upstream, now - BACKGROUND_CHECK_INTERVAL_MS - 1, now, null)
  ).toBe(true);
  expect(shouldBackgroundCheck(upstream, null, now, "update_all")).toBe(false);
  expect(shouldBackgroundCheck([{ ...skill("l", ""), skillType: "local" as const }], null, now, null)).toBe(false);
});

test("skillSubtitle keeps distinct GitHub repo, falls back to @author, and omits local skills", () => {
  expect(
    skillSubtitle({
      ...skill("ask-matt", ""),
      source: "mattpocock/skills",
      author: "mattpocock",
    })
  ).toBe("mattpocock/skills");

  expect(
    skillSubtitle({
      ...skill("orchestration", ""),
      source: "orchestration",
    })
  ).toBe(null);

  expect(
    skillSubtitle({
      ...skill("orchestration", ""),
      source: "orchestration",
      author: "@octocat",
    })
  ).toBe("@octocat");

  expect(
    skillSubtitle({
      ...skill("local-tool", ""),
      skillType: "local",
      source: "acme/local-repo",
      author: "alice",
    })
  ).toBe(null);
});

test("skillSource links the repo page, falls back to the author page, and never guesses", () => {
  // 已跟踪的 http git URL 优先：去掉克隆后缀与尾斜杠才是浏览器地址。
  expect(
    skillSource({
      ...skill("ask-matt", ""),
      source: "mattpocock/skills",
      gitUrl: "https://github.com/mattpocock/skills.git/",
    })
  ).toEqual({ label: "mattpocock/skills", url: "https://github.com/mattpocock/skills" });

  // SSH 的 git URL 不是浏览器地址：按 owner/repo 来源拼 GitHub。
  expect(
    skillSource({
      ...skill("ask-matt", ""),
      source: "mattpocock/skills",
      gitUrl: "git@github.com:mattpocock/skills.git",
    })
  ).toEqual({ label: "mattpocock/skills", url: "https://github.com/mattpocock/skills" });

  // 来源自身就是地址时直接用它。
  expect(
    skillSource({ ...skill("fork", ""), source: "https://example.com/acme/fork" })
  ).toEqual({ label: "https://example.com/acme/fork", url: "https://example.com/acme/fork" });

  // 作者 handle 兜底到个人页。
  expect(
    skillSource({ ...skill("orchestration", ""), source: "orchestration", author: "@octocat" })
  ).toEqual({ label: "@octocat", url: "https://github.com/octocat" });

  // 既不是 GitHub 路径也不是地址：只排文字，不猜链接。
  expect(
    skillSource({
      ...skill("gitlab-tool", ""),
      source: "gitlab.com/acme/tools",
      gitUrl: "git@gitlab.com:acme/tools.git",
    })
  ).toEqual({ label: "gitlab.com/acme/tools", url: null });
  expect(
    skillSource({ ...skill("spaced", ""), source: "spaced", author: "Foo Bar" })
  ).toEqual({ label: "@Foo Bar", url: null });

  // 本地技能没有副标题，也就没有跳转目标。
  expect(
    skillSource({
      ...skill("local-tool", ""),
      skillType: "local",
      source: "acme/local-repo",
      author: "alice",
    })
  ).toBe(null);
});

// ---- 列数硬关系（layout 不变量：关详情列 3 列 / 开 2 列） -------------------

test("the default pane holds three columns; the detail track leaves two", () => {
  // 默认窗宽的页内容区 = 一整行三卡（layout.ts 已锁 CARD_ROW_W）。
  const contentW = WINDOW_W - SHELL_CHROME_W - PANEL_BORDER_X - PAGE_PAD * 2;
  expect(contentW).toBe(CARD_ROW_W);
  expect(columnsFor(contentW)).toBe(3);
  // 详情列轨道 = 一卡加一 gap，打开即退一列（canvas onLayout 实测宽的对位）。
  expect(columnsFor(contentW - DETAIL_COLUMN_W)).toBe(2);
  // 卡宽固定不拉伸：宽出来的窗口加列，不撑卡。
  expect(columnsFor(CARD_ROW_W + 336 + 16)).toBe(4);
  expect(columnsFor(0)).toBe(1);
});

// ---- 桥数据流（真实域，只读） ------------------------------------------------

test("the snapshot carries skills, agents and favorites", async () => {
  const snapshot = (await MySkills.snapshot()) as {
    skills: unknown[];
    agents: unknown[];
    favorites: string[];
    favoritesLocked: boolean;
  };
  for (const entry of snapshot.skills as Record<string, unknown>[]) {
    expect(typeof entry.name).toBe("string");
  }
  // 内建 roster 恒在（registry specs：built-in 先于 prefs customs）。
  expect(snapshot.agents.length).toBeGreaterThan(0);
  expect(Array.isArray(snapshot.favorites)).toBe(true);
  expect(typeof snapshot.favoritesLocked).toBe("boolean");
});

// ---- 页面 UI -----------------------------------------------------------------

test("the page mounts the toolbar: scope pills, search, sort, view switch", async () => {
  const app = await open("../../../App.tsx", { width: WINDOW_W, height: WINDOW_H });
  // my_skills 是默认页；scope 两态滑块两枚 pill 都在（本地 / 共享频道）。
  await app.waitUntil(() => app.shows("id:my-skills-scope-local"));
  expect(app.shows("id:my-skills-scope-channels")).toBe(true);
  // 第三段已随 SSH 远端退场：滑块只有两段。
  expect(app.shows("id:my-skills-scope-remote")).toBe(false);
  expect(app.shows("id:my-skills-search-input")).toBe(true);
  expect(app.shows("id:my-skills-sort-trigger")).toBe(true);
  expect(app.shows("id:my-skills-view-grid")).toBe(true);
  expect(app.shows("id:my-skills-view-list")).toBe(true);
  expect(app.shows("id:my-skills-refresh")).toBe(true);
  expect(app.shows("id:my-skills-import")).toBe(true);
});

test("the grid mirrors the real snapshot, card for card", async () => {
  const snapshot = (await MySkills.snapshot()) as {
    skills: { name: string; skillType?: string; source?: string; author?: string }[];
  };
  const app = await open("../../../App.tsx", { width: WINDOW_W, height: WINDOW_H });
  await app.waitUntil(() => app.shows("id:my-skills-sort-trigger"));
  if (snapshot.skills.length === 0) {
    // 全新安装：空库态 + 去市场 CTA（四套空状态之一）。
    await app.waitUntil(() => app.shows("id:my-skills-goto-marketplace"));
    return;
  }
  // 列表装载完成后第一张卡可见（虚拟化：只断言首卡与首行）。
  const first = snapshot.skills[0];
  await app.waitUntil(() => app.shows(`id:skill-card-${first.name}`));
  expect(app.shows("id:skills-scroll")).toBe(true);
  if (
    first.skillType !== "local" &&
    ((first.source?.trim() && first.source.trim() !== first.name.trim()) ||
      Boolean(first.author?.trim()))
  ) {
    expect(app.shows(`id:skill-card-source-${first.name}`)).toBe(true);
  }
});

test("searching narrows the grid to the query predicate", async () => {
  const snapshot = (await MySkills.snapshot()) as { skills: { name: string }[] };
  if (snapshot.skills.length === 0) return;
  const app = await open("../../../App.tsx", { width: WINDOW_W, height: WINDOW_H });
  const first = snapshot.skills[0].name;
  await app.waitUntil(() => app.shows(`id:skill-card-${first}`));
  await app.type("id:my-skills-search-input", "zz-no-such-skill-zz");
  await app.waitUntil(() => app.shows("id:my-skills-clear-search"));
  await app.clear("id:my-skills-search-input");
  await app.waitUntil(() => app.shows(`id:skill-card-${first}`));
});

test("the sort menu lists three orders and picking name re-orders the grid", async () => {
  const snapshot = (await MySkills.snapshot()) as { skills: { name: string }[] };
  if (snapshot.skills.length < 2) return;
  const app = await open("../../../App.tsx", { width: WINDOW_W, height: WINDOW_H });
  await app.waitUntil(() => app.shows("id:my-skills-sort-trigger"));
  await app.press("id:my-skills-sort-trigger");
  await app.waitUntil(() => app.shows("id:sort-row-name"));
  expect(app.shows("id:sort-row-smart")).toBe(true);
  expect(app.shows("id:sort-row-updated")).toBe(true);
  await app.press("id:sort-row-name");
  await app.waitUntil(() => !app.shows("id:sort-row-name"));
  // 触发器换标当前排序（默认「智能」→「名称」）。
  await app.waitUntil(() =>
    app.texts().some((text) => text === "名称" || text === "Name")
  );
  // 名称排序后第一张可见卡是字典序最小的技能名。
  const sorted = snapshot.skills.map((s) => s.name).sort();
  await app.waitUntil(() => app.shows(`id:skill-card-${sorted[0]}`));
});

test("the two-state scope switch moves the page between local and shared channels", async () => {
  const app = await open("../../../App.tsx", { width: WINDOW_W, height: WINDOW_H });
  await app.waitUntil(() => app.shows("id:my-skills-scope-local"));
  // 本地 scope：本地工具栏控件（导入/排序/搜索）在。
  expect(app.shows("id:my-skills-import")).toBe(true);
  expect(app.shows("id:my-skills-sort-trigger")).toBe(true);
  await app.press("id:my-skills-scope-channels");
  // 频道 scope：频道空状态（emptyState.channelsTitle），本地专属动作退场
  // （render_channels_toolbar 对位：只留标题 + 滑块 + 搜索）。
  await app.waitUntil(() => app.texts().some((text) => text === "共享频道" || text === "Shared channels"));
  await app.waitUntil(() => !app.shows("id:my-skills-import"));
  expect(app.shows("id:my-skills-sort-trigger")).toBe(false);
  // 搜索是两态共用（收敛后本地与频道同一套工具栏头部）。
  expect(app.shows("id:my-skills-search-input")).toBe(true);
  // 回到本地 scope，工具栏恢复。
  await app.press("id:my-skills-scope-local");
  await app.waitUntil(() => app.shows("id:my-skills-sort-trigger"));
});

test("the scope switch still lands at once under reduced motion", async () => {
  // 胶囊的滑行与拉伸都走 transform 过渡，降低动态效果时两者一起清空
  // （chrome/motion.ts）：点击仍然直接落到目标槽位，页面照常切换。
  const app = await open("../../../App.tsx", {
    width: WINDOW_W,
    height: WINDOW_H,
    reducedMotion: true,
  });
  await app.waitUntil(() => app.shows("id:my-skills-scope-local"));
  await app.press("id:my-skills-scope-channels");
  await app.waitUntil(() =>
    app.texts().some((text) => text === "共享频道" || text === "Shared channels")
  );
  await app.press("id:my-skills-scope-local");
  await app.waitUntil(() => app.shows("id:my-skills-sort-trigger"));
});

test("pressing a card opens the detail track; pressing it again closes it", async () => {
  const snapshot = (await MySkills.snapshot()) as { skills: { name: string }[] };
  if (snapshot.skills.length === 0) return;
  const app = await open("../../../App.tsx", { width: WINDOW_W, height: WINDOW_H });
  const first = snapshot.skills[0].name;
  await app.waitUntil(() => app.shows(`id:skill-card-${first}`));
  // 详情列轨道同帧出现（宽度从 0 到 DETAIL_COLUMN_W，一次重绘；抽屉本体
  // 是并行切片的预注册槽位）。卡片刚挂载的首击可能落在未提交的布局上
  //（pressUntil 的重试吸收这个竞态）。
  await pressUntil(app, `id:skill-card-${first}`, "id:my-skills-detail-track");
  await pressUntil(app, `id:skill-card-${first}`, "id:my-skills-detail-track", "gone");
});

test("the view switch keeps the first card visible in both modes", async () => {
  const snapshot = (await MySkills.snapshot()) as { skills: { name: string }[] };
  if (snapshot.skills.length === 0) return;
  const app = await open("../../../App.tsx", { width: WINDOW_W, height: WINDOW_H });
  const first = snapshot.skills[0].name;
  await app.waitUntil(() => app.shows(`id:skill-card-${first}`));
  await app.press("id:my-skills-view-list");
  await app.waitUntil(() => app.shows(`id:skill-card-${first}`));
  await app.press("id:my-skills-view-grid");
  await app.waitUntil(() => app.shows(`id:skill-card-${first}`));
});

test("the origin menu filters by source and dismisses on pick", async () => {
  const snapshot = (await MySkills.snapshot()) as {
    skills: { name: string; skillType: string }[];
  };
  if (snapshot.skills.length === 0) return;
  const app = await open("../../../App.tsx", { width: WINDOW_W, height: WINDOW_H });
  await app.waitUntil(() => app.shows("id:my-skills-origin-trigger"));
  // 触发器随首装数据落地挂载：首击可能丢在未提交的布局上（pressUntil）。
  await pressUntil(app, "id:my-skills-origin-trigger", "id:source-filter");
  expect(app.shows("id:source-filter-all")).toBe(true);
  expect(app.shows("id:source-filter-hub")).toBe(true);
  expect(app.shows("id:source-filter-local")).toBe(true);
  // 选 Hub：本地创建的技能退场（skill_in_scope 的 source 半）。弹层行
  // 同样是刚挂载的目标，press 的丢失用重试吸收。
  await pressUntil(app, "id:source-filter-hub", "id:source-filter", "gone");
  const hubNames = snapshot.skills.filter((s) => s.skillType === "hub").map((s) => s.name);
  const localNames = snapshot.skills.filter((s) => s.skillType === "local").map((s) => s.name);
  if (localNames.length > 0) {
    await app.waitUntil(() => !app.shows(`id:skill-card-${localNames[0]}`));
  }
  if (hubNames.length > 0) {
    await app.waitUntil(() => app.shows(`id:skill-card-${hubNames[0]}`));
  } else {
    await app.waitUntil(() => app.shows("id:my-skills-clear-search"));
  }
  // 回到 All：面板重开，选 All 后名单恢复。
  await pressUntil(app, "id:my-skills-origin-trigger", "id:source-filter");
  await pressUntil(app, "id:source-filter-all", "id:source-filter", "gone");
  await app.waitUntil(() => app.shows(`id:skill-card-${snapshot.skills[0].name}`));
});
