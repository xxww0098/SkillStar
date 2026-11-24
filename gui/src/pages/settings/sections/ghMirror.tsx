// 「GitHub 加速」分区 — 可作为独立卡片（MirrorEmbedded）嵌入 NetworkSection，
// 同时保留 GhMirrorWorkbench 供 ghMirror.test.tsx 独立运行。

import { Text, View, computed, effect, type JSX } from "nsh";
import { Input } from "nsh/base";
import { paletteToken } from "../../../services/themeMode";
import { t } from "../../../services/translation";
import { Icon } from "../../../chrome/icons";
import { IconSpin } from "../../../chrome/feedback";
import {
  CollapseChevron,
  MONO_FONT,
  MetaChip,
  SectionShell,
  SettingsSwitch,
  StatusPill,
  card,
} from "../parts";
import { st } from "../settingsStrings";
import { GH_MIRROR_SECTION } from "../sections";
import {
  CUSTOM_ENTRY_ID,
  createGhMirrorState,
  realGhMirrorBridge,
  type GhMirrorBridge,
  type GhMirrorEntry,
  type GhMirrorState,
  type MirrorConfigDto,
  type MirrorPresetDto,
} from "./ghMirrorState";

export {
  CUSTOM_ENTRY_ID,
  createGhMirrorState,
  realGhMirrorBridge,
  type GhMirrorBridge,
  type GhMirrorEntry,
  type GhMirrorState,
  type MirrorConfigDto,
  type MirrorPresetDto,
};

/** 页面模块级单例（keep-alive 只挂载一次）；测试装置自建 state 注入。 */
const mirrorState = createGhMirrorState(realGhMirrorBridge, { deferLoad: true });

/** 嵌入 NetworkSection 的独立 GitHub 加速卡片 + 可展开镜像源列表。 */
export function MirrorEmbedded(): JSX.Element {
  effect(() => mirrorState.start());
  return (
    <View style={card({ display: "flex", flexDirection: "column" })}>
      <View
        style={{
          width: "100%",
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
        }}
      >
        <View
          testID="gh-mirror-collapse"
          role="button"
          style={{
            flex: 1,
            minWidth: 0,
            paddingLeft: 18,
            paddingRight: 12,
            paddingVertical: 15,
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 12,
            cursor: "pointer",
          }}
          hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
          onPress={() => mirrorState.setExpanded(!mirrorState.expanded())}
        >
          <View
            style={{
              flex: 1,
              minWidth: 0,
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              gap: 12,
            }}
          >
            <View
              style={{
                width: 32,
                height: 32,
                borderRadius: 9,
                flexShrink: 0,
                borderWidth: 1,
                borderColor: paletteToken("amber_border"),
                backgroundColor: paletteToken("amber_bg"),
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
              }}
            >
              <Icon name="zap" size={15} color={paletteToken("amber")} />
            </View>
            <View style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 2 }}>
              <View
                style={{
                  display: "flex",
                  flexDirection: "row",
                  alignItems: "center",
                  gap: 8,
                  minWidth: 0,
                }}
              >
                <Text style={{ fontSize: 14, fontWeight: "500", color: paletteToken("fg") }}>
                  {t("settings.githubMirror")}
                </Text>
                <If when={mirrorState.config().enabled && mirrorState.effectiveUrl() !== ""}>
                  {() => <MetaChip text={mirrorState.effectiveUrl()} mono tone="amber" />}
                </If>
              </View>
              <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), lineHeight: 18 }}>
                {t("settings.mirrorHint")}
              </Text>
            </View>
          </View>
          <CollapseChevron open={mirrorState.expanded()} />
        </View>
        <View style={{ paddingRight: 18, flexShrink: 0 }}>
          <SettingsSwitch
            id="gh-mirror-enabled"
            checked={mirrorState.config().enabled}
            onToggle={() => mirrorState.toggle()}
          />
        </View>
      </View>
      <If when={mirrorState.expanded()}>
        {() => (
          <View style={{ opacity: mirrorState.config().enabled ? 1 : 0.8, width: "100%" }}>
            <MirrorForm state={mirrorState} />
          </View>
        )}
      </If>
    </View>
  );
}

/** 独立工作台（供 ghMirror.test.tsx 独立运行）。 */
export function GhMirrorWorkbench(props: { state: GhMirrorState }): JSX.Element {
  const state = props.state;
  // 构造函数 load 对位：挂载读盘一次。
  effect(() => state.start());
  return (
    <SectionShell
      section={GH_MIRROR_SECTION}
      meta={
        <If when={state.config().enabled && state.effectiveUrl() !== ""}>
          <MetaChip text={state.effectiveUrl()} mono tone="amber" />
        </If>
      }
      action={
        <SettingsSwitch
          id="gh-mirror-enabled"
          checked={state.config().enabled}
          onToggle={() => state.toggle()}
        />
      }
      bodyStyle={{ opacity: state.config().enabled ? 1 : 0.7 }}
    >
      <MirrorCard state={state} />
    </SectionShell>
  );
}

