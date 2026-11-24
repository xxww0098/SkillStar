// desktopShell 退出确认、ErrorBoundary 兜底恢复与 residentWindow 原生桥接测试。
import { expect, open, test } from "nsh/test";
import { Settings } from "skillstar-gui/native";
import { activeCancellableJobs, quitConfirmCopy } from "./desktopShell";
import type { Job } from "./jobBoard";
import {
  backgroundRunEnabled,
  handleMainWindowClosePress,
  seedBackgroundRunPrefs,
  setBackgroundRunEnabled,
} from "./residentWindow";

const HOST = "./desktopShell.test.host.tsx";

function makeJob(partial: Partial<Job> & Pick<Job, "id" | "title">): Job {
  return {
    cancellable: true,
    startedAt: 0,
    done: false,
    ...partial,
  };
}

test("activeCancellableJobs keeps only unfinished cancellable jobs", () => {
  const jobs: Job[] = [
    makeJob({ id: "a", title: "A", cancellable: true, done: false }),
    makeJob({ id: "b", title: "B", cancellable: false, done: false }),
    makeJob({ id: "c", title: "C", cancellable: true, done: true }),
  ];
  expect(activeCancellableJobs(jobs).map((j) => j.id)).toEqual(["a"]);
});

test("quitConfirmCopy formats single, multi, and >3 truncated job lists in zh-CN and en", () => {
  const one = [makeJob({ id: "1", title: "Install skill" })];
  expect(quitConfirmCopy(one, "zh-CN")).toEqual({
    title: "取消进行中的任务并退出？",
    body: "立即退出将会中断以下正在执行的任务：\n· Install skill",
    okText: "中止并退出",
  });
  expect(quitConfirmCopy(one, "en")).toEqual({
    title: "Cancel active task and quit?",
    body: "Quitting now will cancel the ongoing tasks below:\n· Install skill",
    okText: "Cancel & Quit",
  });

  const four = [
    makeJob({ id: "1", title: "Job 1" }),
    makeJob({ id: "2", title: "Job 2" }),
    makeJob({ id: "3", title: "Job 3" }),
    makeJob({ id: "4", title: "Job 4" }),
  ];
  const zhMany = quitConfirmCopy(four, "zh-CN");
  expect(zhMany.title).toBe("取消 4 项进行中的任务并退出？");
  expect(zhMany.body).toContain("· ……等共 4 项任务");
  expect(zhMany.body).not.toContain("Job 4");

  const enMany = quitConfirmCopy(four, "en");
  expect(enMany.title).toBe("Cancel 4 active tasks and quit?");
  expect(enMany.body).toContain("· …and 1 more");
  expect(enMany.body).not.toContain("Job 4");
});

test("Settings.trayIconDataUri is strongly typed and returns an SVG data URI", () => {
  expect(typeof Settings.trayIconDataUri).toBe("function");
  expect(Settings.trayIconDataUri().startsWith("data:image/svg+xml;base64,")).toBe(true);
});

test("Settings outside-click watcher resolves false and cancels cleanly in headless tests", async () => {
  expect(typeof Settings.waitPopoverOutsideClick).toBe("function");
  expect(typeof Settings.cancelPopoverOutsideClick).toBe("function");
  Settings.cancelPopoverOutsideClick();
  const dismissed = await Settings.waitPopoverOutsideClick();
  expect(dismissed).toBe(false);
  Settings.cancelPopoverOutsideClick();
});

test("requestAppQuit safely no-ops without confirm when no cancellable jobs exist", async () => {
  const app = await open(HOST, { width: 960, height: 640, reducedMotion: true });
  await app.press("id:host-quit");
  await app.wait(50);
  expect(app.shows("id:dialog-confirm-card")).toBe(false);
});

test("requestAppQuit prompts on active cancellable job, resets guard on cancel, and runs onCancel on ok", async () => {
  const app = await open(HOST, { width: 960, height: 640, reducedMotion: true });
  await app.press("id:host-add-job");
  await app.press("id:host-quit");
  await app.waitUntil(() => app.shows("id:dialog-confirm-card"), { timeout: 2000 });
  expect(app.texts().some((line) => line.includes("Syncing skills"))).toBe(true);

  await app.press("id:dialog-confirm-cancel");
  await app.waitUntil(() => !app.shows("id:dialog-confirm-card"), { timeout: 2000 });
  expect(app.texts()).toContain("cancel-count:0");

  // Re-trigger quit to verify quitConfirmOpen guard was cleared on cancel.
  await app.press("id:host-quit");
  await app.waitUntil(() => app.shows("id:dialog-confirm-card"), { timeout: 2000 });

  await app.press("id:dialog-confirm-ok");
  await app.waitUntil(() => !app.shows("id:dialog-confirm-card"), { timeout: 2000 });
  await app.waitUntil(() => app.texts().includes("cancel-count:1"), { timeout: 2000 });
});

test("dropping all active cancellable jobs closes the quit confirm dialog", async () => {
  const app = await open(HOST, { width: 960, height: 640, reducedMotion: true });
  await app.press("id:host-add-job");
  await app.press("id:host-quit");
  await app.waitUntil(() => app.shows("id:dialog-confirm-card"), { timeout: 2000 });

  await app.press("id:host-drop-job");
  await app.waitUntil(() => !app.shows("id:dialog-confirm-card"), { timeout: 2000 });
  expect(app.texts()).toContain("cancel-count:0");
});

test("ErrorBoundary renders ShellErrorFallback and recovers child on retry", async () => {
  const app = await open(HOST, { width: 960, height: 640, reducedMotion: true });
  expect(app.shows("id:recovered-child")).toBe(true);

  await app.press("id:host-crash-page");
  await app.waitUntil(() => app.shows("id:page-error-projects"), { timeout: 2000 });
  expect(app.shows("id:page-error-projects-retry")).toBe(true);
  expect(app.texts()).toContain("boom in page render");

  await app.press("id:host-fix-page");
  await app.press("id:page-error-projects-retry");
  await app.waitUntil(() => app.shows("id:recovered-child"), { timeout: 2000 });
  expect(app.shows("id:page-error-projects")).toBe(false);
});

test("backgroundRunEnabled seeds from prefs and controls closePress hide vs quit", () => {
  seedBackgroundRunPrefs(() => ({ background_run: true }));
  expect(backgroundRunEnabled()).toBe(true);

  let hidden = 0;
  let closedPopover = 0;
  let quitCalls = 0;
  const fakeWin = {
    hide: () => {
      hidden += 1;
    },
    focus: () => {},
  };

  expect(
    handleMainWindowClosePress(fakeWin, {
      closePopover: () => {
        closedPopover += 1;
      },
      quitApp: () => {
        quitCalls += 1;
      },
    }),
  ).toBe("hidden");
  expect(hidden).toBe(1);
  expect(closedPopover).toBe(0);
  expect(quitCalls).toBe(0);

  setBackgroundRunEnabled(false);
  expect(backgroundRunEnabled()).toBe(false);
  expect(
    handleMainWindowClosePress(fakeWin, {
      closePopover: () => {
        closedPopover += 1;
      },
      quitApp: () => {
        quitCalls += 1;
      },
    }),
  ).toBe("quit");
  expect(hidden).toBe(1);
  expect(closedPopover).toBe(1);
  expect(quitCalls).toBe(1);

  seedBackgroundRunPrefs(() => {
    throw new Error("prefs unavailable");
  });
  expect(backgroundRunEnabled()).toBe(false);
});
