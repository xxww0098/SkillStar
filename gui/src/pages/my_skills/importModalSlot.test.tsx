// 导入对话框测试 — my_skills/import_modal.rs（+ {phases,select,share}
// .rs）的 nsh 对位。期望值全部出自 GPUI 源码行为与 zh-CN 目录文案；渲染
// 断言走无头 texts()/press()/type()。装置：importModalSlot.test.host.tsx
// （假桥注入，不触文件系统不打网）。真实 git 链路（本地目录扫描→安装回
// 环、120ms 折叠泵、Rust 侧取消）由 native crate 的 cargo test 覆盖
//（my_skills_import.rs）；DomainJob 与 epoch 门端到端由 domainJob.test.tsx 覆盖。
import { expect, open, test } from "nsh/test";
import {
  closeImportModal,
  deckNameFromSource,
  gitErrorMessage,
  importModalOpen,
  installBridges,
  openImportModalWithInput,
  phase,
  progressKey,
  selected,
  shareRemoteLabel,
  urlValue,
} from "./importModalState";
import { requestImportSource } from "../../shell/events";

const HOST = "./importModalSlot.test.host.tsx";

async function boot() {
  const app = await open(HOST, { width: 1200, height: 900, reducedMotion: true });
  await app.host.send("harness", { action: "reset" });
  await app.host.send("harness", { action: "open" });
  await app.waitFor("id:import-modal-card");
  return app;
}

// ---- 纯函数（helpers.rs / progress.rs 的映射表对位） ---------------------------

test("progress key maps phase/stage like progress.rs", () => {
  expect(progressKey("preparing", null)).toBe("githubImportModal.gitPreparing");
  expect(progressKey("running", "fetching")).toBe("githubImportModal.gitRunning");
  expect(progressKey("running", "discovering")).toBe("githubImportModal.discovering");
  expect(progressKey("running", "materializing")).toBe("githubImportModal.materializing");
  expect(progressKey("running", "deploying")).toBe("githubImportModal.materializing");
  expect(progressKey("cancelled", null)).toBe("githubImportModal.gitCancelled");
});

test("git errors map to actionable copy, unknown detail passes through", () => {
  expect(gitErrorMessage("network: dns broke")).toBe(
    "连不上 GitHub。请检查 SkillStar 的代理和网络后再试。",
  );
  expect(gitErrorMessage("token_expired: at 2026")).toContain("GitHub 会话已过期");
  expect(gitErrorMessage("cancelled: by test")).toBe("仓库操作已取消。");
  expect(gitErrorMessage("anything else")).toBe("anything else");
});

test("deck names come from the last path segment (helpers.rs)", () => {
  expect(deckNameFromSource("anthropics/skills")).toBe("skills");
  expect(deckNameFromSource("https://github.com/anthropics/skills.git/")).toBe("skills");
  expect(deckNameFromSource("  skills ")).toBe("skills");
  expect(deckNameFromSource("")).toBe("");
});

test("share remote labels accept https/ssh and reject the rest", () => {
  expect(shareRemoteLabel("https://github.com/owner/demo")).toBe("github.com/owner/demo");
  expect(shareRemoteLabel("ssh://git@corp.example.com:2222/team/secret.git")).toBe(
    "corp.example.com/team/secret",
  );
  expect(shareRemoteLabel("/local/path")).toBeNull();
  expect(shareRemoteLabel("file://host/repo")).toBeNull();
});

// ---- 全链路：URL→扫描→勾选→安装→完成 ------------------------------------------

test("url scan pick install done walks the whole chain", async () => {
  const app = await boot();
  // 输入相：提示、可扫描按钮、最近仓库（host 桥返回两条）。
  expect(app.texts()).toContain("输入 GitHub 地址或粘贴分享码（ags-... / agd-...）");
  expect(app.texts()).toContain("最近的仓库");
  expect(app.texts()).toContain("owner/repo");

  await app.type("id:import-url", "owner/repo");
  await app.press("id:import-scan");
  await app.waitFor("id:import-loading");
  // 进度事件驱动 Loading 相文案（latest 值胜出——发 discovering 即切换）。
  await app.host.send("harness", { action: "emit-progress" });
  await app.waitFor("id:import-progress-label");
  expect(app.texts()).toContain("正在扫描仓库里的技能...");

  await app.host.send("harness", { action: "scan-resolve" });
  await app.waitFor("id:import-select");
  // 选择相：源 chip、计数、预选只含可批量未安装的 demo-skill（internal 与
  // 不可装不预选，import_modal.rs:243-248）。
  expect(app.texts()).toContain("demo-skill");
  expect(app.texts()).toContain("内部");
  expect(app.texts()).toContain("元数据问题");
  expect(app.texts().some((text) => text.includes("已选 1"))).toBe(true);

  await app.press("id:import-install");
  await app.waitFor("id:import-loading");
  await app.host.send("harness", { action: "install-resolve" });
  await app.waitFor("id:import-completed");
  expect(app.texts()).toContain("安装完成");
  expect(app.texts()).toContain("1 个技能安装成功");

  await app.press("id:import-done");
  expect(app.texts().some((text) => text.includes("安装完成"))).toBe(false);
});

