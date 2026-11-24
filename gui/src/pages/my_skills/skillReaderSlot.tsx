import {
  Canvas,
  Overlay,
  Path2D,
  Scroll,
  Text,
  View,
  batch,
  computed,
  effect,
  signal,
  untracked,
  windowSize,
  type Accessor,
  type JSX,
  type ViewRef,
} from "nsh";
import { TextView } from "nsh/base";
import { paletteToken } from "../../services/themeMode";
import { Tip } from "../../chrome/tooltip";
import { IconSweep, Pulse } from "../../chrome/feedback";
import { bindDelayedFlag } from "../../chrome/delayedFlag";
import { promptVeil, SHEET_SHADOW } from "../../chrome/dialog";
import { fadeTransition, motionEnabled } from "../../chrome/motion";
import { dismissReplaceKey } from "../../shell/notify";
import { publishToast, translationPrefsChanged } from "../../shell/events";
import { MySkillsReader } from "skillstar-gui/native";
import { splitMarkdown, type MdBlock } from "./readerBlocks";
import { readerLineTheme, readerTypography, translationBase } from "./readerStyle";
import { t } from "../../services/translation";
import { readerRejectionMessage, rt } from "./readerStrings";

// SKILL.md 阅读器 — my_skills/skill_reader.rs（+ reader_style.rs）对位。
//
// 【形态】窗口正中浮层（open_centered ：182-212）：veil（点外收起）+
// Escape 收起回到抽屉；卡片宽 clamp(vw−80, 720, 1040)、内容高
// reader_shell_height ：177（clamp(vh−114, 400, 840)），chrome 34 是卡片
// 自身的 padding+border。装载 320ms 后再挂正文（ENTRANCE :31；kit 对话框
// 250ms 入场滑停后建层的近似），reduce-motion 首帧直挂（arm_entrance
// :139）。打开即读（Origin::Installed 经 MySkillsReader facade），失败画
// 错误卡 + 重试（Phase::Failed → start 重跑）。
//
// 【翻译】中文界面逐段译文行：每段英文原文下挂缓存译文（file_column
// :460-494，译文≠原文才画；text_sm/行高 20/gap_1，块间 gap_3）。翻译选择
// 只写本次打开（translation_chosen：开reader时读一次 Settings 的
// SKILL.md 开关，翻译按钮翻转不落盘）。排程对位 schedule_when :206：
// 开着译文才排、blocked/已拒绝不排、一次至多 24 段、inflight 去重、传输
// 失败本会话静默；模型 HTTP-400 拒绝 → 会话级钉住 + 通知带（replaceKey
// 互相顶替），下一次「原文→译文」清除（retry_rejected）。
//
// 【样式】译文行样式菜单（reader_style.rs）：18 款保存样式，选中写
// reader_theme 并整面重绘；样式映射与译文行主题在 readerStyle.ts。
//
// 【渲染】nsh TextView + readerTypography()（波 3 spike 裁决：无需自研
// 渲染器）。非翻译模式单 TextView + options.frontmatter + renderCustom
// 把 frontmatter 复刻为 ```yaml 栅栏（peel_frontmatter :527 的干净对位，
// 比字符预处理多白得语法高亮）；翻译模式按 readerBlocks.ts 分块逐块挂
// TextView。
//
// 【nsh 纪律】组件体只执行一次（Solid 式语义）：signal 消费全部落在 JSX
// 表达式、<If>/<Each> 或 effect 里；模块级 effect 是页面实体（state.ts
// 同款）。域调用经 ReaderBridge 注入位（facadeBridge 真桥 / 测试假桥，
// 市场详情列 translate.ts 同款）。

/** Kit dialog slides 250ms; the reader waits a beat longer (skill_reader.rs
 * ENTRANCE :31). Reduce-motion mounts the file on the first paint. */
const ENTRANCE_MS = 320;

/** Kit dialog `max_h` keeps `spacing.lg` clear of each edge (:33-34). */
const DIALOG_MARGIN = 32;

/** Padding and border outside the measured child (`DialogChrome::Padded`
 * :36-37) — the card's own padding + border here. */
const DIALOG_CHROME = 34;

