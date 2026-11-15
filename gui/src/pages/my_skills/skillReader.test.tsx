// SKILL.md 阅读器测试 — skill_reader.rs（+ reader_style.rs）的 nsh 对位。
// 期望值全部出自真实样本或 GPUI 源码数值（readerTypography() 表逐项带行
// 号）；渲染断言走无头 texts()/shows()/spanStyle()，滚动用 scroll 不拒绝
// + 内容仍在（headless texts() 不做视口裁剪，可见性不能断言——spike 教
// 训）。装置：skillReader.test.host.tsx（假桥注入，不触文件系统）。
import { expect, open, test } from "nsh/test";
import { splitMarkdown } from "./readerBlocks";
import { READER_SAMPLES, prefillLine, proseCount } from "./skillReader.samples";

const HOST = "./skillReader.test.host.tsx";

const ASK_MATT = READER_SAMPLES.askMatt.markdown;
/** ask-matt 的 prose 块数（译文行计数期望，从同一份共享样本推导）。 */
const ASK_MATT_PROSE = proseCount(ASK_MATT);
/** 「You don't remember…」段的预填译文行文本（spanStyle 的唯一选择器）。 */
const YOU_DONT_LINE = splitMarkdown(ASK_MATT)
  .map((block, index) => (block.kind === "prose" && block.markdown.startsWith("You don't remember") ? prefillLine(ASK_MATT, index) : ""))
  .find((line) => line.length > 0) as string;

// ---- 真实样本渲染（spike 四类形态） -------------------------------------------

test("ask-matt renders headings, lists, links and a yaml frontmatter block", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open-askMatt" });
  await app.waitFor("id:skill-md-body");
  const texts = app.texts();
  // 标题各自成条（h1/h2/h3）。
  expect(texts).toContain("Ask Matt");
  expect(texts).toContain("The main flow: idea → ship");
  expect(texts).toContain("Context hygiene");
  // 链接文本内联在段落条目里（texts() 是整块文本，substring 匹配）。
  expect(texts.some((t) => t.includes("smart zone"))).toBe(true);
  // frontmatter 剥成 ```yaml 栅栏：YAML 是多行代码条目而非拍平的段落
  // （skill_reader.rs peel_frontmatter :527）。
  expect(texts.some((t) => t.startsWith("name: ask-matt\n"))).toBe(true);
  expect(texts.some((t) => t.startsWith("name: ask-matt description:"))).toBe(false);
  // 列表项渲染（列表合并成一块，条目内 substring）。
  expect(texts.some((t) => t.includes("/grill-with-docs"))).toBe(true);
  // 卡片头只有技能名：原先行首那枚「SKILL.md」文件标签已去掉。
  expect(texts).not.toContain("SKILL.md");
});

test("a table-heavy SKILL.md renders header and body cells", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open-cuaTable" });
  await app.waitFor("id:skill-md-body");
  const texts = app.texts();
  expect(texts).toContain("Goal");
  expect(texts).toContain("Check installation and capabilities");
  expect(texts).toContain("Runtime");
  expect(texts).toContain("Act on a control");
});

test("fenced code renders verbatim and inline code keeps its line", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open-agentDesktop" });
  await app.waitFor("id:skill-md-body");
  const texts = app.texts();
  // 栅栏内命令行逐字保留（条目含内部空格，substring 匹配）。
  expect(
    texts.some((t) => t.includes('agent-desktop snapshot --skeleton --app "Notes"')),
  ).toBe(true);
  // 行内码与正文同行。
  expect(texts.some((t) => t.includes("supported_surfaces"))).toBe(true);
});

test("codebase-memory renders its decision table and command list", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open-cbm" });
  await app.waitFor("id:skill-md-body");
  const texts = app.texts();
  expect(texts).toContain("Quick Decision Matrix");
  expect(texts).toContain("Who calls X?");
  expect(texts).toContain("trace_path(direction=\"inbound\")");
});

// ---- GPUI 排版映射（readerTypography() 的量化断言） --------------------------

test("headings and inline code hit the GPUI sizes and weights", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open-askMatt" });
  await app.waitFor("id:skill-md-body");
  const h1 = app.spanStyle("id:skill-md-rest >> text:Ask Matt");
  expect(h1.fontSize).toBeGreaterThan(28.5); // 16 × 1.8 = 28.8
  expect(h1.fontSize).toBeLessThan(29.1);
  expect(h1.fontWeight).toBe(600);
  const h2 = app.spanStyle("id:skill-md-rest >> text:The main flow: idea → ship");
  expect(Math.round(h2.fontSize)).toBe(21); // 16 × 4/3 = 21.33
  expect(h2.fontWeight).toBe(600);
  const h3 = app.spanStyle("id:skill-md-rest >> text:Context hygiene");
  expect(Math.round(h3.fontSize)).toBe(18); // 16 × 17/15 = 18.13
  expect(h3.fontWeight).toBe(600);
  // 行内码 14px（GPUI 0.875 × 16；/teach 在样本中唯一）。
  const inlineCode = app.spanStyle("id:skill-md-rest >> text:/teach");
  expect(Math.round(inlineCode.fontSize)).toBe(14);
});

