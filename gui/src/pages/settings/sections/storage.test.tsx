// 「存储」分区测试 — storage/mod.rs + skill_repair.rs 的语义对位：两步强删
// （Hub 先 preview 名单再 arm、第二击才执行、busy 门）、删除状态行分解
// （kept/removed/failed）、清理缓存与修复的 busy 语义。不写真实 $HOME——
// 「只动可证明拥有条目」的域语义由 native crate 的 cargo 沙箱测试端到端覆
// 盖（facades/settings_storage.rs 的 hub_reset_takes_only_provable_owned_entries），
// 这里在假桥上锁定 UI 状态机。
import { expect, open, test } from "nsh/test";
import { t } from "../../../services/translation";
import { agentsChanged } from "../../../shell/events";
import {
  breakdownByteCounts,
  confirmKeyOf,
  createStorageState,
  directorySpecs,
  hubDeleteConfirm,
  hubDeleteStatus,
  hubRelative,
  totalStorageBytes,
  type BackupEntryDto,
  type ForceDeleteReportDto,
  type StorageBridge,
  type StorageOverviewDto,
} from "./storage";

const HOST = "./storage.test.host.tsx";

// ---- 装置（假桥：受控 Promise + 调用记录） -----------------------------------

interface StorageHarness {
  bridge: StorageBridge;
  state: ReturnType<typeof createStorageState>;
  calls: {
    overview: number;
    previewHub: number;
    forceHub: number;
    forceCaches: number;
    clean: number;
    repairs: number;
  };
  /** 受控 resolver：测试逐个放行。 */
  resolveOverview: (overview: StorageOverviewDto) => void;
  resolvePreview: (names: string[]) => void;
  resolveForceHub: (report: ForceDeleteReportDto) => void;
}

function overviewFixture(): StorageOverviewDto {
  return {
    data_root_path: "/data",
    hub_root_path: "/data/agents",
    is_hub_under_data: true,
    config_bytes: 10,
    config_path: "/data/config",
    skills_data_path: "/data/data/skills",
    hub_bytes: 20,
    hub_path: "/data/agents/data/skills/installed",
    hub_count: 2,
    broken_count: 0,
    local_count: 0,
    local_bytes: 0,
    local_path: "/data/agents/data/skills-local",
    cache_bytes: 0,
    cache_path: "/data/agents/.repos",
    cache_count: 0,
    cache_unused_count: 0,
    cache_unused_bytes: 0,
    health_issue_count: 0,
    health_repairable: 0,
    intake: [],
    history_count: 0,
  };
}

function harness(): StorageHarness {
  const calls = { overview: 0, previewHub: 0, forceHub: 0, forceCaches: 0, clean: 0, repairs: 0 };
  let resolveOverview: (overview: StorageOverviewDto) => void = () => {};
  let resolvePreview: (names: string[]) => void = () => {};
  let resolveForceHub: (report: ForceDeleteReportDto) => void = () => {};
  const bridge: StorageBridge = {
    storageOverview() {
      calls.overview += 1;
      return new Promise<StorageOverviewDto>((resolve) => {
        resolveOverview = resolve;
      });
    },
    async clearAllCaches() {
      calls.clean += 1;
      return { repos_removed: 1, history_cleared: 2 };
    },
    previewHubDelete() {
      calls.previewHub += 1;
      return new Promise<string[]>((resolve) => {
        resolvePreview = resolve;
      });
    },
    forceDeleteHub() {
      calls.forceHub += 1;
      return new Promise<ForceDeleteReportDto>((resolve) => {
        resolveForceHub = resolve;
      });
    },
    async forceDeleteCaches() {
      calls.forceCaches += 1;
      return 3;
    },
    async previewRepair() {
      calls.repairs += 1;
      return { issues: 1, steps: ["adopt x"], untouched: ["keep y"] };
    },
    async applyRepair() {
      calls.repairs += 1;
      return { applied: 1, failed: [] };
    },
    async previewIntake() {
      calls.repairs += 1;
      return { steps: ["adopt x"], reported: [], would_apply: 1 };
    },
    async applyIntake() {
      calls.repairs += 1;
      return { applied: 1, failed: [] };
    },
    openFolder() {
      /* fire-and-forget */
    },
  };
  return {
    bridge,
    state: createStorageState(bridge),
    calls,
    resolveOverview: (overview) => resolveOverview(overview),
    resolvePreview: (names) => resolvePreview(names),
    resolveForceHub: (report) => resolveForceHub(report),
  };
}

/** 排干 Promise 队列（resolve 的 then 链落地）。 */
async function settle(): Promise<void> {
  for (let i = 0; i < 8; i += 1) {
    await Promise.resolve();
  }
}

// ---- 两步强删（arm_delete / preview_hub_delete / storage_action） ------------