/** The dialog subtracts the window title inset before that max height
 * (:39-41). */
const WINDOW_INSET = 48;

/** One fill call's batch cap (translation.rs schedule_when :232). */
const FILL_BATCH_CAP = 24;

// ---- 域桥 --------------------------------------------------------------------

export interface ReaderBridge {
  /** Origin::Installed 的文件读（Phase::Ready/Failed 的域侧）。 */
  read(name: string): Promise<string>;
  /** Settings 的 SKILL.md 开关（enabled(Surface::SkillMd)），只读。 */
  skillMdEnabled(): boolean;
  /** 逐段缓存译文（display_when 的 lookup 半），未命中 null。 */
  lookup(source: string): string | null;
  /** 目标语言是否需要翻译这段（按钮门 offers_translation :506）。 */
  needsTranslation(text: string): boolean;
  /** LLM 引擎选了但跑不起来（blocked_hint 门）。 */
  blocked(): boolean;
  /** 实时引擎调用（schedule_when → fill），成功写盘后回 done/rejection。 */
  fill(
    sources: string[],
  ): Promise<{
    done: { source: string; text: string }[];
    rejection: { model: string; detail: string } | null;
  }>;
  /** 样式菜单目录（reader_themes，菜单顺序）。 */
  readerThemes(): { id: string; labelKey: string }[];
  /** 保存的译文行样式（canonicalized，默认 quote）。 */
  readerTheme(): string;
  /** 写译文行样式（apply_theme :143），不写其他设置。 */
  setReaderTheme(id: string): void | Promise<void>;
}

/** 真桥：MySkillsReader facade。`read` / `setReaderTheme` 在 Rust 侧走 blocking 池
 * （async Promise）；密钥与模型读取自「设置 → 翻译」保存的配置与密钥文件。 */
export const facadeBridge: ReaderBridge = {
  read: (name) => MySkillsReader.read(name),
  skillMdEnabled: () => MySkillsReader.skillMdEnabled(),
  lookup: (source) => MySkillsReader.lookup(source),
  needsTranslation: (text) => MySkillsReader.needsTranslation(text),
  blocked: () => MySkillsReader.blocked(),
  fill: (sources) =>
    MySkillsReader.fill(sources) as Promise<{
      done: { source: string; text: string }[];
      rejection: { model: string; detail: string } | null;
    }>,
  readerThemes: () => MySkillsReader.readerThemes() as { id: string; labelKey: string }[],
  readerTheme: () => MySkillsReader.readerTheme(),
  setReaderTheme: (id) => MySkillsReader.setReaderTheme(id),
};

/** 当前生效的桥（Host 挂载时安装一次；测试注入假桥）。 */
let bridge: ReaderBridge = facadeBridge;

// ---- 页面实体信号 ------------------------------------------------------------

/** 当前打开的阅读器身份（null = 关闭）。 */
export const readerOpen = signal<string | null>(null);

type Phase =
  | { kind: "loading" }
  | { kind: "ready"; markdown: string }
  | { kind: "failed"; error: string };

const phase = signal<Phase>({ kind: "loading" });
const revealed = signal(false);
/** 本次打开是否显示译文（translation_chosen：开时读一次开关）。 */
const showTranslation = signal(false);
/** 代数：译文落地/拒绝/清标记/换样式后 bump，JSX 借它重读 lookup。 */
const generation = signal(0);
/** 会话级模型拒绝文案（failure_line 的数据面）。 */
const rejectionLine = signal<string | null>(null);
const styleMenuOpen = signal(false);
const styleTriggerRef = signal<ViewRef | null>(null);

/** 会话簿记（GPUI 进程级 statics 的对位物，translate.ts 同款）。 */
const inflight = new Set<string>();
const failed = new Set<string>();
/** translation_chosen 的记录（每次打开重选一次；关闭即复位）。 */
let translationChosenFor: string | null = null;
let readEpoch = 0;
let prefsEpoch = 0;
let revealTimer: ReturnType<typeof setTimeout> | null = null;

