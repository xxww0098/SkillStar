import { Scroll, Text, View, type JSX } from "nsh";
import { Input } from "nsh/base";
import { Icon } from "../../chrome/icons";
import { IconSpin } from "../../chrome/feedback";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import {
  adoptFolder,
  cancelActive,
  closeImportModal,
  history,
  onUrlChange,
  onUrlSubmit,
  pickBundleThenImport,
  pickFolderThenAdopt,
  pickHistory,
  progressLabel,
  resetToInput,
  scanWith,
  shareDetected,
  summary,
  error,
  installedCount,
  phase,
  urlValue,
  type Phase,
} from "./importModalState";
import { SelectBody } from "./importModalSelect";
import { SharePreviewBody } from "./importModalShare";
import { PackBody } from "./importModalPack";

// 导入对话框渲染器（一）— import_modal/phases.rs 对位：URL 输入相、Loading
// 相、完成相、失败相，及共享的计数 pill/徽章/按钮。选择相在
// importModalSelect，分享预览在 importModalShare，Quick Pack 在
// importModalPack。<If> 是 nsh 全局组件（globals.d.ts），无需 import。
//
// nsh 纪律：组件体只执行一次，signal 读取全部落在 JSX 表达式（含以函数
// 调用形式挂在 JSX 属性/子节点上的派生读取）；文本省略用
// textOverflow+numberOfLines（无 whiteSpace）。

/** 4 行后最近仓库列表开始滚动（phases.rs RECENT_VISIBLE）。 */
const RECENT_VISIBLE = 4;
/** 滚动盒是明确像素高度视口框（RECENT_VIEW_H：max_h 不产生滚轮溢出）。 */
const RECENT_VIEW_H = 144;
/** 完成态汇总条目超过 12 条后滚动（SUMMARY_VISIBLE）。 */
const SUMMARY_VISIBLE = 12;
/** 汇总滚动盒视口高（SUMMARY_VIEW_H）。 */
const SUMMARY_VIEW_H = 220;

// ---- 共享小件（phases.rs count_pill / badge / chip_group） -------------------

/** 弱化计数 pill（count_pill：well 底 + fg_muted 字）。 */
export function CountPill(props: { label: string }): JSX.Element {
  return (
    <View
      style={{
        flexShrink: 0,
        paddingHorizontal: 8,
        paddingVertical: 2,
        borderRadius: 999,
        backgroundColor: paletteToken("well"),
        borderWidth: 1,
        borderColor: paletteToken("well"),
      }}
    >
      <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg_muted") }}>
        {props.label}
      </Text>
    </View>
  );
}

/** 次级按钮（ghost_button：描边、hover 加深）。 */
export function GhostButton(props: {
  testID: string;
  label: string;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        height: 28,
        paddingHorizontal: 16,
        borderRadius: 6,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        cursor: "pointer",
      }}
      hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
      onPress={props.onPress}
    >
      <Text style={{ fontSize: 12, color: paletteToken("fg") }}>{props.label}</Text>
    </View>
  );
}

/** 主按钮（primary_button：accent 填充）。 */
export function PrimaryButton(props: {
  testID: string;
  label: string;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        height: 28,
        paddingHorizontal: 20,
        borderRadius: 6,
        backgroundColor: paletteToken("accent"),
        cursor: "pointer",
      }}
      hoverStyle={{ backgroundColor: paletteToken("accent_hover") }}
      onPress={props.onPress}
    >
      <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("on_accent") }}>
        {props.label}
      </Text>
    </View>
  );
}

/** 完成态 chips 组（chip_group：弱化标题 + wrap pills）。 */
function ChipGroup(props: { title: string; names: string[]; fg: string; bg: string }): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg_muted") }}>
        {props.title}
      </Text>
      <View style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", gap: 6 }}>
        {props.names.map((name) => (
          <View
            style={{
              paddingHorizontal: 6,
              paddingVertical: 2,
              borderRadius: 6,
              backgroundColor: props.bg,
            }}
          >
            <Text style={{ fontSize: 12, color: props.fg }}>{name}</Text>
          </View>
        ))}
      </View>
    </View>
  );
}

// ---- URL 输入相（phases.rs render_input） -------------------------------------

