import { Overlay, Text, View, signal, type JSX, type Signal } from "nsh";
import { Input } from "nsh/base";
import { SkillCardsImport } from "skillstar-gui/native";
import { bindSlow } from "../../chrome/delayedFlag";
import { SHEET_SHADOW, promptVeil } from "../../chrome/dialog";
import { IconSpin } from "../../chrome/feedback";
import { Icon, type IconName } from "../../chrome/icons";
import { fadeTransition } from "../../chrome/motion";
import { endJob, putJob } from "../../shell/jobBoard";
import { groupsChanged, requestImportSource, selectPage } from "../../shell/events";
import { isLight, paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";

// 卡组导入对话框 — skill_cards/deck_import.rs 对位。卡组工具栏的一个导入
// 入口：一个智能输入框同时接受 agd-/ags- 分享码（实时识别横幅）与本地
// .ags/.agd 路径；「或」分隔线下是系统文件选择器。四态 chrome（输入 /
// 安装中 / 完成 / 失败），完成态分组展示已存在、新装、未安装与原因。
// 视觉语言与 my_skills 导入弹窗共享：header well、detected banner、
// or-divider、phase wells。
//
// 域经 SkillCardsImport facade（share_install / skill_bundle / skill_group，
// 重活走 blocking 线程）；成功后经 groupsChanged 频道请卡组页刷新——页面
// 互不 import 的唯一跨页通道。
//
// 响应式规则（nsh：组件体只跑一次，只有 JSX 绑定追踪）：状态信号在组件
// 体创建，全部读取都放进 JSX 表达式；相位切换用读取 phase() 的动态三元
// （GPUI 也是整块换相位视图）；识别横幅用 <If>（when 读 value()）。

/// `#rrggbbaa` — the palette carries solid hex tokens; GPUI's `.alpha(n)`
/// wells and banners apply the same token at an opacity.
function withAlpha(hex: string, alpha: number): string {
  const a = Math.round(Math.min(1, Math.max(0, alpha)) * 255)
    .toString(16)
    .padStart(2, "0");
  return `${hex}${a}`;
}

/// Dialog width (deck_import.rs `open_import`: 512 wide, window-centered —
/// flex centering here needs no first-frame measurement).
const DIALOG_W = 512;

type Phase = "input" | "installing" | "completed" | "failed";

/// A share-code member as the facade hands it back (wire field names n/u/c/p).
interface ShareSkill {
  n: string;
  u: string;
  c?: string | null;
  p?: boolean | null;
}

interface ParsedShareCode {
  /// `"skills"` (`ags-`) or `"deck"` (`agd-`), per the facade.
  kind: string;
  name: string;
  description: string;
  icon: string;
  skills: ShareSkill[];
}

interface SkippedEntry {
  name: string;
  reason: string;
}

/// The pre-completed placeholder: the signal never holds null, so the
/// completed branch reads it without a guard (deck_import.rs stages the
/// summary before flipping the phase).
const NO_DONE: ImportDone = {
  flavor: "share",
  requested: 0,
  existing: [],
  installed: [],
  skipped: [],
  nameTaken: false,
};

/// Completed-phase payload, already reduced by the facade so this renderer
/// never re-derives counts (deck_import.rs `ImportDone`).
interface ImportDone {
  flavor: "share" | "bundle";
  requested: number;
  existing: string[];
  installed: string[];
  skipped: SkippedEntry[];
  nameTaken: boolean;
}

// ── Module surface: the deck toolbar opens, the host renders ──────────

interface DeckImportEntry {
  seq: number;
}

const entries = signal<DeckImportEntry[]>([]);
let nextSeq = 1;

/** Open the deck-import dialog (the deck toolbar's import action). */
export function openDeckImport(): void {
  entries.set([...entries(), { seq: nextSeq++ }]);
}

function close(seq: number): void {
  entries.set(entries().filter((entry) => entry.seq !== seq));
}

/**
 * Mount once where the dialog floats (the deck page). Renders the top
 * entry on the shared prompt veil; the dynamic child re-builds only when
 * the entry stack changes, so the page below never re-renders on dialog
 * churn.
 */
export function DeckImportHost(): JSX.Element {
  // Every entry stays mounted, including one parked into the corner job.
  // Rendering only the top entry would unmount that card and drop its UI.
  return (
    <View>
      <Each of={entries()} key={(entry: DeckImportEntry) => String(entry.seq)}>
        {(entry) => <DeckImportCard entry={entry()} />}
      </Each>
    </View>
  );
}

// ── The dialog ────────────────────────────────────────────────────────

function DeckImportCard(props: { entry: DeckImportEntry }): JSX.Element {
  const seq = props.entry.seq;
  const jobId = `deck-import-${seq}`;
  // Per-instance state (the body runs once per opened dialog).
  // on open the card is not parked; background only hides the veil.
  const parked = signal(false);
  const value = signal("");
  const phase = signal<Phase>("input");
  const progress = signal("");
  const error = signal("");
  const done = signal<ImportDone>(NO_DONE);

  // ── Domain actions (read the signals at call time) ────────────

  /// Read the field and route by what it holds: share code → remote repo
  /// source / CLI command (handoff to MySkills import modal) → local bundle
  /// path. A recognized share code that fails decoding surfaces its own
  /// error instead of falling through to file open.
  function submit(): void {
    if (phase() !== "input") return;
    const text = value().trim();
    if (!text) return;
    let parsed: ParsedShareCode | null = null;
    try {
      parsed = SkillCardsImport.parseShareCode(text);
    } catch (reason) {
      if (SkillCardsImport.looksLikeShareCode(text) && !hasBundleExtension(text)) {
        fail(errorText(reason));
        return;
      }
      parsed = null;
    }
    if (parsed) {
      runShare(parsed);
      return;
    }
    if (SkillCardsImport.looksLikeRepoSource(text)) {
      close(seq);
      selectPage.publish("my_skills");
      requestImportSource.publish({ input: text, autoScan: true });
      return;
    }
    runBundle(text);
  }

  function runShare(parsed: ParsedShareCode): void {
    begin(t("skillCards.importRunning"));
    void SkillCardsImport.installShareCode(
      parsed.kind,
      parsed.name,
      parsed.description,
      parsed.icon,
      parsed.skills
    ).then(
      (outcome) => finish({ flavor: "share", ...outcome }),
      (reason) => fail(errorText(reason))
    );
  }

  function runBundle(path: string): void {
    begin(t("skillCards.importRunning"));
    void SkillCardsImport.installBundle(
      path,
      t("importDeckBundleModal.deckBundle"),
      t("importDeckBundleModal.skillsCount")
    ).then(
      (outcome) => finish({ flavor: "bundle", ...outcome }),
      (reason) => fail(errorText(reason))
    );
  }

  /// Native `.ags` / `.agd` picker — the file half of the merged entry.
  /// Cancel and platform-unavailable both resolve null and do nothing.
  function pickFile(): void {
    if (phase() !== "input") return;
    void SkillCardsImport.pickDeckFile(t("skillCards.importPickFile")).then(
      (path) => {
        if (path) runBundle(path);
      }
    );
  }

  function begin(label: string): void {
    phase.set("installing");
    progress.set(label);
    error.set("");
  }

  /// Both import flavors land here: success stages the summary and asks
  /// the deck page to reload; failure keeps the field for a retry.
  function finish(outcome: ImportDone): void {
    done.set(outcome);
    phase.set("completed");
    groupsChanged.publish();
    if (parked()) {
      endJob(jobId);
      close(seq);
    }
  }

  function fail(message: string): void {
    error.set(message);
    phase.set("failed");
    if (parked()) {
      endJob(jobId);
      parked.set(false);
    }
  }

  // The import promise cannot be cancelled. Parking hides this card's veil
  // and leaves the component mounted so the result can still land.
  function park(): void {
    if (parked()) return;
    parked.set(true);
    putJob({ id: jobId, title: t("wait.importing") });
  }

  function retry(): void {
    phase.set("input");
    error.set("");
  }

  return (
    // Parked: the veil unmounts, this component does not. close(seq) is not
    // the background action — that would drop the card before the promise ends.
    <If when={!parked()}>
    <Overlay
      style={{
        position: "fixed",
        top: 0,
        left: 0,
        right: 0,
        bottom: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
      onCancel={() => close(seq)}
    >
      {/* Veil: the kit Dialog's overlay (theme.rs `overlay: prompt_veil()`),
          a translucent wash the page stays visible through. */}
      <View
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          right: 0,
          bottom: 0,
          backgroundColor: promptVeil(),
        }}
        onPress={() => close(seq)}
      />
      {/* Custom self-buttoned surfaces center in the window (chrome
          open_centered parity); the entrance is the FadeSlide the kit
          gives every floating surface. `.modal-surface`: card on paper,
          panel on the dark canvas. */}
      <View
        testID="deck-import-card"
        style={{
          display: "flex",
          flexDirection: "column",
          width: DIALOG_W,
          maxWidth: "100%",
          borderRadius: 12,
          borderWidth: 1,
          borderColor: paletteToken("border"),
          backgroundColor: isLight() ? paletteToken("card") : paletteToken("panel"),
          boxShadow: SHEET_SHADOW,
          transform: "translateY(0)",
          ...fadeTransition(true),
        }}
        onPress={() => {
          // The card swallows the press so the veil does not close it.
        }}
      >
        <Header />
        <If when={phase() === "input"}>
          <InputPhase value={value} onSubmit={submit} onPickFile={pickFile} />
        </If>
        <If when={phase() === "installing"}>
          <LoadingPhase label={progress()} onBackground={park} />
        </If>
        <If when={phase() === "completed"}>
          <CompletedPhase done={done()} onDone={() => close(seq)} />
        </If>
        <If when={phase() === "failed"}>
          <FailedPhase message={error()} onRetry={retry} />
        </If>
      </View>
    </Overlay>
    </If>
  );
}

/// Same header row as the my-skills import dialog: 32px accent well,
/// Download glyph, deck-scoped title.
function Header(): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        width: "100%",
        paddingHorizontal: 24,
        paddingTop: 16,
        paddingBottom: 12,
        flexShrink: 0,
        borderBottomWidth: 1,
        borderBottomColor: paletteToken("border_soft"),
      }}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 10,
        }}
      >
        <View
          style={{
            width: 32,
            height: 32,
            borderRadius: 12,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            backgroundColor: withAlpha(paletteToken("accent"), 0.1),
          }}
        >
          <Icon name="download" size={16} color={paletteToken("accent")} />
        </View>
        <Text
          style={{ fontSize: 16, fontWeight: "600", color: paletteToken("fg") }}
        >
          {t("skillCards.importTitle")}
        </Text>
      </View>
    </View>
  );
}