translationPrefsChanged.subscribe(() => {
  prefsEpoch += 1;
  inflight.clear();
  failed.clear();
  batch(() => {
    rejectionLine.set(null);
    generation.update((n) => n + 1);
  });
});

// ---- 打开 / 关闭 / 重试（open_reader / Phase 流转） ---------------------------

/** 详情抽屉「查看 SKILL.md…」的唯一入口（open_skill_reader :159）。 */
export function openSkillReader(name: string): void {
  phase.set({ kind: "loading" });
  // arm_entrance :139：reduce-motion 首帧直挂，否则 320ms 滑停后挂。
  revealed.set(!motionEnabled());
  if (revealTimer !== null) clearTimeout(revealTimer);
  revealTimer = motionEnabled()
    ? setTimeout(() => {
        revealTimer = null;
        revealed.set(true);
      }, ENTRANCE_MS)
    : null;
  readerOpen.set(name);
}

/** veil 点外 / Escape：收起回到抽屉（selectedSkill 不动）。 */
function closeReader(): void {
  readEpoch += 1;
  if (revealTimer !== null) {
    clearTimeout(revealTimer);
    revealTimer = null;
  }
  readerOpen.set(null);
  revealed.set(false);
  styleMenuOpen.set(false);
  phase.set({ kind: "loading" });
}

/** 失败卡的重试（Phase::Failed → Loading → start 重读）。 */
function retryRead(): void {
  phase.set({ kind: "loading" });
  const name = readerOpen();
  if (name === null) return;
  const epoch = ++readEpoch;
  void runRead(epoch, name);
}

function bump(): void {
  generation.set(generation() + 1);
}

// ---- 读取驱动（start :97-137） -----------------------------------------------

/** 打开身份驱动读取（模块级 effect，页面实体）。 */
effect(() => {
  const name = readerOpen();
  if (name === null) {
    readEpoch += 1;
    translationChosenFor = null;
    return;
  }
  // untracked：桥实现若在 read 里读写宿主信号（测试装置的计数器），
  // 不得成为本 effect 的依赖（自写自读即死循环）。
  untracked(() => {
    // translation_chosen：每次打开读一次开关（重试不重选）。
    if (translationChosenFor !== name) {
      translationChosenFor = name;
      try {
        showTranslation.set(bridge.skillMdEnabled());
      } catch {
        showTranslation.set(false);
      }
    }
    const epoch = ++readEpoch;
    void runRead(epoch, name);
  });
});

async function runRead(epoch: number, name: string): Promise<void> {
  try {
    const markdown = await bridge.read(name);
    if (epoch !== readEpoch) return;
    phase.set({ kind: "ready", markdown });
  } catch (err) {
    if (epoch !== readEpoch) return;
    phase.set({ kind: "failed", error: String(err) });
  }
}

// ---- 翻译排程（render :216-233 + schedule_when :206） -------------------------

effect(() => {
  if (!revealed() || !showTranslation()) return;
  const current = phase();
  void generation();
  if (current.kind !== "ready") return;
  const markdown = current.markdown;
  // 同上：桥的 lookup/blocked 读取不进依赖（测试装置的信号）。
  untracked(() => {
    const prose = splitMarkdown(markdown)
      .filter((block) => block.kind === "prose")
      .map((block) => block.markdown);
    scheduleProse(prose);
  });
});

