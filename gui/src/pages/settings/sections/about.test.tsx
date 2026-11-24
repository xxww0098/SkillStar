// 「关于」分区测试 — about.rs 的语义对位：环境快照解析（GitStatus 的
// installed/not-installed 两态、release_check 的四种 outcome）、手动检查的
// 单飞门与记录落地、复制钮的 copied 态。不写真实 $HOME——真域路径
// （check_git_status / last_record）由 native crate 的 cargo 测试覆盖
// （facades/settings_storage.rs 的 about_snapshot 测试）。
import { expect, open, test } from "nsh/test";
import { t } from "../../../services/translation";
import {
  createAboutState,
  displayHome,
  platformKey,
  platformLabel,
  type AboutBridge,
  type ReleaseRecordDto,
} from "./about";

function harness(checkResult: Promise<ReleaseRecordDto> | Error = Promise.resolve({
  last_checked_unix: 1,
  current_version: "0.0.1",
  outcome: { kind: "up_to_date" as const },
})): { bridge: AboutBridge; state: ReturnType<typeof createAboutState> } {
  const bridge: AboutBridge = {
    async aboutSnapshot() {
      return {
        version: "1.2.3",
        data_root: "/data",
        gh_installed: true,
        gh_install_command: "brew install gh",
        git_status: { status: "Installed", version: "2.44" },
        release_check: null,
      };
    },
    runReleaseCheck() {
      if (checkResult instanceof Error) return Promise.reject(checkResult);
      return checkResult;
    },
    openFolder() {
      /* fire-and-forget */
    },
  };
  return { bridge, state: createAboutState(bridge) };
}

test("start() seeds the snapshot and the persisted release record", async () => {
  const { state } = harness();
  await state.start();
  expect(state.snapshot()?.version).toBe("1.2.3");
  expect(state.snapshot()?.git_status).toEqual({ status: "Installed", version: "2.44" });
  // 沙箱里没有上一轮记录：chip 相是 null（首次检查前不显示）。
  expect(state.releaseCheck()).toBeNull();
  expect(state.releaseChecking()).toBe(false);
});

test("the manual check is single-flight and records the fresh outcome", async () => {
  const gate: { release: null | (() => void) } = { release: null };
  const record: ReleaseRecordDto = {
    last_checked_unix: 5,
    current_version: "1.2.3",
    outcome: { kind: "available", tag: "v1.3.0", url: "https://x" },
  };
  const pending = new Promise<ReleaseRecordDto>((resolve) => {
    gate.release = () => resolve(record);
  });
  const h = harness(pending);
  await h.state.start();
  h.state.runReleaseCheck();
  expect(h.state.releaseChecking()).toBe(true);
  // 在途再点：吞掉（about.rs run_release_check 的 return 分支）。
  h.state.runReleaseCheck();
  gate.release?.();
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
  expect(h.state.releaseChecking()).toBe(false);
  expect(h.state.releaseCheck()?.outcome).toEqual({
    kind: "available",
    tag: "v1.3.0",
    url: "https://x",
  });
});

test("a bridge failure only resets the button", async () => {
  const h = harness(new Error("offline"));
  h.state.runReleaseCheck();
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
  expect(h.state.releaseChecking()).toBe(false);
  expect(h.state.releaseCheck()).toBeNull();
});

// ---- 纯映射（about.rs display_home / gh_install_command 的平台分支） ---------

test("displayHome folds the home prefix into ~ and passes the rest through", () => {
  expect(displayHome("/Users/me/.skillstar", "/Users/me")).toBe("~/.skillstar");
  expect(displayHome("/Users/me", "/Users/me")).toBe("~");
  // GPUI 的 strip_prefix 是裸字符串前缀（非路径段边界）：/Users/mex/data
  // 同样命中 /Users/me 前缀，得 ~x/data——对位保留这一行为。
  expect(displayHome("/Users/mex/data", "/Users/me")).toBe("~x/data");
  expect(displayHome("/opt/data", "/Users/me")).toBe("/opt/data");
  expect(displayHome("/opt/data", "")).toBe("/opt/data");
});

test("platform keys and labels follow the GPUI cfg ladder", () => {
  expect(platformKey("macos")).toBe("macos");
  expect(platformKey("windows")).toBe("windows");
  expect(platformKey("linux")).toBe("linux");
  expect(platformKey("ios")).toBe("unknown");
  expect(platformLabel("macos")).toBe("macOS");
  expect(platformLabel("windows")).toBe("Windows");
  // GPUI 的 else 臂：其余平台一律 Linux。
  expect(platformLabel("linux")).toBe("Linux");
  expect(platformLabel("android")).toBe("Linux");
});

// ---- host UI（装置交互；假桥脚本） ---------------------------------------------

const HOST = "./about.test.host.tsx";

test("the about card paints tool badges, version chip and the update chip", async () => {
  const app = await open(HOST);
  await app.press("id:device-reset");
  await app.waitUntil(
    () =>
      app.texts().some((s) => s.includes("v1.2.3")) &&
      app.texts().includes(t("settings.gitInstalled")),
  );
  // Git/GitHub CLI 双装：两枚 installed 徽章。
  expect(app.texts()).toContain(t("settings.gitInstalled"));
  expect(app.texts()).toContain(t("settings.ghInstalled"));
  // 上一轮 available 记录：chip 带 tag 文案。
  const found = t("settings.updateFoundDesc").replace("{{version}}", "v1.3.0");
  expect(app.texts()).toContain(found);
  // data root 原样显示（不在 $HOME 下不折算 ~）。
  expect(app.texts()).toContain("/opt/skillstar-data");
});

test("missing git and gh show install links plus platform commands", async () => {
  const app = await open(HOST, { platform: "macos" });
  await app.press("id:device-nogit");
  await app.waitUntil(
    () =>
      app.texts().some((s) => s === "env:nogit") &&
      app.texts().includes(t("settings.gitInstall")),
  );
  // git 未装：安装外链按钮 + 两条平台安装命令（复制行）。
  expect(app.texts()).toContain(t("settings.gitInstall"));
  expect(app.texts()).toContain("brew install git");
  expect(app.texts()).toContain("conda install git");
  // gh 未装：安装命令面板（macOS 平台标签 + brew install gh）。
  expect(app.texts()).toContain("brew install gh");
  const label = t("settings.ghInstallCommandLabel").replace("{{platform}}", "macOS");
  expect(app.texts()).toContain(label);
});

test("the manual check is single-flight in the UI and lands the fresh chip", async () => {
  const app = await open(HOST);
  await app.press("id:device-reset");
  await app.waitUntil(() => app.texts().some((s) => s === "calls:0"));
  await app.press("id:about-check-update");
  // 检查中只转图标：没有 checking 文案，按钮仍是空闲标签。
  expect(app.texts().filter((s) => s === t("settings.checkingUpdate")).length).toBe(0);
  expect(app.texts()).toContain(t("settings.checkUpdate"));
  // 在途再点：吞掉（仍 1 笔）。
  await app.press("id:about-check-update");
  await app.press("id:device-check-ok");
  await app.waitUntil(() => app.texts().some((s) => s === t("settings.upToDate")));
  expect(app.texts().some((s) => s === "calls:1")).toBe(true);
});