// ── Phase bodies ──────────────────────────────────────────────────────

/// Source step: smart field with live share-code detection, the primary
/// import action, then the or-divider and the native file picker.
function InputPhase(props: {
  value: Signal<string>;
  onSubmit: () => void;
  onPickFile: () => void;
}): JSX.Element {
  const value = props.value;
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        paddingHorizontal: 24,
        paddingTop: 16,
        paddingBottom: 20,
        gap: 16,
      }}
    >
      <View style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        <Text style={{ fontSize: 14, color: paletteToken("fg_muted") }}>
          {t("skillCards.importHint")}
        </Text>
        {/* Any edit can flip the detected banner (the facade sniffs share
            codes and remote repo sources per Change). */}
        <If when={SkillCardsImport.looksLikeShareCode(value())}>
          <View testID="deck-import-detected" style={detectedBannerStyle()}>
            <Icon name="share" size={14} color={paletteToken("accent")} />
            <Text style={{ fontSize: 12, color: paletteToken("accent") }}>
              {t("skillCards.shareCodeDetected")}
            </Text>
          </View>
        </If>
        <If
          when={
            !SkillCardsImport.looksLikeShareCode(value()) &&
            SkillCardsImport.looksLikeRepoSource(value())
          }
        >
          <View testID="deck-import-repo-detected" style={detectedBannerStyle()}>
            <Icon name="gitBranch" size={14} color={paletteToken("accent")} />
            <Text style={{ fontSize: 12, color: paletteToken("accent") }}>
              {t("skillCards.repoSourceDetected")}
            </Text>
          </View>
        </If>
        {/* The smart field: clipboard prefix; code or path in one input. */}
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 8,
            width: "100%",
            height: 36,
            paddingHorizontal: 10,
            borderRadius: 8,
            borderWidth: 1,
            borderColor: paletteToken("border"),
            backgroundColor: paletteToken("input"),
          }}
        >
          <Icon name="clipboard" size={14} color={paletteToken("fg_muted")} />
          <Input
            testID="deck-import-input"
            style={{
              flex: 1,
              minWidth: 0,
              fontSize: 14,
              color: paletteToken("fg"),
            }}
            value={value()}
            placeholder={t("skillCards.importPlaceholder")}
            submitBehavior="submit"
            autoFocus
            onChange={(e) => props.value.set(e.value)}
            onSubmit={() => props.onSubmit()}
          />
        </View>
        <PrimaryAction
          id="deck-import-submit"
          glyph="download"
          label={t("common.import")}
          disabled={value().trim().length === 0}
          onPress={props.onSubmit}
        />
      </View>

      <View style={{ display: "flex", flexDirection: "column", gap: 12 }}>
        {/* or-divider */}
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 16,
          }}
        >
          <View
            style={{
              flex: 1,
              height: 1,
              backgroundColor: paletteToken("border_soft"),
            }}
          />
          <Text
            style={{
              fontSize: 12,
              fontWeight: "500",
              color: paletteToken("fg_muted"),
            }}
          >
            {t("common.or")}
          </Text>
          <View
            style={{
              flex: 1,
              height: 1,
              backgroundColor: paletteToken("border_soft"),
            }}
          />
        </View>
        <View testID="deck-import-file-row" style={{ flex: 1, minWidth: 0 }}>
          <OutlineAction
            id="deck-import-pick-file"
            glyph="package"
            label={t("skillCards.importPickFile")}
            onPress={props.onPickFile}
          />
        </View>
      </View>
    </View>
  );
}