function scheduleProse(sources: string[]): void {
  if (bridge.blocked() || rejectionLine() !== null) return;
  const batch: string[] = [];
  for (const source of sources) {
    const trimmed = source.trim();
    if (trimmed.length === 0 || !bridge.needsTranslation(trimmed)) continue;
    if (bridge.lookup(trimmed) !== null || failed.has(trimmed) || inflight.has(trimmed)) continue;
    inflight.add(trimmed);
    batch.push(trimmed);
    if (batch.length === FILL_BATCH_CAP) break;
  }
  if (batch.length === 0) return;
  const runPrefsEpoch = prefsEpoch;
  bridge
    .fill(batch)
    .then((report) => {
      if (runPrefsEpoch !== prefsEpoch) return;
      for (const source of batch) inflight.delete(source);
      if (report.rejection !== null) {
        // 每会话只播报一次；同 replaceKey 的旧通知被顶替。
        // 失败同时留在各段下面，通知关掉之后仍能重试。
        for (const source of batch) failed.add(source);
        const message = readerRejectionMessage(report.rejection.model, report.rejection.detail);
        rejectionLine.set(message);
        publishToast({
          tone: "error",
          message,
          pinned: true,
          replaceKey: "translation-model-rejected",
        });
      } else {
        const done = new Set(report.done.map((item) => item.source));
        for (const source of batch) {
          if (done.has(source) || bridge.lookup(source) !== null) failed.delete(source);
          else failed.add(source);
        }
        if (report.done.length > 0 && rejectionLine() === null) {
          dismissReplaceKey("translation-model-rejected");
        }
      }
      bump();
    })
    .catch(() => {
      if (runPrefsEpoch !== prefsEpoch) return;
      // 传输失败：本会话静默这批（GPUI 按帧重问，effect 重跑会热循环）。
      for (const source of batch) {
        inflight.delete(source);
        failed.add(source);
      }
      bump();
    });
}

/** resolved() ：170-193：已缓存/不需要译/已拒绝/已失败 都算落定。 */
function proseResolved(source: string): boolean {
  const trimmed = source.trim();
  if (trimmed.length === 0 || !bridge.needsTranslation(trimmed)) return true;
  if (bridge.lookup(trimmed) !== null || rejectionLine() !== null) return true;
  return failed.has(trimmed);
}

// ---- 挂载点（index.tsx 页根渲染一次） ----------------------------------------

/**
 * 阅读器宿主：页根常挂，readerOpen 非空时渲染浮层。`bridge` 注入位供
 * 无头测试换假桥（真桥默认）。
 */
export function SkillReaderHost(props: { bridge?: ReaderBridge }): JSX.Element | null {
  bridge = props.bridge ?? facadeBridge;
  return (
    <If when={readerOpen()}>
      {(open: () => string) => <ReaderCard name={open()} />}
    </If>
  );
}

// ---- 浮层与卡片 ---------------------------------------------------------------

/** reader_shell_height ：177：剩余高度而非预算高度。 */
function shellHeight(): number {
  const vh = windowSize().height;
  const fit = vh - DIALOG_MARGIN - DIALOG_CHROME - WINDOW_INSET;
  return Math.min(Math.max(fit, 400), 840);
}

function cardWidth(): number {
  return Math.min(Math.max(windowSize().width - 80, 720), 1040);
}

function ReaderCard(props: { name: string }): JSX.Element {
  return (
    <Overlay
      style={{
        position: "fixed",
        top: 0,
        left: 0,
        right: 0,
        bottom: 0,
        zIndex: 60,
      }}
      onCancel={closeReader}
    >
      {/* Veil（kit 对话框遮罩 prompt_veil）：点外收起。 */}
      <View
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          right: 0,
          bottom: 0,
          backgroundColor: promptVeil(),
        }}
        onPress={closeReader}
      />
      <View
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          right: 0,
          bottom: 0,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <View
          testID="skill-md-card"
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 12,
            width: cardWidth(),
            height: shellHeight() + DIALOG_CHROME,
            minHeight: 0,
            minWidth: 0,
            padding: 16,
            borderWidth: 1,
            borderColor: paletteToken("border"),
            borderRadius: 12,
            backgroundColor: paletteToken("panel"),
            boxShadow: SHEET_SHADOW,
            transform: "translateY(0)",
            ...fadeTransition(true),
          }}
          onPress={() => {
            // 吞掉按压，veil 不关窗。
          }}
        >
          {/* DialogTitle（title.rs：text_base semibold lh1.25）。 */}
          <Text
            testID="skill-md-title"
            style={{
              fontSize: 16,
              fontWeight: "600",
              lineHeight: 20,
              color: paletteToken("fg"),
              paddingRight: 24,
              flexShrink: 0,
            }}
          >
            {props.name}
          </Text>
          <ActionRow />
          <ReaderBody />
        </View>
      </View>
    </Overlay>
  );
}

// ---- 操作行（label_row :246-274 的右半） --------------------------------------