export function InputBody(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", paddingHorizontal: 24, paddingTop: 16, paddingBottom: 20, gap: 16 }}>
      <View style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        <Text style={{ fontSize: 14, color: paletteToken("fg_muted") }}>
          {t("shareCodeImport.smartInputHint")}
        </Text>
        <If when={shareDetected()}>
          <View
            testID="import-share-detected"
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              gap: 8,
              paddingHorizontal: 12,
              paddingVertical: 8,
              borderRadius: 6,
              borderWidth: 1,
              borderColor: paletteToken("accent") + "33",
              backgroundColor: paletteToken("accent") + "0d",
            }}
          >
            <Icon name="share" size={14} color={paletteToken("accent")} />
            <Text style={{ fontSize: 12, color: paletteToken("accent") }}>
              {t("shareCodeImport.detected")}
            </Text>
          </View>
        </If>
        <UrlField />
        <View style={{ display: "flex", flexDirection: "row", alignItems: "stretch", gap: 8 }}>
          <View style={{ flex: 1, minWidth: 0 }}>
            <ScanButton testID="import-scan" label={t("githubImportModal.scan")} primary onPress={() => onUrlSubmit()} />
          </View>
          <View style={{ flex: 1, minWidth: 0 }}>
            <ScanButton
              testID="import-deep-scan-start"
              label={t("githubImportModal.fullDepthLabel")}
              primary={false}
              onPress={() => scanWith(urlValue().trim(), true, false)}
            />
          </View>
        </View>
      </View>

      {/* — 或 — 本地文件/目录源（一行两个次级动作，系统选择框）。 */}
      <View style={{ display: "flex", flexDirection: "column", gap: 12 }}>
        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 16 }}>
          <View style={{ flex: 1, height: 1, backgroundColor: paletteToken("border_soft") }} />
          <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg_muted") }}>
            {t("common.or")}
          </Text>
          <View style={{ flex: 1, height: 1, backgroundColor: paletteToken("border_soft") }} />
        </View>
        <View style={{ display: "flex", flexDirection: "row", alignItems: "stretch", gap: 8 }}>
          <View style={{ flex: 1, minWidth: 0 }}>
            <SourceButton testID="import-pick-file" label={t("importBundleModal.pickFile")} glyph="package" onPress={() => pickBundleThenImport()} />
          </View>
          <View style={{ flex: 1, minWidth: 0 }}>
            <SourceButton testID="import-pick-folder" label={t("importModal.adoptFolder")} glyph="folder" onPress={() => pickFolderThenAdopt()} />
          </View>
        </View>
      </View>

      <If when={history().length > 0}>
        {() => <RecentList />}
      </If>
    </View>
  );
}

function UrlField(): JSX.Element {
  return (
    <Input
      testID="import-url"
      value={urlValue()}
      placeholder={t("githubImportModal.placeholder")}
      autoFocus
      submitBehavior="submit"
      style={{
        width: "100%",
        height: 32,
        paddingHorizontal: 10,
        borderRadius: 6,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("input"),
        fontSize: 14,
        fontWeight: "400",
        color: paletteToken("fg"),
        placeholderColor: paletteToken("fg_muted"),
      }}
      onChange={(event: { value: string }) => onUrlChange(event.value)}
      onSubmit={() => onUrlSubmit()}
    />
  );
}

function ScanButton(props: {
  testID: string;
  label: string;
  primary: boolean;
  onPress: () => void;
}): JSX.Element {
  const enabled = () => urlValue().trim().length > 0;
  return (
    <View
      testID={props.testID}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        height: 32,
        borderRadius: 6,
        cursor: "pointer",
        opacity: enabled() ? 1 : 0.5,
        backgroundColor: props.primary ? paletteToken("accent") : "transparent",
        borderWidth: 1,
        borderColor: props.primary ? "transparent" : paletteToken("border"),
      }}
      hoverStyle={{
        backgroundColor: enabled()
          ? props.primary
            ? paletteToken("accent_hover")
            : paletteToken("panel_hover")
          : props.primary
            ? paletteToken("accent")
            : "transparent",
      }}
      onPress={() => {
        if (!enabled()) return;
        props.onPress();
      }}
    >
      <Text
        style={{
          fontSize: 12,
          fontWeight: "500",
          color: props.primary ? paletteToken("on_accent") : paletteToken("fg"),
        }}
      >
        {props.label}
      </Text>
    </View>
  );
}