/// Spinner well + message. The import is not cancellable (no facade
/// session). After 10s the card may move to the corner job.
function LoadingPhase(props: { label: string; onBackground: () => void }): JSX.Element {
  const slow = bindSlow(() => true);
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        paddingVertical: 64,
        gap: 16,
      }}
    >
      <View
        style={{
          width: 48,
          height: 48,
          borderRadius: 16,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          backgroundColor: withAlpha(paletteToken("accent"), 0.1),
        }}
      >
        <IconSpin name="loader" size={24} color={paletteToken("accent")} spin />
      </View>
      <Text style={{ fontSize: 14, color: paletteToken("fg_muted") }}>
        {props.label}
      </Text>
      <If when={slow()}>
        <View
          testID="deck-import-background"
          onPress={props.onBackground}
          style={{ cursor: "pointer", paddingHorizontal: 12, paddingVertical: 6 }}
        >
          <Text style={{ fontSize: 12, color: paletteToken("accent") }}>
            {t("wait.continueInBackground")}
          </Text>
        </View>
      </If>
    </View>
  );
}

/// Success well + summary line + result chips. Skipped skills keep their
/// per-reason rows.
function CompletedPhase(props: { done: ImportDone; onDone: () => void }): JSX.Element {
  const done = props.done;
  const line =
    done.flavor === "share"
      ? tf("shareCodeImport.resultSummary", {
          total: done.requested,
          existing: done.existing.length,
          installed: done.installed.length,
          skipped: done.skipped.length,
        })
      : tf("skillCards.importDoneDesc", { count: done.installed.length });
  return (
    <View
      testID="deck-import-completed"
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        paddingVertical: 40,
        paddingHorizontal: 24,
        gap: 16,
      }}
    >
      <View
        style={{
          width: 56,
          height: 56,
          borderRadius: 16,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          backgroundColor: withAlpha(paletteToken("ok"), 0.1),
        }}
      >
        <Icon name="checkBig" size={28} color={paletteToken("ok")} />
      </View>
      <View
        style={{
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: 4,
        }}
      >
        <Text
          style={{ fontSize: 16, fontWeight: "600", color: paletteToken("fg") }}
        >
          {t("skillCards.importDone")}
        </Text>
        <Text
          style={{
            fontSize: 14,
            color: paletteToken("fg_muted"),
            textAlign: "center",
          }}
        >
          {line}
        </Text>
      </View>

      <If when={done.existing.length > 0}>
        <ChipGroup
          title={tf("shareCodeImport.alreadyHadTitle", { count: done.existing.length })}
          names={done.existing}
          fg={paletteToken("fg")}
          bg={paletteToken("well")}
        />
      </If>
      <If when={done.installed.length > 0}>
        <ChipGroup
          title={tf("shareCodeImport.installedNowTitle", {
            count: done.installed.length,
          })}
          names={done.installed}
          fg={paletteToken("ok")}
          bg={paletteToken("ok_bg")}
        />
      </If>
      <If when={done.skipped.length > 0}>
        <View
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 4,
            width: "100%",
          }}
        >
          <Text
            style={{
              fontSize: 12,
              fontWeight: "500",
              color: paletteToken("fg_muted"),
            }}
          >
            {tf("shareCodeImport.skippedTitle", { count: done.skipped.length })}
          </Text>
          <Each of={done.skipped} key={(entry) => entry.name}>
            {(entry: () => ImportDone["skipped"][number]) => <SkippedRow entry={entry()} />}
          </Each>
        </View>
      </If>
      <If when={done.nameTaken}>
        <Text style={{ width: "100%", fontSize: 12, color: paletteToken("warn") }}>
          {t("skillCards.importNameTaken")}
        </Text>
      </If>

      <View style={{ display: "flex", flexDirection: "row", gap: 8, marginTop: 8 }}>
        <PrimaryAction
          id="deck-import-done"
          glyph="check"
          label={t("common.done")}
          onPress={props.onDone}
        />
      </View>
    </View>
  );
}