// ---- 取消与过期（关闭即 cancel、迟到结算不落屏） ------------------------------

test("cancel button settles into the failed phase with the cancelled copy", async () => {
  const app = await boot();
  await app.type("id:import-url", "owner/repo");
  await app.press("id:import-scan");
  await app.waitFor("id:import-loading");
  // Loading 相的取消按钮（仅扫描/安装可取消）。
  await app.press("id:import-cancel");
  await app.host.send("harness", { action: "cancelled" });
  expect(app.texts().some((text) => text.includes("cancelled:true"))).toBe(true);
  // 域层以 cancelled: 结算（真实 Rust 侧 facade.cancel 后的返回）。
  await app.host.send("harness", { action: "scan-reject-cancelled" });
  await app.waitFor("id:import-failed");
  expect(app.texts()).toContain("仓库操作已取消。");
});

test("closing mid-scan cancels the job and a late settle never lands", async () => {
  const app = await boot();
  await app.type("id:import-url", "owner/repo");
  await app.press("id:import-scan");
  await app.waitFor("id:import-loading");
  // 遮罩关闭 = on_close → cancel_active。
  await app.press("id:import-modal-veil");
  await app.host.send("harness", { action: "cancelled" });
  expect(app.texts().some((text) => text.includes("cancelled:true"))).toBe(true);
  // 迟到的成功结算（对话框已关）：不产生任何完成态残留。
  await app.host.send("harness", { action: "scan-resolve" });
  expect(app.texts().some((text) => text.includes("安装完成"))).toBe(false);
  expect(app.texts().some((text) => text.includes("无法从这个仓库导入"))).toBe(false);
  // 重开是全新的输入相（Drop 丢弃实体对位）。
  await app.host.send("harness", { action: "open" });
  await app.waitFor("id:import-url");
  expect(app.texts()).toContain("输入 GitHub 地址或粘贴分享码（ags-... / agd-...）");
});

// ---- ags-/agd- 分享码实时识别与安装 --------------------------------------------