/** 样式按钮与翻译按钮，只在 offers_translation 时出现（中文文案时两枚都不画，
 * reader_style 测试同款）。行首那枚「SKILL.md」文件标签按用户裁定去掉
 * （2026-11-16）：卡片标题就是技能名，再挂一行文件名是重复信息。整行跟着
 * 按钮一起藏：没有可翻的内容时不占高度。 */
const parsedBlocks = computed<MdBlock[]>(() => {
  const current = phase();
  if (current.kind !== "ready" || current.markdown.trim().length === 0) {
    return [];
  }
  return splitMarkdown(current.markdown);
});

const proseSources = computed<string[]>(() =>
  parsedBlocks()
    .filter((block) => block.kind === "prose")
    .map((block) => block.markdown),
);

/** offers_translation ：506 / translation_pending ：513。 */
const translateGate = computed<{ visible: boolean; pending: boolean }>(() => {
  void generation();
  const prose = proseSources();
  if (prose.length === 0) {
    return { visible: false, pending: false };
  }
  const visible = prose.some((source) => bridge.needsTranslation(source));
  const pending =
    visible &&
    showTranslation() &&
    !bridge.blocked() &&
    prose.some((source) => !proseResolved(source));
  return { visible, pending };
});

function ActionRow(): JSX.Element {
  return (
    <If when={translateGate().visible}>
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "flex-end",
          gap: 8,
          minWidth: 0,
          paddingRight: 24,
          flexShrink: 0,
        }}
      >
        <StyleButton />
        <TranslateButton />
      </View>
    </If>
  );
}

/** 翻译按钮（translate_button :281-332）：图标 14、图标式、pending 时激光
 * 扫描；点击只翻转本次打开（不写 Settings 开关）。 */
function TranslateButton(): JSX.Element {
  return (
    <View
      testID="skill-md-translate"
      tooltip={<Tip text={translateTip()} />}
      style={{
        display: "flex",
        padding: 3,
        borderRadius: 6,
        flexShrink: 0,
        cursor: "pointer",
      }}
      hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
      onPress={toggleTranslation}
    >
      <IconSweep
        name="languages"
        size={14}
        color={paletteToken("fg_muted")}
        sweep={translateGate().pending}
        testID="skill-md-translate-sweep"
      />
    </View>
  );
}

function translateTip(): string {
  const gate = translateGate();
  return showTranslation() && !gate.pending
    ? rt("detailPanel.showOriginal")
    : rt("detailPanel.translate");
}

function toggleTranslation(): void {
  const next = !showTranslation();
  showTranslation.set(next);
  if (next) {
    // retry_rejected：从原文翻过去清拒绝与失败标记，引擎可以再问。
    rejectionLine.set(null);
    failed.clear();
  }
  bump();
}

// ---- 本地图形（chrome/icons 无 highlighter 字形；切片自持，24×24 stroke
//      网格同款。翻译按钮的 languages 字形与激光扫描走 chrome 的
//      Icon / IconSweep） ------------------------------------------------------

const GLYPHS: Record<string, string[]> = {
  // Lucide "highlighter"：笔尖加笔杆。
  highlighter: [
    "m9 11-6 6v3h9l3-3",
    "m22 12-4.6 4.6a2 2 0 0 1-2.8 0l-5.2-5.2a2 2 0 0 1 0-2.8L14 4",
  ],
  // Lucide "check"：样式菜单的选中勾（chrome/icons 同形）。
  check: ["M20 6 9 17l-5-5"],
};

function Glyph(props: { name: keyof typeof GLYPHS; size?: number; color: string }): JSX.Element {
  const size = props.size ?? 14;
  const scale = size / 24;
  return (
    <Canvas
      style={{ width: size, height: size, flexShrink: 0 }}
      draw={(cx) => {
        cx.scale(scale, scale);
        cx.strokeStyle = props.color;
        cx.lineWidth = 2;
        cx.lineCap = "round";
        cx.lineJoin = "round";
        for (const d of GLYPHS[props.name]) {
          cx.stroke(new Path2D(d));
        }
      }}
    />
  );
}

// ---- 样式按钮与菜单（reader_style.rs） ---------------------------------------