/// Known skip-reason slugs; anything else reads as a failed install
/// (deck_import.rs `skipped_row`'s match with its `install_failed` arm).
const KNOWN_REASONS = [
  "repo_missing",
  "no_source",
  "install_failed",
  "unsupported_source",
  "embedded_failed",
  "cancelled",
];

/// One skipped skill: name + localized reason chip.
function SkippedRow(props: { entry: SkippedEntry }): JSX.Element {
  const warn = paletteToken("warn");
  const reason = KNOWN_REASONS.includes(props.entry.reason)
    ? props.entry.reason
    : "install_failed";
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 8,
        borderRadius: 6,
        borderWidth: 1,
        borderColor: withAlpha(warn, 0.2),
        backgroundColor: withAlpha(warn, 0.05),
        paddingHorizontal: 8,
        paddingVertical: 6,
      }}
    >
      <Text
        style={{
          fontSize: 12,
          color: paletteToken("fg"),
          flex: 1,
          minWidth: 0,
          overflow: "hidden",
        }}
      >
        {props.entry.name}
      </Text>
      <Text style={{ fontSize: 12, flexShrink: 0, color: warn }}>
        {t(`shareCodeImport.skipReason.${reason}`)}
      </Text>
    </View>
  );
}

/// Amber well + message + Try again (back to the field, text kept).
function FailedPhase(props: { message: string; onRetry: () => void }): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        paddingVertical: 56,
        paddingHorizontal: 24,
        gap: 16,
      }}
    >
      <View
        style={{
          width: 56,
          height: 56,
          borderRadius: 16,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          backgroundColor: withAlpha(paletteToken("warn"), 0.1),
        }}
      >
        <Icon name="alertTriangle" size={28} color={paletteToken("warn")} />
      </View>
      <View
        style={{
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: 4,
        }}
      >
        <Text
          style={{ fontSize: 16, fontWeight: "600", color: paletteToken("fg") }}
        >
          {t("skillCards.importFailed")}
        </Text>
        <Text
          style={{
            maxWidth: 320,
            fontSize: 14,
            color: paletteToken("fg_muted"),
            textAlign: "center",
          }}
        >
          {props.message}
        </Text>
      </View>
      <View
        testID="deck-import-retry"
        style={{
          marginTop: 4,
          height: 28,
          paddingHorizontal: 12,
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 6,
          borderRadius: 6,
          cursor: "pointer",
        }}
        hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
        onPress={props.onRetry}
      >
        <Icon name="rotateCcw" size={14} color={paletteToken("fg_muted")} />
        <Text style={{ fontSize: 12, color: paletteToken("fg") }}>
          {t("common.retry")}
        </Text>
      </View>
    </View>
  );
}

