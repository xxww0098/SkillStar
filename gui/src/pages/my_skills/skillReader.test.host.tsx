// SKILL.md 阅读器的无头测试装置（detailDrawer.test.host.tsx 样板）：假
// ReaderBridge 注入 SkillReaderHost 的注入位，装置按钮换脚本（真实样本
// /失败/空）、切 Settings 开关、预填译文缓存、按脚本放行在途 fill；调用
// 与事件计数渲染成文本供 app.texts() 断言。真实域读写由 native crate 的
// cargo test 用隔离数据目录覆盖（my_skills_reader.rs）。
//
// 样本为真实 SKILL.md 的逐字节内嵌（波 3 spike 同款四类形态；来源见各
// 常量注释），测试不触文件系统、不写 $HOME。
// nsh 0.6 test 的 press 会做命中测试：模态遮罩（skillReaderSlot 的
// position:fixed 覆盖层）之下的装置按钮会以 ERR_NSH_TEST_COVERED 拒绝，
// 因此同一批装置动作也挂在 host 事件通道上（见下方的装置动作表）。

import { Button } from "nsh/base";
import { Text, View, host, mount, signal, type JSX } from "nsh";
import { setThemeMode } from "../../services/themeMode";
import { translationPrefsChanged } from "../../shell/events";
import { SkillReaderHost, openSkillReader, type ReaderBridge } from "./skillReaderSlot";
import { READER_SAMPLES, type ReaderSampleId } from "./skillReader.samples";
import { splitMarkdown } from "./readerBlocks";

// ---- 假域件 -------------------------------------------------------------------

/** 当前读取脚本（fail 为一次性：首读拒绝，其后回落 askMatt）。 */
const script = signal<ReaderSampleId | "fail" | "empty">("askMatt");
/** 最近一次打开的样本（预填按钮按它填缓存）。 */
const opened = signal<ReaderSampleId>("askMatt");
const switchOn = signal(false);
const blockedOn = signal(false);
const readCount = signal(0);
const fillCount = signal(0);
// 非响应式计数：promise executor 里不做「读信号再写同信号」——那会被
// 调用方 effect 跟踪成自写自读（skillReaderSlot 侧已加 untracked 兜底，
// 装置侧同样避免）。
let reads = 0;
let fills = 0;

const cache = new Map<string, string>();
let failedOnce = false;
let currentTheme = "quote";
const writeCount = signal(0);

const fillCalls: {
  sources: string[];
  resolve: (value: {
    done: { source: string; text: string }[];
    rejection: { model: string; detail: string } | null;
  }) => void;
  reject: (reason: unknown) => void;
}[] = [];

/** ss-core themes.rs THEMES 的 18 款（id 顺序即菜单顺序）。 */
const THEMES = [
  "none", "line", "dot", "dash", "dbold", "wavy", "wbold", "box", "xbold", "mark",
  "gmark", "fuzzy", "hi", "quote", "grad", "blink", "glow", "color",
].map((id) => ({ id, labelKey: `settings.tranTheme${id[0].toUpperCase()}${id.slice(1)}` }));

const bridge: ReaderBridge = {
  read: (name) =>
    new Promise<string>((resolve, reject) => {
      reads += 1;
      readCount.set(reads);
      const current = script();
      if (current === "fail") {
        if (!failedOnce) {
          failedOnce = true;
          reject(new Error(`Skill '${name}' is not installed`));
          return;
        }
        resolve(READER_SAMPLES.askMatt.markdown);
        return;
      }
      resolve(current === "empty" ? "" : READER_SAMPLES[current].markdown);
    }),
  skillMdEnabled: () => switchOn(),
  lookup: (source) => cache.get(source) ?? null,
  // zh-CN 目标：含拉丁字母的 prose 才需要翻译（needs_translation 语义）。
  needsTranslation: (text) => /[A-Za-z]/.test(text),
  blocked: () => blockedOn(),
  fill: (sources) =>
    new Promise((resolve, reject) => {
      fillCalls.push({ sources, resolve, reject });
      fills += 1;
      fillCount.set(fills);
    }),
  readerThemes: () => THEMES,
  readerTheme: () => currentTheme,
  setReaderTheme: (id) => {
    currentTheme = id;
    writeCount.set(writeCount() + 1);
  },
};

/** 预填当前样本全部 prose 块的译文（〔译N〕前缀保证每行唯一）。 */
function prefill(): void {
  const sample = READER_SAMPLES[opened()];
  splitMarkdown(sample.markdown).forEach((block, index) => {
    if (block.kind === "prose" && bridge.needsTranslation(block.markdown)) {
      cache.set(block.markdown, `〔译${index}〕${block.markdown.slice(0, 12)}`);
    }
  });
}

/** 放行最旧一笔 fill：域语义是成功即写盘（fill → remember），resolve
 * 同时把译文放进假缓存，阅读器的下一次重算才能读到。 */
function resolveFill(): void {
  const call = fillCalls.shift();
  if (call === undefined) return;
  const done = call.sources.map((source) => {
    const text = `〔已译〕${source.slice(0, 8)}`;
    cache.set(source, text);
    return { source, text };
  });
  call.resolve({ done, rejection: null });
}