/** 样式按钮：highlighter 图标，弹层锚定触发器（sortMenu 同款 Overlay），
 * 320 宽菜单、8 行可见（VISIBLE_ROWS×ROW_H）内滚。 */
function StyleButton(): JSX.Element {
  return (
    <View style={{ position: "relative", display: "flex", flexShrink: 0 }}>
      <View
        ref={(node) => {
          styleTriggerRef.set(node);
        }}
      >
        <View
          testID="skill-md-style"
          tooltip={<Tip text={rt("settings.tranReaderTheme")} />}
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            width: 26,
            height: 26,
            borderRadius: 6,
            cursor: "pointer",
          }}
          hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
          onPress={() => styleMenuOpen.update((open) => !open)}
        >
          <Glyph name="highlighter" size={14} color={paletteToken("fg_muted")} />
        </View>
      </View>
      <If when={styleMenuOpen()}>
        <Overlay
          anchor={styleTriggerRef()}
          placement="top"
          align="end"
          offset={6}
          onPressOutside={() => styleMenuOpen.set(false)}
          onCancel={() => styleMenuOpen.set(false)}
        >
          <StyleMenu />
        </Overlay>
      </If>
    </View>
  );
}

/** 当前保存样式（generation 里重读：换样式/译文落地后只读一次桥）。 */
const currentReaderTheme = computed(() => {
  void generation();
  return bridge.readerTheme();
});
const activeLineTheme = computed(() => readerLineTheme(currentReaderTheme()));

function StyleMenu(): JSX.Element {
  const themes = bridge.readerThemes();
  return (
    <View
      testID="skill-md-style-menu"
      style={{
        display: "flex",
        width: 320,
        borderRadius: 12,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        boxShadow: SHEET_SHADOW,
      }}
      onPress={() => {
        // 吞掉按压，透明幕不收菜单。
      }}
    >
      <View style={{ width: "100%", height: 8 * 44 }}>
        <Scroll testID="skill-md-style-list" style={{ width: "100%", height: "100%" }}>
          <View style={{ display: "flex", flexDirection: "column", gap: 4, padding: 4 }}>
            <Each of={themes} key={(theme) => theme.id}>
              {(theme: () => { id: string; labelKey: string }) => (
                <StyleRow
                  id={theme().id}
                  labelKey={theme().labelKey}
                  selected={theme().id === currentReaderTheme()}
                />
              )}
            </Each>
          </View>
        </Scroll>
      </View>
    </View>
  );
}

/** 样式行（style_row :89-141）：样式样例 + 名称（96 宽）+ 选中 Check。 */
function StyleRow(props: { id: string; labelKey: string; selected: boolean }): JSX.Element {
  const line = readerLineTheme(props.id);
  return (
    <View
      testID={`skill-md-style-${props.id}`}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 8,
        width: "100%",
        minWidth: 0,
        paddingHorizontal: 8,
        paddingVertical: 4,
        borderRadius: 6,
        backgroundColor: props.selected ? paletteToken("accent_soft") : "transparent",
        cursor: "pointer",
      }}
      hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
      onPress={() => applyReaderTheme(props.id)}
    >
      <View
        style={{
          flex: 1,
          minWidth: 0,
          overflow: "hidden",
          display: "flex",
          ...line.view,
        }}
      >
        <Text style={{ fontSize: 14, fontWeight: "600", ...line.text }}>
          {rt("settings.tranStyleSample")}
        </Text>
      </View>
      <Text
        style={{
          width: 96,
          flexShrink: 0,
          fontSize: 12,
          color: props.selected ? paletteToken("fg") : paletteToken("fg_muted"),
        }}
      >
        {rt(props.labelKey)}
      </Text>
      <View style={{ width: 14, height: 14, flexShrink: 0 }}>
        <If when={props.selected}>
          <Glyph name="check" size={14} color={paletteToken("accent")} />
        </If>
      </View>
    </View>
  );
}