// ── Shared bits ───────────────────────────────────────────────────────

/// Label + wrapped name pills, the same shape as the my-skills summary.
function ChipGroup(props: {
  title: string;
  names: string[];
  fg: string;
  bg: string;
}): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 6, width: "100%" }}>
      <Text
        style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg_muted") }}
      >
        {props.title}
      </Text>
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          flexWrap: "wrap",
          gap: 6,
        }}
      >
        <Each of={props.names} key={(name) => name}>
          {(name: () => string) => (
            <View
              style={{
                paddingHorizontal: 6,
                paddingVertical: 2,
                borderRadius: 6,
                backgroundColor: props.bg,
              }}
            >
              <Text style={{ fontSize: 12, color: props.fg }}>{name()}</Text>
            </View>
          )}
        </Each>
      </View>
    </View>
  );
}

/// Accent-filled w-full action (the kit primary button's shape; disabled
/// reads the prop in the JSX so the binding keeps following it).
function PrimaryAction(props: {
  id: string;
  glyph: IconName;
  label: string;
  disabled?: boolean;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.id}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "center",
        gap: 6,
        width: "100%",
        height: 36,
        borderRadius: 8,
        cursor: props.disabled ? "default" : "pointer",
        backgroundColor: paletteToken("accent"),
      }}
      disabled={props.disabled}
      disabledStyle={{ backgroundColor: paletteToken("accent_soft") }}
      hoverStyle={
        props.disabled ? undefined : { backgroundColor: paletteToken("accent_hover") }
      }
      onPress={props.disabled ? undefined : props.onPress}
    >
      <Icon name={props.glyph} size={14} color={paletteToken("on_accent")} />
      <Text
        style={{ fontSize: 12, fontWeight: "500", color: paletteToken("on_accent") }}
      >
        {props.label}
      </Text>
    </View>
  );
}