function rejectFill(): void {
  const call = fillCalls.shift();
  if (call === undefined) return;
  call.resolve({ done: [], rejection: { model: "bad-model", detail: "bad key" } });
}

function failFill(): void {
  const call = fillCalls.shift();
  if (call === undefined) return;
  call.reject(new Error("network down"));
}

function missFill(): void {
  const call = fillCalls.shift();
  if (call === undefined) return;
  call.resolve({ done: [], rejection: null });
}

function openSample(id: ReaderSampleId): void {
  script.set(id);
  opened.set(id);
  openSkillReader(READER_SAMPLES[id].name);
}

// ---- 装置动作表 ---------------------------------------------------------------

// 0.5 起 nsh test 的 `press` 会做命中测试：模态遮罩（见 skillReaderSlot 的
// position:fixed 覆盖层）之下的装置按钮会以 ERR_NSH_TEST_COVERED 拒绝。装置
// 因此把同一批动作同时挂到 host 事件通道（nsh 给测试的驱动面，见
// nsh/test 的 AppHost.send），测试用 app.host.send("harness", {action}) 驱动，
// 按钮保留给看得见的路径与同在装置内的断言。键 = 按钮 testID 去掉 host- 前缀。
const harnessActions: Record<string, () => void> = {
  // 样本按钮按 READER_SAMPLES 的键自动生成（open-<sampleId>）。
  ...Object.fromEntries(
    (Object.keys(READER_SAMPLES) as ReaderSampleId[]).map((id) => [
      `open-${id}`,
      () => openSample(id),
    ]),
  ),
  "open-fail": () => {
    failedOnce = false;
    script.set("fail");
    opened.set("askMatt");
    openSkillReader("missing-skill");
  },
  "open-empty": () => {
    script.set("empty");
    opened.set("askMatt");
    openSkillReader("empty-skill");
  },
  switch: () => switchOn.set(!switchOn()),
  blocked: () => blockedOn.set(!blockedOn()),
  prefill,
  "resolve-fill": resolveFill,
  "reject-fill": rejectFill,
  "fail-fill": failFill,
  "miss-fill": missFill,
  "prefs-changed": () => translationPrefsChanged.publish(),
  "theme-line": () => {
    currentTheme = "line";
  },
  "theme-paper": () => setThemeMode("paper"),
  "theme-current": () => setThemeMode("current"),
  reset: () => {
    cache.clear();
    failedOnce = false;
    currentTheme = "quote";
    writeCount.set(0);
    fillCalls.length = 0;
    fills = 0;
    fillCount.set(0);
    switchOn.set(false);
    blockedOn.set(false);
    setThemeMode("current");
  },
};

host.on("harness", (data) => {
  const action = (data as { action?: string } | null)?.action;
  if (action !== undefined) harnessActions[action]?.();
});

// ---- 装置 ---------------------------------------------------------------------

function App(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", height: "100%", gap: 6 }}>
      <View style={{ display: "flex", flexDirection: "row", gap: 6, flexWrap: "wrap" }}>
        <Each of={Object.keys(READER_SAMPLES) as ReaderSampleId[]} key={(id) => id}>
          {(id: () => ReaderSampleId) => (
            <Button
              testID={`host-open-${id()}`}
              onPress={() => harnessActions[`open-${id()}`]?.()}
            >
              <Text>{id()}</Text>
            </Button>
          )}
        </Each>
        <Button testID="host-open-fail" onPress={harnessActions["open-fail"]}>
          <Text>fail</Text>
        </Button>
        <Button testID="host-open-empty" onPress={harnessActions["open-empty"]}>
          <Text>empty</Text>
        </Button>
      </View>
      <View style={{ display: "flex", flexDirection: "row", gap: 6, flexWrap: "wrap" }}>
        <Button testID="host-switch" onPress={harnessActions["switch"]}>
          <Text>switch:{switchOn() ? "on" : "off"}</Text>
        </Button>
        <Button testID="host-blocked" onPress={harnessActions["blocked"]}>
          <Text>blocked:{blockedOn() ? "on" : "off"}</Text>
        </Button>
        <Button testID="host-prefill" onPress={harnessActions["prefill"]}>
          <Text>prefill</Text>
        </Button>
        <Button testID="host-resolve-fill" onPress={harnessActions["resolve-fill"]}>
          <Text>resolve-fill</Text>
        </Button>
        <Button testID="host-theme-line" onPress={harnessActions["theme-line"]}>
          <Text>theme-line</Text>
        </Button>
        <Button testID="host-reset" onPress={harnessActions["reset"]}>
          <Text>reset</Text>
        </Button>
      </View>
      <Text testID="host-counters" style={{ fontSize: 12 }}>
        {`read:${readCount()} fill:${fillCount()} writes:${writeCount()}`}
      </Text>
      <View style={{ flex: 1, minHeight: 0 }}>
        <SkillReaderHost bridge={bridge} />
      </View>
    </View>
  );
}

mount(App);
