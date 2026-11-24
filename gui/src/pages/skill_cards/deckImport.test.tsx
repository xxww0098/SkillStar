// Deck-import dialog (skill_cards/deck_import.rs 对位) — headless data-flow
// and interaction checks on the slice's own harness entry (the deck page
// that owns the toolbar trigger is a parallel slice's file). Product locale
// is zh-CN via the locale signal's initial value, not the headless system
// locale. The share code here is a real uncompressed code (version byte,
// compression flag, LE-f64 now, JSON body) whose only member has no install
// source: the install pipeline reads the hub, records the skip, and writes
// nothing — so the test never touches the developer's real storage.
import { expect, open, test } from "nsh/test";

const HARNESS = "./deckImportHarness.tsx";

/// Little-endian f64 bytes for the share code's timestamp slot.
function leF64(n: number): number[] {
  const buffer = new ArrayBuffer(8);
  new DataView(buffer).setFloat64(0, n, true);
  return Array.from(new Uint8Array(buffer));
}

/// An uncompressed share code around `payload` (share_install.rs wire
/// layout: version, compression flag 0, LE-f64 milliseconds, raw JSON).
function shareCode(
  prefix: "ags-" | "agd-",
  payload: unknown,
  timestamp = Date.now(),
): string {
  const json = Array.from(new TextEncoder().encode(JSON.stringify(payload)));
  const bytes = [1, 0, ...leF64(timestamp), ...json];
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return prefix + btoa(binary);
}

/// A code whose single member has neither a remote nor embedded content:
/// the domain records it as `no_source` without writing anything.
function noSourceSkillsCode(): string {
  return shareCode("ags-", {
    n: "",
    d: "",
    i: "",
    s: [{ n: "no-source-demo", u: "", c: null, p: null }],
  });
}

test("the dialog opens with the deck-scoped import chrome", async () => {
  const app = await open(HARNESS);
  await app.press("deck-import-harness-open");
  await app.waitFor("deck-import-input");
  // Header well + deck title, hint, or-divider, both file-side controls.
  expect(app.texts()).toContain("导入卡组");
  expect(app.texts()).toContain("从分享码、GitHub 仓库链接或 .ags / .agd 卡组文件导入");
  expect(app.texts()).toContain("或");
  expect(app.texts()).toContain("选择 .ags / .agd 文件");
  expect(app.texts()).toContain("导入");
  // Empty field: the primary action cannot run.
  expect(app.enabled("deck-import-submit")).toBe(false);
  expect(app.shows("deck-import-detected")).toBe(false);
});

test("a pasted share code flips the detected banner live", async () => {
  const app = await open(HARNESS);
  await app.press("deck-import-harness-open");
  await app.waitFor("deck-import-input");
  // A bare agd- token of real code length is sniffed as a share code.
  await app.type("deck-import-input", `agd-${"A".repeat(48)}`);
  await app.waitFor("deck-import-detected");
  expect(app.texts()).toContain("已识别分享码");
  expect(app.enabled("deck-import-submit")).toBe(true);
  // A plain path is not a code: the banner leaves, the field stays usable.
  await app.clear("deck-import-input");
  await app.type("deck-import-input", "/tmp/pack.agd");
  expect(app.shows("deck-import-detected")).toBe(false);
  expect(app.enabled("deck-import-submit")).toBe(true);
});

test("a skills share code completes with grouped reasons and reloads decks", async () => {
  const app = await open(HARNESS);
  await app.press("deck-import-harness-open");
  await app.waitFor("deck-import-input");
  await app.type("deck-import-input", noSourceSkillsCode());
  await app.press("deck-import-submit");
  await app.waitFor("deck-import-completed");
  const texts = app.texts();
  expect(texts).toContain("卡组已导入");
  expect(texts).toContain("共请求 1 个技能：已存在 0 个，新安装 0 个，未安装 1 个。");
  expect(texts).toContain("未安装（1）");
  expect(texts).toContain("no-source-demo");
  expect(texts).toContain("缺少安装来源");
  // Success asks the keep-alive deck page to reload (groupsChanged).
  expect(app.shows("deck-import-harness-refreshed")).toBe(true);
  // Done closes the dialog.
  await app.press("deck-import-done");
  expect(app.count("deck-import-completed")).toBe(0);
});

test("the return key runs the same submit path", async () => {
  const app = await open(HARNESS);
  await app.press("deck-import-harness-open");
  await app.waitFor("deck-import-input");
  await app.type("deck-import-input", noSourceSkillsCode());
  await app.submit("deck-import-input");
  await app.waitFor("deck-import-completed");
  expect(app.texts()).toContain("卡组已导入");
});

test("a bundle path that is no file fails, and retry keeps the text", async () => {
  const app = await open(HARNESS);
  await app.press("deck-import-harness-open");
  await app.waitFor("deck-import-input");
  const bogus = "/definitely/not/a/deck-bundle.agd";
  await app.type("deck-import-input", bogus);
  await app.press("deck-import-submit");
  await app.waitFor("deck-import-retry");
  expect(app.texts()).toContain("导入失败");
  // Try again returns to the field with the failed input kept.
  await app.press("deck-import-retry");
  await app.waitFor("deck-import-input");
  expect(app.value("deck-import-input")).toBe(bogus);
  expect(app.texts()).toContain("导入卡组");
});

test("a pasted repository URL shows the repo banner live and clears on bundle path", async () => {
  const app = await open(HARNESS);
  await app.press("deck-import-harness-open");
  await app.waitFor("deck-import-input");
  await app.type("deck-import-input", "https://github.com/vuejs-ai/skills");
  await app.waitFor("deck-import-repo-detected");
  expect(app.shows("deck-import-detected")).toBe(false);
  expect(app.texts()).toContain("已识别仓库地址，点击导入将扫描技能并可「快速打包」为卡组");

  await app.clear("deck-import-input");
  await app.type("deck-import-input", "/tmp/pack.agd");
  expect(app.shows("deck-import-repo-detected")).toBe(false);
  expect(app.shows("deck-import-detected")).toBe(false);
});

test("submitting a repository URL hands off to MySkills import instead of opening a bundle file", async () => {
  const app = await open(HARNESS);
  await app.press("deck-import-harness-open");
  await app.waitFor("deck-import-input");
  await app.type("deck-import-input", "https://github.com/vuejs-ai/skills");
  await app.press("deck-import-submit");
  await app.waitFor("deck-import-harness-handoff");
  expect(app.count("deck-import-card")).toBe(0);
  expect(app.texts()).not.toContain("导入失败");
  expect(app.texts()).toContain("my_skills:https://github.com/vuejs-ai/skills:true");
});

test("an expired share code reports its share error without falling back to file open", async () => {
  const app = await open(HARNESS);
  await app.press("deck-import-harness-open");
  await app.waitFor("deck-import-input");
  const expired = shareCode(
    "agd-",
    { n: "Pack", d: "", i: "", s: [] },
    0,
  );
  await app.type("deck-import-input", expired);
  await app.press("deck-import-submit");
  await app.waitFor("deck-import-retry");
  const texts = app.texts();
  expect(texts).toContain("导入失败");
  expect(texts.some((text) => text.includes("expired"))).toBe(true);
  expect(texts.some((text) => text.includes("Cannot open bundle"))).toBe(false);
});
