// shareSheet（skill_cards/share_sheet.rs 对位）的无头测试：装置入口上的
// UI 交互断言（open + press + texts / clipboard）走真实域桥——分享码由
// ss_skills 的 encoder 真的生成（agd- 前缀）、remote/local 计数是 Rust 侧
// 分流决策的结果；.agd 导出落在注入的临时目录（产品路径是下载目录，与
// GPUI 一致，测试不写真实 $HOME）。成员分流的规则单测在 native crate 的
// cargo test（facades/skill_cards_share.rs）。
import { expect, open, test } from "nsh/test";
import { setLocale } from "../../services/locale";
import { t, tf } from "../../services/translation";
import { WINDOW_H, WINDOW_W } from "../../layout";

const HOST = "./shareSheet.test.host.tsx";
const OUT_DIR = "/tmp/skillstar-nsh-share-sheet-host";

// The headless engine's default window is shorter than the product's; every
// UI test opens at the product default (layout.rs WINDOW_W / WINDOW_H) so
// the centered sheet sits in the viewport exactly as in the product.
async function openSheet() {
  const app = await open(HOST);
  await app.setViewport(WINDOW_W, WINDOW_H);
  await app.press("harness-open");
  return app;
}

// ── 文案目录（zh/en 同步，i18n.rs 键位对位）──────────────────────────────

test("both catalogs carry the share-sheet keys with their interpolations", () => {
  // zh-CN 是产品默认（locale 信号初值，不由系统 locale 推断）。
  expect(t("skillCards.shareDeck")).toBe("分享卡组");
  expect(tf("skillCards.shareCodeCount", { count: 2 })).toBe("2 个技能");
  expect(tf("skillCards.shareCodeExcluded", { count: 2 })).toBe(
    "2 个本地技能未包含，请改用打包分享"
  );
  expect(t("skillCards.shareCodeGenerating")).toBe("正在生成…");
  expect(t("shareResultCard.copied")).toBe("分享码已复制到剪贴板");
  expect(t("shareResultCard.pasteToImportHint")).toBe("复制整段消息直接粘贴导入");
  expect(t("skillCards.shareBundleOpenFolder")).toBe("打开所在文件夹");
  setLocale("en");
  expect(t("skillCards.shareDeck")).toBe("Share Deck");
  expect(tf("skillCards.shareCodeCount", { count: 2 })).toBe("2 skill(s)");
  expect(tf("skillCards.shareCodeExcluded", { count: 2 })).toBe(
    "2 local skill(s) excluded — use the bundle"
  );
  expect(t("skillCards.shareBundleHint")).toContain(".agd");
  expect(t("shareResultCard.copied")).toBe("Share code copied to clipboard");
  setLocale("zh-CN");
});

// ── UI 数据流（真实域桥，非 mock）───────────────────────────────────────

test("the sheet generates a real agd- code and counts the local-only members", async () => {
  const app = await openSheet();
  const texts = app.texts();
  // 居中对话框的两条出路都在，头部带卡组名。
  expect(texts).toContain("分享卡组");
  expect(texts).toContain("测试卡组 Deck");
  expect(texts).toContain("分享码");
  expect(texts).toContain("打包分享");
  expect(texts).toContain("导出 .agd");
  // harness 的两个成员没有任何远程来源（deck sources 空、hub 无此名）：
  // 码仍然生成（agd- 前缀来自 ss_skills 的 encoder），remote=0、local=2。
  expect(texts.some((s) => s.startsWith("agd-"))).toBe(true);
  expect(texts).toContain("复制整段消息直接粘贴导入 · 0 个技能");
  expect(texts).toContain("2 个本地技能未包含，请改用打包分享");
});

test("copying puts the code on the clipboard and posts the success notice", async () => {
  const app = await openSheet();
  await app.press("share-copy");
  expect(app.clipboard()?.startsWith("agd-")).toBe(true);
  expect(app.texts()).toContain("分享码已复制到剪贴板");
});

test("the bundle export lands in the injected dir and offers the folder button", async () => {
  const app = await openSheet();
  await app.press("share-export");
  const texts = app.texts();
  // 导出完成：路径行显示写出的 .agd（harness 注入的临时目录），按钮回到
  // 静止文案，「打开所在文件夹」出现。
  expect(texts.some((s) => s.startsWith(`${OUT_DIR}/`) && s.endsWith(".agd"))).toBe(true);
  expect(texts).toContain("导出 .agd");
  expect(texts).not.toContain("正在导出…");
  expect(texts).toContain("打开所在文件夹");
});

test("a press on the veil dismisses the sheet", async () => {
  const app = await openSheet();
  expect(app.texts()).toContain("分享卡组");
  await app.press("share-veil");
  expect(app.texts()).not.toContain("分享卡组");
  // And the sheet reopens cleanly (state reset, no stale path row).
  await app.press("harness-open");
  expect(app.texts()).toContain("分享卡组");
  expect(app.texts().some((s) => s.startsWith("/tmp/"))).toBe(false);
});