// ---- 双主题（paletteToken 在渲染时读取 + 引擎外观跟随） ------------------------

// 回归：排版表曾经是模块级常量，paletteToken() 在模块导入时取值，切到 paper
// 后正文一直是 #f2efe9（白卡片上几乎看不见）、代码块仍是深色底。
test("a theme swap repaints the markdown already on screen", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open-askMatt" });
  await app.waitFor("id:skill-md-body");
  const heading = () => app.spanStyle("id:skill-md-rest >> text:Ask Matt").color;
  const body = () => app.spanStyle("id:skill-md-rest >> text:The main flow: idea → ship").color;
  try {
    expect(heading()).toBe("#f2efe9ff"); // current fg
    expect(body()).toBe("#f2efe9ff");
    await app.host.send("harness", { action: "theme-paper" });
    expect(heading()).toBe("#18181bff"); // paper fg
    expect(body()).toBe("#18181bff");
    await app.host.send("harness", { action: "theme-current" });
    expect(heading()).toBe("#f2efe9ff");
  } finally {
    // 主题是模块级 signal：断言失败也要把装置放回深色，后面的测试才不受影响。
    await app.host.send("harness", { action: "theme-current" });
  }
});

test("paper mode reads the light palette and the light engine tokens", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "theme-paper" });
  await app.host.send("harness", { action: "open-cuaTable" });
  await app.waitFor("id:skill-md-body");
  try {
    // 表头文字不吃 palette 部件样式，取引擎 token 的弱化墨色：引擎外观没跟着
    // 切浅色的话这里是深色主题的 #9ca0ab。
    expect(app.spanStyle("id:skill-md-rest >> text:Goal").color).toBe("#57534eff");
    // 链接同理取引擎 primary（#b45309 而不是深色的 #ba5a08）。
    expect(app.spanStyle("id:skill-md-rest >> text:Runtime").color).toBe("#b45309ff");
    // 正文格子是应用调色板的 paper 墨色。
    expect(
      app.spanStyle("id:skill-md-rest >> text:Check installation and capabilities").color,
    ).toBe("#18181bff");
  } finally {
    await app.host.send("harness", { action: "theme-current" });
  }
});

// ---- 320ms 入场门（ENTRANCE :31 / arm_entrance :139） ------------------------

test("the file stays unmounted while the dialog slides, then appears", async () => {
  const app = await open(HOST, { width: 1200, height: 800, clock: {} });
  await app.host.send("harness", { action: "open-askMatt" });
  expect(app.shows("id:skill-md-body")).toBe(false);
  await app.wait(400); // manual clock：ENTRANCE 320ms 过去
  await app.waitFor("id:skill-md-body");
  expect(app.shows("id:skill-md-card")).toBe(true);
});

test("reduce-motion mounts the file on the first paint", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open-askMatt" });
  await app.waitFor("id:skill-md-body");
});

// ---- Loading / 失败 / 空（Phase 流转） ---------------------------------------

test("a failed read paints the failure card and retry recovers", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open-fail" });
  await app.waitFor("id:skill-md-retry");
  expect(app.texts()).toContain("读不到 SKILL.md。");
  expect(app.texts().some((t) => t.includes("is not installed"))).toBe(true);
  await app.press("skill-md-retry");
  await app.waitFor("id:skill-md-body");
  expect(app.texts()).toContain("Ask Matt");
});

test("an empty SKILL.md paints the empty line and no translate buttons", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open-empty" });
  await app.waitFor("id:skill-md-reader");
  expect(app.texts()).toContain("这份 SKILL.md 是空的。");
  expect(app.shows("id:skill-md-translate")).toBe(false);
  expect(app.shows("id:skill-md-style")).toBe(false);
});

// ---- 翻译模式（file_column :460-494 + schedule_when :206） -------------------

test("chinese prose hides the translate and style buttons", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open-zhOnly" });
  await app.waitFor("id:skill-md-body");
  expect(app.shows("id:skill-md-translate")).toBe(false);
  expect(app.shows("id:skill-md-style")).toBe(false);
  expect(app.shows("id:skill-md-translation")).toBe(false);
});

test("the translate button shows and hides the cached lines without writing the switch", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open-askMatt" });
  await app.waitFor("id:skill-md-body");
  await app.host.send("harness", { action: "prefill" });
  expect(app.shows("id:skill-md-translate")).toBe(true);
  expect(app.shows("id:skill-md-translation")).toBe(false);
  await app.press("skill-md-translate");
  await app.waitFor("id:skill-md-translation");
  const lines = app.texts().filter((t) => t.startsWith("〔译"));
  expect(lines.length).toBe(ASK_MATT_PROSE);
  // 再按一次回到原文：译文行消失（showOriginal）。
  await app.press("skill-md-translate");
  expect(app.shows("id:skill-md-translation")).toBe(false);
  // 翻译选择只写本次打开：Settings 开关读数不动。
  expect(app.texts().some((t) => t.includes("switch:off"))).toBe(true);
});