/** 折叠卡（render_github_mirror 的 body）：折叠头 + 展开时的表单。 */
function MirrorCard(props: { state: GhMirrorState }): JSX.Element {
  const state = props.state;
  return (
    <View style={{ width: "100%", display: "flex", flexDirection: "column" }}>
      <View
        testID="gh-mirror-collapse"
        role="button"
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "space-between",
          width: "100%",
          paddingHorizontal: 18,
          paddingVertical: 14,
          cursor: "pointer",
        }}
        hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
        onPress={() => state.setExpanded(!state.expanded())}
      >
        <Text style={{ fontSize: 14, fontWeight: "500", color: paletteToken("fg") }}>
          {t("settings.githubMirrorConfig")}
        </Text>
        <CollapseChevron open={state.expanded()} />
      </View>
      <If when={state.expanded()}>{() => <MirrorForm state={state} />}</If>
    </View>
  );
}

/** gh_mirror_form：info 提示条 + 有序源行清单 + 保存状态行。 */
function MirrorForm(props: { state: GhMirrorState }): JSX.Element {
  const state = props.state;
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 8,
        width: "100%",
        paddingHorizontal: 18,
        paddingTop: 14,
        paddingBottom: 16,
        borderTopWidth: 1,
        borderTopColor: paletteToken("border_soft"),
        backgroundColor: paletteToken("well"),
      }}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "flex-start",
          gap: 8,
          paddingHorizontal: 12,
          paddingVertical: 10,
          borderRadius: 10,
          borderWidth: 1,
          borderColor: paletteToken("info_border"),
          backgroundColor: paletteToken("info_bg"),
          marginBottom: 2,
          width: "100%",
        }}
      >
        <View style={{ paddingTop: 2, flexShrink: 0 }}>
          <Icon name="info" size={14} color={paletteToken("info")} />
        </View>
        <Text
          style={{
            flex: 1,
            minWidth: 0,
            fontSize: 12,
            color: paletteToken("fg"),
            lineHeight: 18,
          }}
        >
          {t("settings.githubMirrorNotice")}
        </Text>
      </View>
      {(() => {
        const entries = computed(() => state.entries());
        return (
          <Each of={entries()} key={(entry) => entry.id}>
            {(entry, index) => (
              <MirrorEntryRow
                state={state}
                entry={entry()}
                index={index()}
                count={entries().length}
              />
            )}
          </Each>
        );
      })()}
      <If when={state.status() !== null}>
        {() => <MirrorStatusRow state={state} />}
      </If>
    </View>
  );
}

/** gh_entry_row：radio 圆点 + 优先级序号 + 源名称与 URL + 排序箭头 + 测速胶囊。 */
function MirrorEntryRow(props: {
  state: GhMirrorState;
  entry: GhMirrorEntry;
  index: number;
  count: number;
}): JSX.Element {
  const state = props.state;
  const isSelected = computed(() => state.selectedId() === props.entry.id);
  const isPrimary = computed(() => props.index === 0);
  return (
    <View
      testID={`gh-entry-${props.entry.id}`}
      role="radio"
      accessibilityState={{ selected: isSelected() }}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 10,
        width: "100%",
        paddingHorizontal: 12,
        paddingVertical: 10,
        borderRadius: 10,
        borderWidth: 1,
        cursor: "pointer",
        borderColor: isSelected() ? paletteToken("accent_soft_edge") : paletteToken("border_soft"),
        backgroundColor: isSelected() ? paletteToken("card_active") : paletteToken("card"),
      }}
      hoverStyle={
        isSelected() ? undefined : { backgroundColor: paletteToken("panel_hover") }
      }
      onPress={() => state.selectEntry(props.entry.id)}
    >
      <RadioDot selected={isSelected()} />
      <View
        style={{
          minWidth: 28,
          height: 22,
          paddingHorizontal: 6,
          borderRadius: 6,
          borderWidth: 1,
          borderColor: isPrimary() ? paletteToken("accent_soft_edge") : paletteToken("border_soft"),
          backgroundColor: isPrimary() ? paletteToken("accent_soft") : paletteToken("well"),
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          flexShrink: 0,
        }}
      >
        <Text
          style={{
            fontSize: 11,
            fontWeight: "600",
            fontFamily: MONO_FONT,
            fontVariant: ["tabular-nums"],
            color: isPrimary() ? paletteToken("accent_fg") : paletteToken("fg_muted"),
          }}
        >
          {`#${props.index + 1}`}
        </Text>
      </View>
      <EntryLabel
        state={state}
        entry={props.entry}
        selected={isSelected()}
        primary={isPrimary()}
      />
      <ReorderButtons
        entryId={props.entry.id}
        index={props.index}
        count={props.count}
        onMove={(delta) => state.moveEntry(props.entry.id, delta)}
      />
      {/* custom 槽未选中时没有可探活的 URL，不显示测试按钮（GPUI 同款）。 */}
      <If when={!props.entry.custom || isSelected()}>
        {() => <MirrorTestButton state={state} entryId={props.entry.id} url={props.entry.url} />}
      </If>
    </View>
  );
}