test("share code typing flips the banner and opens the preview", async () => {
  const app = await boot();
  await app.type("id:import-url", "agd-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
  // 实时识别 banner（looks_like_share_code 的 onChange 驱动；解析 deferred
  // 未放行前停留在输入相，banner 可稳定断言）。
  await app.waitFor("id:import-share-detected");
  expect(app.texts()).toContain("已从剪贴板检测到分享码");
  // 放行解析 → 预览相：deck 名、内嵌/私有横幅、已装徽章、安装 N。
  await app.host.send("harness", { action: "parse-resolve" });
  await app.waitFor("id:import-share-preview");
  expect(app.texts()).toContain("Pack");
  expect(app.texts().some((text) => text.includes("已直接从分享码还原到本地"))).toBe(true);
  expect(app.texts().some((text) => text.includes("没有权限时下载会失败"))).toBe(true);
  expect(app.texts().some((text) => text.includes("检测到已有 1 个技能已安装"))).toBe(true);
  // 三成员中 demo-skill 已装 → 安装 2 个技能。
  expect(app.texts()).toContain("安装 2 个技能");

  await app.press("id:import-share-install");
  await app.waitFor("id:import-loading");
  await app.host.send("harness", { action: "share-resolve" });
  await app.waitFor("id:import-completed");
  // 完成态汇总：结果摘要 + 已存在/新装 chips + 未装行（share.rs chips）。
  expect(app.texts()).toContain("共请求 3 个技能：已存在 1 个，新安装 1 个，未安装 1 个。");
  expect(app.texts()).toContain("已存在（1）");
  expect(app.texts()).toContain("新安装（1）");
  expect(app.texts()).toContain("secret-one");
  // agd- 创建了卡组 → groupsChanged 通知卡组页（deck_created 分支）。
  await app.host.send("harness", { action: "open" });
  expect(app.texts().some((text) => text.includes("groups:1"))).toBe(true);
});

// ---- Quick Pack -----------------------------------------------------------------

test("quick pack installs then stages the deck form and creates it", async () => {
  const app = await boot();
  await app.type("id:import-url", "owner/repo");
  await app.press("id:import-scan");
  await app.waitFor("id:import-loading");
  await app.host.send("harness", { action: "scan-resolve" });
  await app.waitFor("id:import-select");

  await app.press("id:import-quick-pack");
  await app.waitFor("id:import-loading");
  await app.host.send("harness", { action: "install-resolve" });
  // Pack 相：种子名 = 来源末段（open_pack 的 deck_name_from_source；受控
  // 输入的 value 不进 texts()，种子映射由纯函数用例覆盖）。
  await app.waitFor("id:import-pack");
  expect(app.texts()).toContain("新建卡组");
  expect(app.texts()).toContain("demo-skill");

  // 域层重名拒绝：表单保留并显示错误（CreateGroupDialog 失败分支）。
  await app.press("id:import-pack-create");
  await app.host.send("harness", { action: "pack-reject" });
  await app.waitFor("id:import-pack-error");
  expect(app.texts()).toContain("deck name already exists");
  // 成功提交：关窗 + groupsChanged。
  await app.press("id:import-pack-create");
  await app.host.send("harness", { action: "pack-resolve" });
  await app.host.send("harness", { action: "open" });
  expect(app.texts().some((text) => text.includes("groups:1"))).toBe(true);
  expect(app.texts().some((text) => text.includes("新建卡组"))).toBe(false);
});

// ---- 失败相与重试 ---------------------------------------------------------------

test("a scan failure lands on the failed phase with the mapped copy and retries", async () => {
  const app = await boot();
  await app.type("id:import-url", "owner/repo");
  await app.press("id:import-scan");
  await app.waitFor("id:import-loading");
  await app.host.send("harness", { action: "scan-reject" });
  await app.waitFor("id:import-failed");
  expect(app.texts()).toContain("无法从这个仓库导入");
  expect(app.texts()).toContain("连不上 GitHub。请检查 SkillStar 的代理和网络后再试。");

  await app.press("id:import-retry");
  await app.waitFor("id:import-url");
  // reset 回输入相（最近仓库的历史行不属于输入值，重扫入口仍在）。
  expect(app.texts()).toContain("最近的仓库");
});

test("openImportModalWithInput and requestImportSource prefill input and respect preselectedSkills", async () => {
  const seenInputs: string[] = [];
  installBridges({
    looksLikeShareCode: () => false,
    parseShareCode: async () => {
      throw new Error("unused");
    },
    installedNames: async () => [],
    repoHistory: async () => [],
    scanRepo: async (input) => {
      seenInputs.push(input);
      return {
        short: "owner/repo",
        repoUrl: "https://github.com/owner/repo",
        plugin: false,
        cacheHit: false,
        preselectedSkills: ["beta"],
        skills: [
          {
            id: "alpha",
            folderPath: "skills/alpha",
            description: "Alpha",
            alreadyInstalled: false,
            installable: true,
            internal: false,
            frontmatterIssues: [],
          },
          {
            id: "beta",
            folderPath: "skills/beta",
            description: "Beta",
            alreadyInstalled: false,
            installable: true,
            internal: false,
            frontmatterIssues: [],
          },
        ],
      };
    },
    installSkills: async () => ({ installed: [], short: "" }),
    installShare: async () => {
      throw new Error("unused");
    },
    adoptFolder: async () => ({ names: [] }),
    importBundle: async () => ({ installed: [], deckCreated: false }),
    installAudits: async () => [],
    pickFolder: async () => null,
    pickBundleFile: async () => null,
  });
  try {
    openImportModalWithInput("npx skills add owner/repo --skill beta", true);
    for (let i = 0; i < 8; i += 1) await Promise.resolve();
    expect(seenInputs[0]).toBe("npx skills add owner/repo --skill beta");
    expect(urlValue()).toBe("npx skills add owner/repo --skill beta");
    expect(phase()).toBe("select");
    expect([...selected()]).toEqual(["beta"]);

    closeImportModal();
    requestImportSource.publish({ input: "owner/repo --skill alpha", autoScan: false });
    for (let i = 0; i < 4; i += 1) await Promise.resolve();
    expect(importModalOpen()).toBe("import");
    expect(urlValue()).toBe("owner/repo --skill alpha");
    expect(seenInputs.length).toBe(1);
  } finally {
    closeImportModal();
    installBridges();
  }
});