/// Bordered w-full action (the kit outline button's shape).
function OutlineAction(props: {
  id: string;
  glyph: IconName;
  label: string;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.id}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "center",
        gap: 6,
        width: "100%",
        height: 36,
        borderRadius: 8,
        cursor: "pointer",
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
      }}
      hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
      onPress={props.onPress}
    >
      <Icon name={props.glyph} size={14} color={paletteToken("fg")} />
      <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg") }}>
        {props.label}
      </Text>
    </View>
  );
}

function detectedBannerStyle() {
  return {
    display: "flex" as const,
    flexDirection: "row" as const,
    alignItems: "center" as const,
    gap: 8,
    paddingHorizontal: 12,
    paddingVertical: 8,
    borderRadius: 6,
    borderWidth: 1,
    borderColor: withAlpha(paletteToken("accent"), 0.2),
    backgroundColor: withAlpha(paletteToken("accent"), 0.05),
  };
}

function hasBundleExtension(text: string): boolean {
  const clean = text.trim().replace(/^["']|["']$/g, "").toLowerCase();
  return clean.endsWith(".ags") || clean.endsWith(".agd");
}

/// Facade rejections arrive as Error objects carrying the domain message.
function errorText(reason: unknown): string {
  if (reason instanceof Error) return reason.message;
  return String(reason);
}