test("the translated line carries the GPUI 14px metrics under each prose block", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open-askMatt" });
  await app.waitFor("id:skill-md-body");
  await app.host.send("harness", { action: "prefill" });
  await app.press("skill-md-translate");
  await app.waitFor("id:skill-md-translation");
  // spanStyle 的 id: 只配嵌套文本，用 text: 选唯一行（〔译7〕前缀保证）。
  const style = app.spanStyle(`text:${YOU_DONT_LINE}`);
  expect(style.fontSize).toBe(14);
  // 译文行下原文仍是 markdown 块（frontmatter 是 yaml 代码块）。
  expect(app.texts().some((t) => t.startsWith("name: ask-matt\n"))).toBe(true);
});

test("an on switch translates before a click and schedules one fill batch", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "prefill" });
  await app.host.send("harness", { action: "switch" });
  await app.host.send("harness", { action: "open-askMatt" });
  await app.waitFor("id:skill-md-translation");
  // 开关先于点击翻译（the_skill_md_setting 测试对位）：预填后每段都有行。
  const lines = app.texts().filter((t) => t.startsWith("〔译"));
  expect(lines.length).toBeGreaterThan(0);
  expect(app.shows("id:skill-md-translate")).toBe(true);
});

test("an uncached opening schedules one fill of every prose block, capped at 24", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "switch" });
  await app.host.send("harness", { action: "open-askMatt" });
  await app.waitFor("id:skill-md-body");
  // 未预填：排程把全部 prose 块一次交给引擎（schedule_when 批次语义）。
  await app.waitUntil(() => app.texts().some((t) => t.includes("fill:1")));
  expect(app.shows("id:skill-md-translation")).toBe(false);
  await app.host.send("harness", { action: "resolve-fill" });
  await app.waitFor("id:skill-md-translation");
  // fill 成功写盘：每段都出现译文行（〔已译〕前缀）。
  const lines = app.texts().filter((t) => t.startsWith("〔已译〕"));
  expect(lines.length).toBe(ASK_MATT_PROSE);
  expect(lines.length).toBeLessThanOrEqual(24);
});

test("a blocked engine shows the account hint and stays quiet", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "blocked" });
  await app.host.send("harness", { action: "switch" });
  await app.host.send("harness", { action: "open-askMatt" });
  await app.waitFor("id:skill-md-column");
  expect(app.texts().some((t) => t.includes("请先在账户里添加"))).toBe(true);
  expect(app.texts().some((t) => t.includes("fill:0"))).toBe(true);
});

// ---- 译文样式菜单（reader_style.rs） ----------------------------------------

test("the style menu lists every theme and picking one repaints the lines", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open-askMatt" });
  await app.waitFor("id:skill-md-body");
  await app.host.send("harness", { action: "prefill" });
  await app.press("skill-md-translate");
  await app.waitFor("id:skill-md-translation");
  await app.press("skill-md-style");
  await app.waitFor("id:skill-md-style-menu");
  // 默认 quote 选中（DEFAULT_READER）。
  expect(app.shows("id:skill-md-style-quote")).toBe(true);
  expect(app.shows("id:skill-md-style-line")).toBe(true);
  await app.press("skill-md-style-line");
  // 菜单收起，选择落盘，译文行换下划线装饰。
  expect(app.shows("id:skill-md-style-menu")).toBe(false);
  expect(app.texts().some((t) => t.includes("writes:1"))).toBe(true);
  const style = app.spanStyle(`text:${YOU_DONT_LINE}`);
  expect(style.underline).toBe(true);
});

// ---- 滚动（正文唯一 Scroll） -------------------------------------------------

test("a long SKILL.md scrolls inside the reader port", async () => {
  const app = await open(HOST, { width: 1200, height: 800, reducedMotion: true });
  await app.host.send("harness", { action: "open-cuaTable" });
  await app.waitFor("id:skill-md-body");
  // headless 下 scroll 步骤被拒绝才说明不是真滚动容器（spike 方法论）。
  await app.scroll("skill-md-reader", 2000);
  await app.scroll("skill-md-reader", -2000);
  expect(app.shows("id:skill-md-body")).toBe(true);
});

// ---- 分块器纯语义（skill_reader.rs frontmatter_and_code 测试对位） ----------

test("splitMarkdown keeps frontmatter and fences out of prose", () => {
  const blocks = splitMarkdown("---\nname: demo\n---\n\nHello world.\n\n```\ncode\n```\n\nSecond.");
  expect(blocks.map((b) => b.kind)).toEqual(["code", "prose", "code", "prose"]);
  expect(blocks[0].markdown).toContain("name: demo");
  expect(blocks[1].markdown).toBe("Hello world.");
  expect(blocks[3].markdown).toBe("Second.");
});