/** radio_dot：选中实心 accent + 内点；未选中空心。 */
function RadioDot(props: { selected: boolean }): JSX.Element {
  return (
    <View
      style={{
        width: 14,
        height: 14,
        flexShrink: 0,
        borderRadius: 7,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        ...(props.selected
          ? { backgroundColor: paletteToken("accent") }
          : { borderWidth: 1, borderColor: paletteToken("fg_muted") }),
      }}
    >
      <If when={props.selected}>
        {() => (
          <View
            style={{
              width: 6,
              height: 6,
              borderRadius: 3,
              backgroundColor: paletteToken("on_accent"),
            }}
          />
        )}
      </If>
    </View>
  );
}

/** gh_entry_label：源名 + 首选标签 + MONO_FONT URL（custom 且选中时是受控输入框）。 */
function EntryLabel(props: {
  state: GhMirrorState;
  entry: GhMirrorEntry;
  selected: boolean;
  primary: boolean;
}): JSX.Element {
  const state = props.state;
  const entry = props.entry;
  return (
    <View
      style={{
        flex: 1,
        minWidth: 0,
        display: "flex",
        flexDirection: "column",
        gap: 3,
      }}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 8,
          minWidth: 0,
        }}
      >
        <Text
          style={{
            fontSize: 13,
            fontWeight: "500",
            color: paletteToken("fg"),
            numberOfLines: 1,
          }}
        >
          {entry.name}
        </Text>
        <If when={props.primary}>
          {() => <StatusPill tone="accent" text={st("settings.mirrorPriorityPrimary")} />}
        </If>
      </View>
      <If when={entry.custom && props.selected}>
        {() => (
          <View
            style={{
              width: "100%",
              height: 36,
              marginTop: 3,
              paddingHorizontal: 10,
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              borderRadius: 10,
              borderWidth: 1,
              borderColor: paletteToken("border"),
              backgroundColor: paletteToken("input"),
            }}
          >
            <Input
              testID={`gh-url-${entry.id}`}
              style={{
                flex: 1,
                minWidth: 0,
                fontSize: 13,
                fontFamily: MONO_FONT,
                color: paletteToken("fg"),
              }}
              value={state.customUrlForm()}
              placeholder="https://"
              submitBehavior="blurAndSubmit"
              onChange={(event) => state.editCustomUrl(event.value)}
              onBlur={() => state.save()}
              onSubmit={() => state.save()}
            />
          </View>
        )}
      </If>
      <If when={!(entry.custom && props.selected) && entry.url !== ""}>
        {() => (
          <Text
            style={{
              fontSize: 12,
              fontFamily: MONO_FONT,
              color: paletteToken("fg_muted"),
              numberOfLines: 1,
            }}
          >
            {entry.url}
          </Text>
        )}
      </If>
    </View>
  );
}

/** reorder_buttons：并排紧凑上/下排序钮；边界位禁用（opacity 0.3、不响应）。 */
function ReorderButtons(props: {
  entryId: string;
  index: number;
  count: number;
  onMove: (delta: -1 | 1) => void;
}): JSX.Element {
  const moves: { delta: -1 | 1; icon: "arrowUp" | "arrowDown"; suffix: "up" | "down" }[] = [
    { delta: -1, icon: "arrowUp", suffix: "up" },
    { delta: 1, icon: "arrowDown", suffix: "down" },
  ];
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 4,
        flexShrink: 0,
      }}
    >
      {moves.map((move) => {
        const disabled = () =>
          (move.delta < 0 && props.index === 0) ||
          (move.delta > 0 && props.index + 1 === props.count);
        return (
          <View
            testID={`gh-${move.suffix}-${props.entryId}`}
            role="button"
            accessibilityState={{ disabled: disabled() }}
            style={{
              width: 26,
              height: 26,
              borderRadius: 7,
              borderWidth: 1,
              borderColor: paletteToken("border_soft"),
              backgroundColor: paletteToken("well"),
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              opacity: disabled() ? 0.3 : 1,
              ...(disabled() ? {} : { cursor: "pointer" }),
            }}
            hoverStyle={disabled() ? undefined : { backgroundColor: paletteToken("panel_hover") }}
            onPress={() => {
              if (!disabled()) props.onMove(move.delta);
            }}
          >
            <Icon name={move.icon} size={12} color={paletteToken("fg_muted")} />
          </View>
        );
      })}
    </View>
  );
}