function SourceButton(props: {
  testID: string;
  label: string;
  glyph: "package" | "folder";
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        gap: 6,
        height: 32,
        borderRadius: 6,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        cursor: "pointer",
      }}
      hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
      onPress={props.onPress}
    >
      <Icon name={props.glyph} size={14} color={paletteToken("fg_muted")} />
      <Text style={{ fontSize: 12, color: paletteToken("fg") }}>{props.label}</Text>
    </View>
  );
}

function RecentList(): JSX.Element {
  const entries = () => history();
  const scrollable = () => entries().length > RECENT_VISIBLE;
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg_muted") }}>
        {t("githubImportModal.recentRepos")}
      </Text>
      {(() => {
        // 滚动盒外层是明确像素高度视口框（禁 max_h——波 4 硬契约）。
        if (!scrollable()) return <Rows entries={entries} />;
        return (
          <View testID="import-recent-repos" style={{ height: RECENT_VIEW_H, width: "100%", flexShrink: 0 }}>
            <Scroll testID="import-recent-scroll" style={{ width: "100%", height: "100%" }}>
              <Rows entries={entries} />
            </Scroll>
          </View>
        );
      })()}
    </View>
  );
}

function Rows(props: { entries: () => string[] }): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 2, width: "100%" }}>
      {props.entries().map((source, index) => (
        <View
          testID={`import-history-${index}`}
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 10,
            paddingHorizontal: 12,
            paddingVertical: 8,
            borderRadius: 8,
            cursor: "pointer",
          }}
          hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
          onPress={() => pickHistory(source)}
        >
          <Icon name="rotateCcw" size={14} color={paletteToken("fg_muted")} />
          <Text
            style={{
              fontSize: 14,
              color: paletteToken("fg"),
              minWidth: 0,
              textOverflow: "ellipsis",
              numberOfLines: 1,
            }}
          >
            {source}
          </Text>
        </View>
      ))}
    </View>
  );
}

// ---- Loading 相（phases.rs render_loading） -----------------------------------

export function LoadingBody(): JSX.Element {
  const cancellable = () => phase() === "scanning" || phase() === "installing";
  return (
    <View
      testID="import-loading"
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
          backgroundColor: paletteToken("accent") + "1a",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <IconSpin name="loader" size={24} color={paletteToken("accent")} spin />
      </View>
      <Text testID="import-progress-label" style={{ fontSize: 14, color: paletteToken("fg_muted") }}>
        {progressLabel()}
      </Text>
      <If when={cancellable()}>
        <GhostButton testID="import-cancel" label={t("common.cancel")} onPress={() => cancelActive()} />
      </If>
    </View>
  );
}

// ---- 完成相（phases.rs render_completed） --------------------------------------

export function CompletedBody(): JSX.Element {
  const doneSummary = () => summary();
  const entries = () => {
    const value = doneSummary();
    if (value === null) return 0;
    return value.existing.length + value.installed.length + value.skipped.length;
  };
  return (
    <View
      testID="import-completed"
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
          backgroundColor: paletteToken("ok") + "1a",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <Icon name="checkBig" size={28} color={paletteToken("ok")} />
      </View>
      <View style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 4, width: "100%" }}>
        <Text style={{ fontSize: 16, fontWeight: "600" }}>{t("githubImportModal.titleComplete")}</Text>
        <Text style={{ fontSize: 14, color: paletteToken("fg_muted"), textAlign: "center" }}>
          {(() => {
            const value = doneSummary();
            if (value === null) {
              return tf("githubImportModal.descComplete", { count: installedCount() });
            }
            return tf("shareCodeImport.resultSummary", {
              total: value.requested,
              existing: value.existing.length,
              installed: value.installed.length,
              skipped: value.skipped.length,
            });
          })()}
        </Text>
      </View>
      {(() => {
        const value = doneSummary();
        if (value === null) return null;
        const installedNames = [...value.installed];
        const block = (
          <View style={{ width: "100%", display: "flex", flexDirection: "column", gap: 12 }}>
            {value.existing.length > 0 ? (
              <ChipGroup
                title={tf("shareCodeImport.alreadyHadTitle", { count: value.existing.length })}
                names={value.existing}
                fg={paletteToken("fg")}
                bg={paletteToken("well")}
              />
            ) : null}
            {installedNames.length > 0 ? (
              <ChipGroup
                title={tf("shareCodeImport.installedNowTitle", { count: installedNames.length })}
                names={installedNames}
                fg={paletteToken("ok")}
                bg={paletteToken("ok_bg")}
              />
            ) : null}
            {value.skipped.length > 0 ? <SkippedRows skipped={value.skipped} /> : null}
          </View>
        );
        if (entries() > SUMMARY_VISIBLE) {
          return (
            <View testID="import-summary" style={{ height: SUMMARY_VIEW_H, width: "100%", flexShrink: 0 }}>
              <Scroll testID="import-summary-scroll" style={{ width: "100%", height: "100%" }}>
                {block}
              </Scroll>
            </View>
          );
        }
        return block;
      })()}
      <If when={error() !== null}>
        <Text style={{ width: "100%", fontSize: 12, color: paletteToken("warn") }}>{error() ?? ""}</Text>
      </If>
      <View style={{ display: "flex", gap: 8, marginTop: 8 }}>
        <PrimaryButton
          testID="import-done"
          label={t("githubImportModal.done")}
          onPress={() => closeImportModal()}
        />
      </View>
    </View>
  );
}