test("the hub reset previews the owned names first and deletes only on the second click", async () => {
  const h = harness();
  h.state.start();
  h.resolveOverview(overviewFixture());
  await settle();

  // 第一击：只发 preview，不删。
  h.state.armDelete("hub");
  expect(h.calls.previewHub).toBe(1);
  expect(h.calls.forceHub).toBe(0);
  await settle(); // preview 未 resolve：仍未删除。
  expect(h.calls.forceHub).toBe(0);
  expect(h.state.storageBusy()).toBe("hub");

  h.resolvePreview(["owned-skill"]);
  await settle();
  // 名单进第二击的确认文案（confirmForceDeleteHub 带 names）。
  expect(h.state.storageConfirm()).toBe("hub");
  expect(h.state.storageStatus()).toBe(hubDeleteConfirm(["owned-skill"]));
  expect(h.state.storageBusy()).toBeNull();

  // 第二击：执行。
  h.state.armDelete("hub");
  expect(h.calls.forceHub).toBe(1);
  h.resolveForceHub({ removed: ["owned-skill"], failed: [], kept: ["manual-skill"] });
  await settle();
  expect(h.state.storageConfirm()).toBeNull();
  // kept 上报进状态行（只动可证明拥有条目的 UI 面：留下的要说明）。
  expect(h.state.storageStatus()).toBe(
    hubDeleteStatus({ removed: ["owned-skill"], failed: [], kept: ["manual-skill"] }),
  );
  // hub_delete_status 只报数量（forceDeleteHubKept）；名字只进确认文案。
  expect(h.state.storageStatus()).not.toContain("manual-skill");
  // 删除完成后重读 overview。
  expect(h.calls.overview).toBeGreaterThanOrEqual(2);
});

test("an empty preview arms nothing and says so", async () => {
  const h = harness();
  h.state.start();
  h.resolveOverview(overviewFixture());
  await settle();
  h.state.armDelete("hub");
  h.resolvePreview([]);
  await settle();
  expect(h.state.storageConfirm()).toBeNull();
  expect(h.state.storageStatus()).toBe(t("settings.forceDeleteHubEmpty"));
  expect(h.calls.forceHub).toBe(0);
});

test("the cache delete arms inline; a busy row ignores further clicks", async () => {
  const h = harness();
  h.state.start();
  h.resolveOverview(overviewFixture());
  await settle();

  // cache 第一击：直接 arm（无 preview），第二击执行。
  h.state.armDelete("cache");
  expect(h.state.storageConfirm()).toBe("cache");
  expect(h.state.storageStatus()).toBe(t(confirmKeyOf("cache")));
  h.state.armDelete("cache");
  expect(h.calls.forceCaches).toBe(1);
  await settle();
  expect(h.state.storageStatus()).toBe(
    t("settings.forceDeleteCacheDone").replace("{{count}}", "3"),
  );

  // busy 期间第一击被吞：preview 发出后、resolve 前再点不重复发。
  h.state.armDelete("hub");
  const previews = h.calls.previewHub;
  h.state.armDelete("hub");
  expect(h.calls.previewHub).toBe(previews);
  h.resolvePreview(["x"]);
  await settle();
  h.resolveOverview(overviewFixture());
  await settle();
});

test("the delete status decomposes partial failures and empty results", () => {
  expect(hubDeleteStatus({ removed: ["a"], failed: [], kept: [] })).toBe(
    t("settings.forceDeleteHubDone").replace("{{count}}", "1"),
  );
  expect(hubDeleteStatus({ removed: ["a"], failed: ["b: boom"], kept: [] })).toBe(
    t("settings.forceDeleteHubPartial")
      .replace("{{count}}", "1")
      .replace("{{failed}}", "1"),
  );
  expect(hubDeleteStatus({ removed: [], failed: [], kept: [] })).toBe(
    t("settings.forceDeleteHubEmpty"),
  );
});

// ---- 路径结构（hub_relative） -----------------------------------------------

test("the hub-relative path strips the data prefix case-insensitively", () => {
  expect(hubRelative("/data", "/data/agents")).toBe("agents");
  expect(hubRelative("/Data/", "/DATA/agents/")).toBe("agents");
  expect(hubRelative("/data", "/elsewhere/agents")).toBe(".agents");
});

// ---- 清理与修复（clean_caches / storage_busy_repair） -------------------------

test("clean caches sums repos and history, then reloads", async () => {
  const h = harness();
  h.state.start();
  h.resolveOverview(overviewFixture());
  await settle();
  h.state.cleanCaches();
  // 三个门互相独立（GPUI 同款）：cleaning 只挡 clean_caches 自身与
  // arm_delete，不挡修复动作——这里只断言清理本身的落点。
  await settle();
  expect(h.state.cleaning()).toBe(false);
  expect(h.state.storageStatus()).toBe(
    t("settings.cacheCleanDone").replace("{{count}}", "3"),
  );
  expect(h.calls.overview).toBeGreaterThanOrEqual(2);
});

test("the four repair operations share one busy gate", async () => {
  const h = harness();
  h.state.start();
  h.resolveOverview(overviewFixture());
  await settle();
  h.state.repairSkills();
  h.state.previewSkillRepair();
  h.state.previewAgentIntake();
  h.state.adoptAgentSkills();
  expect(h.calls.repairs).toBe(1); // 首个占门，其余吞掉。
  await settle();
  expect(h.state.storageStatus()).toBe(t("settings.repairDone").replace("{{count}}", "1"));
  h.state.previewSkillRepair();
  await settle();
  expect(h.state.storageStatus()).toContain("adopt x");
});