function applyReaderTheme(id: string): void {
  styleMenuOpen.set(false);
  try {
    const result = bridge.setReaderTheme(id);
    if (result && typeof (result as Promise<void>).then === "function") {
      void (result as Promise<void>)
        .catch((err) => {
          console.warn("failed to save the reader style:", String(err));
        })
        .finally(() => {
          bump();
        });
      return;
    }
  } catch (err) {
    console.warn("failed to save the reader style:", String(err));
  }
  bump();
}

// ---- 正文（body :334-350 + scroll_port :410-437 + file_column :439-504） ----

const readyMarkdown = computed(() => {
  const current = phase();
  return current.kind === "ready" ? current.markdown : "";
});
const failedError = computed(() => {
  const current = phase();
  return current.kind === "failed" ? current.error : "";
});
const isReadyEmpty = computed(
  () => phase().kind === "ready" && readyMarkdown().trim().length === 0,
);
const isReadyContent = computed(
  () => phase().kind === "ready" && readyMarkdown().trim().length > 0,
);

function ReaderBody(): JSX.Element {
  return (
    <View
      style={{ flex: 1, minHeight: 0, width: "100%", minWidth: 0, display: "flex" }}
      testID={bodyReady() ? "skill-md-body" : undefined}
    >
      <View
        style={{
          width: "100%",
          height: "100%",
          padding: 12,
          borderRadius: 8,
          backgroundColor: paletteToken("card"),
          borderWidth: 1,
          borderColor: paletteToken("border"),
        }}
      >
        <Scroll testID="skill-md-reader" style={{ width: "100%", height: "100%" }}>
          <If when={revealed()}>
            <ReaderPhaseBody />
          </If>
        </Scroll>
      </View>
    </View>
  );
}

/** ready 才挂 skill-md-body 选择器（GPUI scroll_port 的 ready 分支）。 */
function bodyReady(): boolean {
  if (!revealed()) return false;
  return phase().kind === "ready";
}

function ReaderPhaseBody(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", width: "100%", minWidth: 0 }}>
      <If when={phase().kind === "loading"}>
        <ReaderLoading />
      </If>
      <If when={phase().kind === "failed"}>
        <FailureCard error={failedError()} />
      </If>
      <If when={isReadyEmpty()}>
        <MutedLine text={rt("detailPanel.emptySkillMd")} />
      </If>
      <If when={isReadyContent()}>
        <FileColumn />
      </If>
    </View>
  );
}

/** 仍在读时晚 300ms 出一条脉冲。快于这个时间保持空白，没有「正在阅读」。 */
function ReaderLoading(): JSX.Element {
  const show = bindDelayedFlag(() => phase().kind === "loading");
  return (
    <If when={show()}>
      <View
        accessibilityState={{ busy: true }}
        style={{ display: "flex", flexDirection: "column", gap: 8, width: "100%" }}
      >
        <Pulse id="skill-md-loading-1" style={{ height: 14, width: "92%", borderRadius: 4 }} />
        <Pulse id="skill-md-loading-2" style={{ height: 14, width: "80%", borderRadius: 4 }} />
        <Pulse id="skill-md-loading-3" style={{ height: 14, width: "64%", borderRadius: 4 }} />
      </View>
    </If>
  );
}

function MutedLine(props: { text: string }): JSX.Element {
  return (
    <Text style={{ fontSize: 14, color: paletteToken("fg_muted") }}>{props.text}</Text>
  );
}

/** 失败卡（failure :352-400）：危险行 + 域错误 + 重试按钮。 */
function FailureCard(props: { error: string }): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      <Text style={{ fontSize: 14, color: paletteToken("danger") }}>
        {rt("detailPanel.readFailed")}
      </Text>
      <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{props.error}</Text>
      <View
        testID="skill-md-retry"
        style={{
          alignSelf: "flex-start",
          paddingHorizontal: 8,
          paddingVertical: 4,
          borderRadius: 6,
          borderWidth: 1,
          borderColor: paletteToken("border"),
          backgroundColor: paletteToken("card"),
          cursor: "pointer",
        }}
        hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
        onPress={retryRead}
      >
        <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg") }}>
          {rt("common.retry")}
        </Text>
      </View>
    </View>
  );
}