/** 未装行（skipped：reason 归一到已知集合，未知落 install_failed——React
 * knownReasons 回退对位）。 */
function SkippedRows(props: { skipped: { name: string; reason: string }[] }): JSX.Element {
  const known = [
    "repo_missing",
    "no_source",
    "install_failed",
    "unsupported_source",
    "embedded_failed",
    "cancelled",
  ];
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg_muted") }}>
        {tf("shareCodeImport.skippedTitle", { count: props.skipped.length })}
      </Text>
      {props.skipped.map((entry) => (
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 8,
            paddingHorizontal: 8,
            paddingVertical: 6,
            borderRadius: 6,
            borderWidth: 1,
            borderColor: paletteToken("warn") + "33",
            backgroundColor: paletteToken("warn") + "0d",
          }}
        >
          <Text
            style={{
              fontSize: 12,
              minWidth: 0,
              textOverflow: "ellipsis",
              numberOfLines: 1,
            }}
          >
            {entry.name}
          </Text>
          <Text style={{ fontSize: 12, flexShrink: 0, color: paletteToken("warn") }}>
            {t(`shareCodeImport.skipReason.${known.includes(entry.reason) ? entry.reason : "install_failed"}`)}
          </Text>
        </View>
      ))}
    </View>
  );
}

// ---- 失败相（phases.rs render_failed） -----------------------------------------

export function FailedBody(): JSX.Element {
  return (
    <View
      testID="import-failed"
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
          backgroundColor: paletteToken("warn") + "1a",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <Icon name="alertTriangle" size={28} color={paletteToken("warn")} />
      </View>
      <View style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 4 }}>
        <Text style={{ fontSize: 16, fontWeight: "600" }}>{t("githubImportModal.somethingWrong")}</Text>
        <Text
          testID="import-failed-message"
          style={{ maxWidth: 320, fontSize: 14, color: paletteToken("fg_muted"), textAlign: "center" }}
        >
          {error() ?? ""}
        </Text>
      </View>
      <View
        testID="import-retry"
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 6,
          marginTop: 4,
          height: 28,
          paddingHorizontal: 12,
          borderRadius: 6,
          cursor: "pointer",
        }}
        hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
        onPress={() => resetToInput()}
      >
        <Icon name="rotateCcw" size={14} color={paletteToken("fg_muted")} />
        <Text style={{ fontSize: 12, color: paletteToken("fg") }}>{t("githubImportModal.tryAgain")}</Text>
      </View>
    </View>
  );
}

// ---- 头部与相分派 --------------------------------------------------------------

/** 头部标题（pack 相无 icon 井——render_header 的 pack 分支）。 */
export function headerTitle(at: Phase): string {
  return at === "pack" ? t("createGroupModal.newGroup") : t("common.import");
}

/** 相分派（render 的 Phase match；各相渲染器见同目录模块）。 */
export function phaseBody(at: Phase): JSX.Element {
  switch (at) {
    case "input":
      return <InputBody />;
    case "scanning":
    case "installing":
    case "shareInstalling":
      return <LoadingBody />;
    case "select":
      return <SelectBody />;
    case "completed":
      return <CompletedBody />;
    case "failed":
      return <FailedBody />;
    case "sharePreview":
      return <SharePreviewBody />;
    case "pack":
      return <PackBody />;
  }
}