/** gh_test_button 的 (bg, color, iconColor, label, border, mono) 分派，在 JSX 求值点调用。 */
function probeTone(state: GhMirrorState, entryId: string): {
  bg: string;
  color: string;
  iconColor: string;
  label: string;
  border: string;
  mono: boolean;
  testing: boolean;
  result: number | "fail" | undefined;
} {
  const testing = state.testingId() === entryId;
  const result = state.testResults()[entryId];
  if (testing) {
    return {
      bg: paletteToken("well"),
      color: paletteToken("fg_muted"),
      iconColor: paletteToken("fg_muted"),
      label: "…",
      border: paletteToken("border"),
      mono: true,
      testing,
      result,
    };
  }
  if (typeof result === "number") {
    return {
      bg: paletteToken("ok_bg"),
      color: paletteToken("ok"),
      iconColor: paletteToken("ok"),
      label: `${result}ms`,
      border: paletteToken("ok_border"),
      mono: true,
      testing,
      result,
    };
  }
  if (result === "fail") {
    return {
      bg: paletteToken("danger_bg"),
      color: paletteToken("danger_fg"),
      iconColor: paletteToken("danger"),
      label: t("settings.mirrorTestFail"),
      border: paletteToken("danger_border"),
      mono: false,
      testing,
      result,
    };
  }
  return {
    bg: paletteToken("card"),
    color: paletteToken("fg_muted"),
    iconColor: paletteToken("fg_muted"),
    label: t("settings.mirrorTest"),
    border: paletteToken("border"),
    mono: false,
    testing,
    result,
  };
}

/** can_test 判定（!testing && url 非空）；在 JSX 求值点或按压时调用。 */
function probeAvailable(state: GhMirrorState, entryId: string, url: string): boolean {
  return state.testingId() !== entryId && url !== "";
}

/** gh_test_button：带边框状态胶囊测速按钮（… / ms / mirrorTestFail / mirrorTest）。 */
function MirrorTestButton(props: {
  state: GhMirrorState;
  entryId: string;
  url: string;
}): JSX.Element {
  const state = props.state;
  const tone = computed(() => probeTone(state, props.entryId));
  const canTest = computed(() => probeAvailable(state, props.entryId, props.url));
  return (
    <View
      testID={`gh-test-${props.entryId}`}
      role="button"
      accessibilityState={{
        disabled: !canTest(),
        busy: tone().testing,
      }}
      style={{
        height: 26,
        flexShrink: 0,
        paddingHorizontal: 10,
        borderRadius: 999,
        borderWidth: 1,
        borderColor: tone().border,
        backgroundColor: tone().bg,
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 5,
        opacity: canTest() ? 1 : 0.6,
        ...(canTest() ? { cursor: "pointer" } : {}),
      }}
      hoverStyle={canTest() ? { backgroundColor: paletteToken("panel_hover") } : undefined}
      onPress={() => {
        if (canTest()) {
          state.runTest(props.entryId, props.url);
        }
      }}
    >
      <If when={typeof tone().result === "number"}>
        {() => <Icon name="wifi" size={12} color={tone().iconColor} />}
      </If>
      <If when={tone().result === "fail" && !tone().testing}>
        {() => <Icon name="wifiOff" size={12} color={tone().iconColor} />}
      </If>
      <View
        style={{
          width: tone().testing ? 12 : 0,
          height: tone().testing ? 12 : 0,
          overflow: "hidden",
          flexShrink: 0,
          ...(tone().testing ? {} : { position: "absolute" }),
        }}
      >
        <IconSpin
          name="loader"
          size={12}
          color={tone().iconColor}
          spin={tone().testing}
        />
      </View>
      <Text
        style={{
          fontSize: 11,
          fontWeight: "600",
          ...(tone().mono
            ? { fontFamily: MONO_FONT, fontVariant: ["tabular-nums"] }
            : {}),
          color: tone().color,
        }}
      >
        {tone().label}
      </Text>
    </View>
  );
}

/** 保存状态行：成功 ok 色 + CheckCircle 图标，失败 danger（右对齐）。 */
function MirrorStatusRow(props: { state: GhMirrorState }): JSX.Element {
  const state = props.state;
  const isOk = () => state.status() === t("common.saved");
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        justifyContent: "flex-end",
        alignItems: "center",
        gap: 5,
        minHeight: 18,
      }}
    >
      <Icon
        name={isOk() ? "checkCircle" : "xCircle"}
        size={13}
        color={isOk() ? paletteToken("ok") : paletteToken("danger")}
      />
      <Text
        style={{
          fontSize: 12,
          fontWeight: "500",
          color: isOk() ? paletteToken("ok") : paletteToken("danger"),
        }}
      >
        {state.status()}
      </Text>
    </View>
  );
}