/** 文件列（file_column :439-504）。 */
function FileColumn(): JSX.Element {
  return (
    <If
      when={showTranslation()}
      else={<SingleDocument markdown={readyMarkdown()} />}
    >
      <TranslatedColumn />
    </If>
  );
}

/** 非翻译模式：单 TextView；frontmatter 经 options+renderCustom 复刻为
 * ```yaml 栅栏（spike 验证；GPUI 是字符串预处理两段 TextView）。 */
function SingleDocument(props: { markdown: string }): JSX.Element {
  return (
    <TextView
      testID="skill-md-rest"
      markdown={props.markdown}
      styles={readerTypography()}
      options={{ frontmatter: true }}
      renderCustom={(node) => (
        <TextView
          testID="skill-md-front"
          markdown={"```yaml\n" + (node.text ?? "") + "\n```"}
          styles={readerTypography()}
        />
      )}
    />
  );
}

function isBlocked(): boolean {
  void generation();
  return bridge.blocked();
}

/** 翻译模式：逐块 TextView + 译文行（file_column :460-494）。 */
function TranslatedColumn(): JSX.Element {
  return (
    <View
      testID="skill-md-column"
      style={{ display: "flex", flexDirection: "column", gap: 12, width: "100%", minWidth: 0 }}
    >
      {/* blocked 提示优先，其次会话级拒绝行（:447-459）。 */}
      <If when={isBlocked()} else={<RejectionHint />}>
        <MutedLine text={rt("detailPanel.translationNeedsConfig")} />
      </If>
      <Each of={parsedBlocks()} key="index">
        {(block: () => MdBlock) => (
          <If
            when={block().kind === "prose"}
            else={<TextView markdown={block().markdown} styles={readerTypography()} />}
          >
            <View
              style={{
                display: "flex",
                flexDirection: "column",
                gap: 4,
                width: "100%",
                minWidth: 0,
              }}
            >
              <TextView markdown={block().markdown} styles={readerTypography()} />
              <TranslationLine source={() => block().markdown} />
            </View>
          </If>
        )}
      </Each>
    </View>
  );
}

function RejectionHint(): JSX.Element {
  return (
    <If when={rejectionLine() !== null}>
      <MutedLine text={rejectionLine() ?? ""} />
    </If>
  );
}

/** display_when ：161-167 + paint_reader：开着且缓存命中（译文≠原文）才画
 * 译文行，样式取保存的 reader 主题。generation 里重读（译文落地/换样式）。 */
function TranslationLine(props: { source: Accessor<string> }): JSX.Element {
  const lookupState = computed(() => {
    void generation();
    const source = props.source();
    const trimmed = source.trim();
    const translated = bridge.lookup(source) ?? source;
    const hasTranslation = translated !== source;
    const broken = trimmed.length > 0 && failed.has(trimmed) && !hasTranslation;
    return { trimmed, translated, hasTranslation, broken };
  });
  return (
    <If when={lookupState().hasTranslation || lookupState().broken}>
      <View style={{ display: "flex", flexDirection: "column", gap: 4, width: "100%", minWidth: 0 }}>
        <If when={lookupState().hasTranslation}>
          <View
            testID="skill-md-translation"
            style={{ width: "100%", minWidth: 0, ...activeLineTheme().view }}
          >
            <Text style={{ ...translationBase(), ...activeLineTheme().text }}>
              {lookupState().translated}
            </Text>
          </View>
        </If>
        <If when={lookupState().broken}>
          <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
            <Text style={{ fontSize: 12, color: paletteToken("danger") }}>
              {rejectionLine() ?? t("wait.couldntSave")}
            </Text>
            <View testID="skill-md-paragraph-retry" onPress={() => retryProse(lookupState().trimmed)} style={{ cursor: "pointer" }}>
              <Text style={{ fontSize: 12, color: paletteToken("accent_fg") }}>{t("common.retry")}</Text>
            </View>
          </View>
        </If>
      </View>
    </If>
  );
}

function retryProse(source: string): void {
  failed.delete(source);
  // 会话级拒绝会挡住排程。这一段的重试清掉它，其余已失败的段仍留在 failed 里。
  rejectionLine.set(null);
  bump();
  scheduleProse([source]);
}