// ---- host UI（假桥装置；两步强删按钮的渲染） ---------------------------------

test("the storage card renders rows and the two-step hub delete walks its clicks", async () => {
  const app = await open(HOST);
  await app.waitUntil(() => app.shows("id:storage-del-hub"), { timeout: 2000 });
  await app.waitUntil(() => app.texts().some((s) => s.includes("storage:2")));
  // 第一击：preview 名单进确认文案，armed 相挂在 readout 上。
  await app.press("id:storage-del-hub");
  await app.waitUntil(() =>
    app.texts().some((s) => s.includes("confirm:hub") && s.includes("owned-skill")),
  );
  // 第二击：执行完成后 armed 相复位，状态行报告已卸载与保留的数量
  // （hub_delete_status 只报数量，名字只进确认文案）。
  await app.press("id:storage-del-hub");
  const done = hubDeleteStatus({ removed: ["owned-skill"], failed: [], kept: ["manual-skill"] });
  await app.waitUntil(() =>
    app.texts().some((s) => s.includes("confirm:none") && s.includes(done)),
  );
});

test("backups state lists snapshots, restores a backup with agentsChanged, and clears all backups", async () => {
  const h = harness();
  let entries: BackupEntryDto[] = [
    {
      id: "bak-1",
      skillName: "demo-owner",
      called: "demo",
      reason: "Uninstall",
      createdAt: "2026-11-16T01:00:00Z",
      source: "owner/repo",
      sizeBytes: 1024,
      path: "/data/skills/backups/bak-1",
    },
  ];
  const restoredIds: string[] = [];
  let clearCalls = 0;

  h.bridge.listBackups = async () => entries.map((item) => ({ ...item }));
  h.bridge.restoreBackup = async (id) => {
    restoredIds.push(id);
    entries = entries.filter((item) => item.id !== id);
    return "demo-owner";
  };
  h.bridge.clearBackups = async () => {
    clearCalls += 1;
    const count = entries.length;
    entries = [];
    return { removed_count: count, freed_bytes: 2048 };
  };

  h.state.start();
  expect(h.state.backupsAvailable()).toBe(true);
  h.resolveOverview(overviewFixture());
  await settle();
  expect(h.state.backups().length).toBe(1);
  expect(h.state.backups()[0].called).toBe("demo");

  h.state.restoreBackup("bak-1");
  await settle();
  expect(restoredIds).toEqual(["bak-1"]);
  h.resolveOverview(overviewFixture());
  await settle();
  expect(h.state.backups().length).toBe(0);

  entries = [
    {
      id: "bak-2",
      skillName: "other",
      reason: "UpdateOverwrite" as const,
      createdAt: "2026-11-16T01:05:00Z",
      sizeBytes: 2048,
      path: "/data/skills/backups/bak-2",
    },
  ];
  h.state.reloadBackups();
  await settle();
  expect(h.state.backups().length).toBe(1);

  h.state.clearAllBackups();
  await settle();
  expect(clearCalls).toBe(1);
  h.resolveOverview(overviewFixture());
  await settle();
  expect(h.state.backups().length).toBe(0);
});

test("storage totals do not double-count local_bytes included in hub_bytes and history points to skills_data_path", () => {
  const overview: StorageOverviewDto = {
    ...overviewFixture(),
    config_bytes: 10,
    hub_bytes: 100,
    local_bytes: 40,
    cache_bytes: 25,
    skills_data_path: "/data/data/skills",
  };
  expect(totalStorageBytes(overview)).toBe(135);
  expect(breakdownByteCounts(overview)).toEqual({
    hub: 60,
    cache: 25,
    local: 40,
    config: 10,
  });

  const rows = directorySpecs(overview);
  const historyRow = rows.find((row) => row.folderId === "history");
  expect(historyRow?.path).toBe("/data/data/skills");
});

test("forceDeleteHub, repairSkills, and adoptAgentSkills publish agentsChanged on success only", async () => {
  const h = harness();
  h.state.start();
  h.resolveOverview(overviewFixture());
  await settle();

  let events = 0;
  const unsub = agentsChanged.subscribe(() => {
    events += 1;
  });

  h.state.armDelete("hub");
  h.resolvePreview(["owned-skill"]);
  await settle();
  h.state.armDelete("hub");
  h.resolveForceHub({ removed: ["owned-skill"], failed: [], kept: [] });
  await settle();
  h.resolveOverview(overviewFixture());
  await settle();
  expect(events).toBe(1);

  h.state.repairSkills();
  await settle();
  h.resolveOverview(overviewFixture());
  await settle();
  expect(events).toBe(2);

  h.state.adoptAgentSkills();
  await settle();
  h.resolveOverview(overviewFixture());
  await settle();
  expect(events).toBe(3);

  h.bridge.applyRepair = async () => {
    throw new Error("boom");
  };
  h.state.repairSkills();
  await settle();
  h.resolveOverview(overviewFixture());
  await settle();
  expect(events).toBe(3);
  unsub();
});
