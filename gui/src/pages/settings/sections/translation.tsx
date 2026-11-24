// 「翻译」分区 UI — 状态机与桥接在 ./translationState.ts（保持单文件 < 800 行）。

import { List, Text, View, cleanup, computed, effect, type JSX } from "nsh";
import { Input } from "nsh/base";
import { paletteToken } from "../../../services/themeMode";
import { t, tf } from "../../../services/translation";
import { Icon } from "../../../chrome/icons";
import { IconSpin } from "../../../chrome/feedback";
import { TextSlider } from "../../../chrome/segmented";
import {
  CollapseChevron,
  FieldLabel,
  MONO_FONT,
  SectionShell,
  SettingRow,
  SettingsGroup,
  SettingsSwitch,
  StatusPill,
} from "../parts";
import { SETTINGS_SECTIONS } from "../sections";
import { st } from "../settingsStrings";
import {
  createTranslationState,
  realTranslationBridge,
  targetSelectedIndex,
  type TranslationState,
} from "./translationState";

export {
  createTranslationState,
  pullErrorMessage,
  realTranslationBridge,
  targetSelectedIndex,
  type ModelPullReport,
  type TranslationBridge,
  type TranslationConfigDto,
  type TranslationLanguageRow,
  type TranslationState,
} from "./translationState";

const pageState = createTranslationState(realTranslationBridge);

export function TranslationSection(): JSX.Element {
  return <TranslationWorkbench state={pageState} />;
}

export function TranslationWorkbench(props: { state: TranslationState }): JSX.Element {
  const state = props.state;
  effect(() => state.start());
  return (
    <SectionShell
      section={SETTINGS_SECTIONS.find((section) => section.id === "translation")!}
      grouped
    >
      <SettingsGroup title={st("settings.group.translationScope")}>
        <SettingRow
          icon="fileText"
          iconTone="cyan"
          title={t("settings.tranDescriptions")}
          hint={t("settings.tranDescriptionsHint")}
          control={
            <SettingsSwitch
              id="tran-descriptions"
              checked={state.config().translate_descriptions}
              onToggle={() => state.toggleDescriptions()}
            />
          }
        />
        <SettingRow
          icon="fileText"
          iconTone="accent"
          borderTop
          title={t("settings.tranSkillMd")}
          hint={t("settings.tranSkillMdHint")}
          control={
            <SettingsSwitch
              id="tran-skill-md"
              checked={state.config().translate_skill_md}
              onToggle={() => state.toggleSkillMd()}
            />
          }
        />
        <SettingRow
          icon="globe"
          iconTone="info"
          borderTop
          title={t("settings.tranTarget")}
          hint={t("settings.tranTargetHint")}
          control={
            <If when={state.languages().length > 0} else={<View />}>
              <TextSlider
                motionId="tran-target-motion"
                options={state.languages().map((language) => ({
                  id: language.code,
                  label: language.label,
                }))}
                selected={targetSelectedIndex(state.config().target_lang, state.languages())}
                onClick={(ix) => state.setTargetLang(state.languages()[ix].code)}
              />
            </If>
          }
        />
      </SettingsGroup>
      <SettingsGroup title={st("settings.group.translationEngine")}>
        <SettingRow
          icon="sparkle"
          iconTone="amber"
          title={t("settings.tranEngine")}
          hint={
            state.config().engine === "llm"
              ? t("settings.tranEngineLlmHint")
              : t("settings.tranEngineMachineHint")
          }
          control={
            <TextSlider
              motionId="tran-engine-motion"
              options={[
                { id: "tran-engine-machine", label: t("settings.tranEngineMachine") },
                { id: "tran-engine-llm", label: t("settings.tranEngineLlm") },
              ]}
              selected={state.config().engine === "llm" ? 1 : 0}
              onClick={(ix) => state.setEngine(ix === 1 ? "llm" : "machine")}
            />
          }
        >
          <If when={state.config().engine === "llm"}>{() => <LlmBlock state={state} />}</If>
        </SettingRow>
      </SettingsGroup>
    </SectionShell>
  );
}

/**
 * LLM 区：以 1px 发丝线与 `well` 托盘区隔于引擎行下，接口地址 + 密钥两列等高齐平，
 * 下方接模型选择与拉取区。密钥只在 `llmKeySaved()` 为真时渲染「已保存」胶囊，从不回显原文。
 */
function LlmBlock(props: { state: TranslationState }): JSX.Element {
  const state = props.state;
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 14,
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
          gap: 12,
          width: "100%",
          alignItems: "flex-end",
        }}
      >
        {/* 两侧保持相同的 22px trailing 占位高度，确保状态胶囊切换时双列输入框零跳动齐平。 */}
        <FieldLabel
          title={t("settings.tranLlmBaseUrl")}
          style={{ flex: 1 }}
          trailing={<View style={{ height: 22 }} />}
        >
          <BaseUrlField state={state} />
        </FieldLabel>
        <FieldLabel
          title={t("settings.tranLlmKey")}
          style={{ flex: 1 }}
          trailing={
            <If when={state.llmKeySaved()} else={<View style={{ height: 22 }} />}>
              <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6, height: 22 }}>
                <StatusPill
                  testID="tran-llm-key-state"
                  tone="ok"
                  icon="check"
                  text={t("settings.tranLlmKeySaved")}
                />
                <View
                  testID="tran-llm-key-clear"
                  role="button"
                  accessibilityLabel={t("common.clear")}
                  style={{
                    height: 22,
                    paddingHorizontal: 7,
                    display: "flex",
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 3,
                    borderRadius: 999,
                    borderWidth: 1,
                    borderColor: paletteToken("border"),
                    backgroundColor: paletteToken("card"),
                    cursor: "pointer",
                  }}
                  hoverStyle={{
                    backgroundColor: paletteToken("danger_bg"),
                    borderColor: paletteToken("danger_border"),
                  }}
                  activeStyle={{ backgroundColor: paletteToken("well") }}
                  onPress={() => state.clearLlmKey()}
                >
                  <Icon name="x" size={11} color={paletteToken("fg_muted")} />
                  <Text style={{ fontSize: 11, fontWeight: "500", color: paletteToken("fg_muted") }}>
                    {t("common.clear")}
                  </Text>
                </View>
              </View>
            </If>
          }
        >
          <BlurInput
            testID="tran-llm-key"
            value={state.llmKeyDraft()}
            placeholder={state.llmKeySaved() ? "••••••••" : ""}
            masked
            mono
            onChange={(value) => state.setLlmKeyDraft(value)}
            onCommit={() => state.commitLlmKey()}
          />
        </FieldLabel>
      </View>
      <ModelField state={state} />
    </View>
  );
}

/**
 * 接口地址行：输入框自己持有草稿（组件体只执行一次，闭包变量就是草稿），
 * 仅在已持久化地址实际变更时同步草稿，失焦/回车才写盘。
 */
function BaseUrlField(props: { state: TranslationState }): JSX.Element {
  const state = props.state;
  let lastSynced = state.config().llm_base_url;
  let draft = lastSynced;
  effect(() => {
    const saved = state.config().llm_base_url;
    if (saved !== lastSynced) {
      lastSynced = saved;
      draft = saved;
    }
  });
  return (
    <BlurInput
      testID="tran-llm-base-url"
      value={state.config().llm_base_url}
      placeholder="https://api.example.com/v1"
      mono
      onChange={(value) => {
        draft = value;
      }}
      onCommit={() => state.setLlmBaseUrl(draft)}
    />
  );
}

/** 失焦/回车提交的输入框；所有响应式 props 在 JSX 表达式内读取。 */
function BlurInput(props: {
  testID: string;
  value: string;
  placeholder: string;
  masked?: boolean;
  mono?: boolean;
  onChange: (value: string) => void;
  onCommit: () => void;
}): JSX.Element {
  return (
    <View
      style={{
        width: "100%",
        height: 36,
        paddingHorizontal: 10,
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        borderRadius: 10,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("input"),
      }}
      hoverStyle={{ borderColor: paletteToken("edge") }}
      focusWithinStyle={{ borderColor: paletteToken("accent") }}
    >
      <Input
        testID={props.testID}
        style={{
          flex: 1,
          minWidth: 0,
          fontSize: 13,
          color: paletteToken("fg"),
          caretColor: paletteToken("accent"),
          placeholderColor: paletteToken("fg_faint"),
          padding: 0,
          backgroundColor: "transparent",
          borderWidth: 0,
          ...(props.mono ? { fontFamily: MONO_FONT } : {}),
        }}
        value={props.value}
        placeholder={props.placeholder}
        masked={props.masked === true}
        submitBehavior="blurAndSubmit"
        onChange={(event) => props.onChange(event.value)}
        onBlur={() => props.onCommit()}
        onSubmit={() => props.onCommit()}
      />
    </View>
  );
}

/** 拉到的目录是否归属当前接口地址（fetched 判定）。只在 JSX 表达式里调用。 */
function fetchedModels(state: TranslationState): string[] | null {
  const base = state.config().llm_base_url.trim();
  if (base.length > 0 && state.modelsFor() === base && state.models().length > 0) {
    return state.models();
  }
  return null;
}

/** fetched 目录的规模；0 = 未拉取（fetched 定义保证非空）。 */
function fetchedCount(state: TranslationState): number {
  return fetchedModels(state)?.length ?? 0;
}

/** sync_llm_model_state 的派生半边：目录 ∪ 当前模型（缺则补首行）。 */
export function modelRows(state: TranslationState): string[] {
  const fetched = fetchedModels(state);
  const rows = fetched === null ? [] : fetched.slice();
  const currentModel = state.config().llm_model.trim();
  if (currentModel.length > 0 && !rows.some((id) => id === currentModel)) {
    rows.unshift(currentModel);
  }
  return rows;
}

/** 菜单可见行：目录行再过搜索词（大小写不敏感）。 */
export function modelMenuRows(state: TranslationState): string[] {
  const query = state.modelQuery().trim().toLowerCase();
  if (query.length === 0) {
    return modelRows(state);
  }
  return modelRows(state).filter((id) => id.toLowerCase().includes(query));
}

function ModelField(props: { state: TranslationState }): JSX.Element {
  const state = props.state;
  return (
    <FieldLabel title={t("settings.tranLlmModel")}>
      <View style={{ display: "flex", flexDirection: "column", gap: 8, width: "100%" }}>
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "flex-start",
            gap: 8,
            width: "100%",
          }}
        >
          <View testID="tran-llm-model-field" style={{ flex: 1, minWidth: 0, display: "flex" }}>
            <ModelCombobox state={state} />
          </View>
          <PullButton state={state} />
        </View>
        <If when={state.modelsError() !== null}>
          <Text
            testID="tran-llm-model-error"
            style={{ fontSize: 12, color: paletteToken("danger_fg"), lineHeight: 18 }}
          >
            {state.modelsError()}
          </Text>
        </If>
        <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), lineHeight: 18 }}>
          {fetchedCount(state) > 0
            ? tf("settings.tranLlmModelCount", { count: fetchedCount(state) })
            : t("settings.tranLlmModelHint")}
        </Text>
      </View>
    </FieldLabel>
  );
}

/** 拉取钮（tran-llm-pull）：与模型选择器统一 36px 高度与 10px 圆角，拉取中旋转并吞点击。 */
function PullButton(props: { state: TranslationState }): JSX.Element {
  const state = props.state;
  return (
    <View
      testID="tran-llm-pull"
      role="button"
      accessibilityState={{ busy: state.modelsLoading() }}
      style={{
        height: 36,
        paddingHorizontal: 13,
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 6,
        borderRadius: 10,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        cursor: state.modelsLoading() ? "default" : "pointer",
        flexShrink: 0,
        opacity: 1,
      }}
      hoverStyle={
        state.modelsLoading()
          ? undefined
          : { backgroundColor: paletteToken("panel_hover"), borderColor: paletteToken("edge") }
      }
      activeStyle={state.modelsLoading() ? undefined : { backgroundColor: paletteToken("well") }}
      onPress={() => {
        if (!state.modelsLoading()) {
          state.pullModels();
        }
      }}
    >
      <IconSpin
        name="refresh"
        size={14}
        color={paletteToken("fg_muted")}
        spin={state.modelsLoading()}
      />
      <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg") }}>
        {t("settings.tranLlmModelPull")}
      </Text>
    </View>
  );
}

const MODEL_ITEM_H = 32;

/** 模型下拉：36px 可编辑触发器（支持直接输入自定义模型 id + 回车/失焦/确认）+ 可搜索、限高滚动菜单。 */
function ModelCombobox(props: { state: TranslationState }): JSX.Element {
  const state = props.state;
  const dirty = () => {
    const draft = state.modelDraft().trim();
    const saved = state.config().llm_model.trim();
    return draft.length > 0 && (draft !== saved || !state.config().llm_model_pinned);
  };
  return (
    <View style={{ width: "100%", display: "flex", flexDirection: "column", gap: 6 }}>
      <View
        style={{
          height: 36,
          paddingLeft: 10,
          paddingRight: 4,
          width: "100%",
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 4,
          borderRadius: 10,
          borderWidth: 1,
          borderColor: state.modelMenuOpen() ? paletteToken("accent") : paletteToken("border"),
          backgroundColor: paletteToken("input"),
        }}
        hoverStyle={{ borderColor: paletteToken("edge") }}
        focusWithinStyle={{ borderColor: paletteToken("accent") }}
      >
        <Input
          testID="tran-llm-model-input"
          style={{
            flex: 1,
            minWidth: 0,
            fontSize: 13,
            fontFamily: MONO_FONT,
            color: paletteToken("fg"),
            caretColor: paletteToken("accent"),
            placeholderColor: paletteToken("fg_faint"),
            padding: 0,
            backgroundColor: "transparent",
            borderWidth: 0,
          }}
          value={state.modelDraft()}
          placeholder="gpt-4o-mini"
          submitBehavior="blurAndSubmit"
          onChange={(event) => state.setModelDraft(event.value)}
          onBlur={() => state.commitModelDraft()}
          onSubmit={() => state.commitModelDraft()}
        />
        <If when={dirty()}>
          <View
            testID="tran-llm-model-confirm"
            role="button"
            accessibilityLabel={t("common.done")}
            style={{
              height: 26,
              paddingHorizontal: 8,
              borderRadius: 7,
              borderWidth: 1,
              borderColor: paletteToken("accent_soft_edge"),
              backgroundColor: paletteToken("accent_soft"),
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              justifyContent: "center",
              cursor: "pointer",
            }}
            hoverStyle={{ backgroundColor: paletteToken("card_active") }}
            onPress={() => state.commitModelDraft()}
          >
            <Icon name="check" size={13} color={paletteToken("accent_fg")} />
          </View>
        </If>
        <View
          testID="tran-llm-model-button"
          role="button"
          accessibilityState={{ expanded: state.modelMenuOpen() }}
          style={{
            width: 28,
            height: 28,
            borderRadius: 7,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            cursor: "pointer",
          }}
          hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
          onPress={() => state.setModelMenuOpen(!state.modelMenuOpen())}
        >
          <CollapseChevron open={state.modelMenuOpen()} />
        </View>
      </View>
      <If when={state.modelMenuOpen()}>{() => <ModelMenu state={state} />}</If>
    </View>
  );
}

function ModelMenu(props: { state: TranslationState }): JSX.Element {
  const state = props.state;
  let blurTimer: ReturnType<typeof setTimeout> | null = null;
  const cancelBlur = (): void => {
    if (blurTimer !== null) {
      clearTimeout(blurTimer);
      blurTimer = null;
    }
  };
  const applyCustomQuery = (): void => {
    cancelBlur();
    const query = state.modelQuery().trim();
    if (query.length === 0) return;
    state.selectModel(query);
    state.setModelMenuOpen(false);
  };
  cleanup(() => cancelBlur());

  return (
    <View
      testID="tran-llm-model-menu"
      style={{
        width: "100%",
        borderRadius: 10,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
      }}
    >
      <View
        style={{
          height: 36,
          paddingLeft: 10,
          paddingRight: 6,
          borderBottomWidth: 1,
          borderBottomColor: paletteToken("border_soft"),
          backgroundColor: paletteToken("input"),
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 8,
        }}
      >
        <Icon name="search" size={14} color={paletteToken("fg_faint")} />
        <Input
          testID="tran-llm-model-search"
          style={{
            flex: 1,
            minWidth: 0,
            fontSize: 12,
            color: paletteToken("fg"),
            caretColor: paletteToken("accent"),
            placeholderColor: paletteToken("fg_faint"),
            padding: 0,
            backgroundColor: "transparent",
            borderWidth: 0,
          }}
          value={state.modelQuery()}
          placeholder={t("settings.tranLlmModelSearch")}
          submitBehavior="blurAndSubmit"
          onChange={(event) => state.setModelQuery(event.value)}
          onSubmit={() => applyCustomQuery()}
          onBlur={() => {
            cancelBlur();
            blurTimer = setTimeout(() => {
              blurTimer = null;
              if (state.modelMenuOpen() && state.modelQuery().trim().length > 0) {
                applyCustomQuery();
              }
            }, 120);
          }}
        />
        <If when={state.modelQuery().trim().length > 0}>
          <View
            testID="tran-llm-model-menu-confirm"
            role="button"
            accessibilityLabel={t("common.done")}
            style={{
              height: 24,
              paddingHorizontal: 7,
              borderRadius: 6,
              borderWidth: 1,
              borderColor: paletteToken("accent_soft_edge"),
              backgroundColor: paletteToken("accent_soft"),
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              justifyContent: "center",
              cursor: "pointer",
            }}
            hoverStyle={{ backgroundColor: paletteToken("card_active") }}
            onPress={() => applyCustomQuery()}
          >
            <Icon name="check" size={12} color={paletteToken("accent_fg")} />
          </View>
        </If>
      </View>
      {/* 限高菜单：定像素视口框 + <List> 虚拟化（nsh 定高滚动契约，空时保留 1 行高）。 */}
      {(() => {
        const rows = computed(() => modelMenuRows(state));
        const customCandidate = computed(() => {
          const query = state.modelQuery().trim();
          return query.length > 0 && !modelRows(state).includes(query) ? query : null;
        });
        return (
          <View style={{ width: "100%", display: "flex", flexDirection: "column" }}>
            <If when={customCandidate() !== null}>
              {() => (
                <View
                  testID="tran-llm-model-custom-option"
                  role="menuitem"
                  style={{
                    width: "100%",
                    height: MODEL_ITEM_H,
                    paddingHorizontal: 12,
                    borderBottomWidth: 1,
                    borderBottomColor: paletteToken("border_soft"),
                    backgroundColor: paletteToken("accent_soft"),
                    display: "flex",
                    flexDirection: "row",
                    alignItems: "center",
                    justifyContent: "space-between",
                    gap: 8,
                    cursor: "pointer",
                  }}
                  hoverStyle={{ backgroundColor: paletteToken("card_active") }}
                  onPress={() => applyCustomQuery()}
                >
                  <Text
                    style={{
                      flex: 1,
                      minWidth: 0,
                      fontSize: 12,
                      fontFamily: MONO_FONT,
                      fontWeight: "600",
                      color: paletteToken("accent_fg"),
                      numberOfLines: 1,
                    }}
                  >
                    {customCandidate() ?? ""}
                  </Text>
                  <Icon name="check" size={13} color={paletteToken("accent_fg")} />
                </View>
              )}
            </If>
            <If when={rows().length > 0 || customCandidate() === null}>
              <View
                style={{
                  height: Math.min(Math.max(rows().length, 1) * MODEL_ITEM_H + 8, 280),
                  width: "100%",
                }}
              >
                <If
                  when={rows().length > 0}
                  else={
                    <View style={{ padding: 4, width: "100%" }}>
                      <View
                        style={{
                          height: MODEL_ITEM_H,
                          paddingHorizontal: 8,
                          display: "flex",
                          alignItems: "center",
                          justifyContent: "center",
                          width: "100%",
                        }}
                      >
                        <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
                          {t("settings.tranLlmModelEmpty")}
                        </Text>
                      </View>
                    </View>
                  }
                >
                  <List
                    each={rows()}
                    keyExtractor={(model: string) => model}
                    estimatedItemSize={MODEL_ITEM_H}
                    scrollEventThrottle={32}
                    style={{ width: "100%", height: "100%" }}
                    contentStyle={{ padding: 4 }}
                  >
                    {(model: () => string) => (
                      <ModelMenuItem
                        model={model()}
                        selected={model() === state.config().llm_model.trim()}
                        onSelect={() => {
                          cancelBlur();
                          state.selectModel(model());
                          state.setModelMenuOpen(false);
                        }}
                      />
                    )}
                  </List>
                </If>
              </View>
            </If>
          </View>
        );
      })()}
      {/* footer 报目录规模（fetched 才有）。 */}
      <If when={fetchedCount(state) > 0}>
        <View
          style={{
            paddingHorizontal: 10,
            paddingVertical: 7,
            borderTopWidth: 1,
            borderTopColor: paletteToken("border_soft"),
            backgroundColor: paletteToken("panel"),
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <Text
            testID="tran-llm-model-total"
            style={{
              fontSize: 11,
              fontVariant: ["tabular-nums"],
              color: paletteToken("fg_muted"),
            }}
          >
            {tf("settings.tranLlmModelTotal", { count: fetchedCount(state) })}
          </Text>
        </View>
      </If>
    </View>
  );
}

function ModelMenuItem(props: {
  model: string;
  selected: boolean;
  onSelect: () => void;
}): JSX.Element {
  return (
    <View
      testID={`tran-llm-model-item-${props.model}`}
      role="menuitem"
      style={{
        width: "100%",
        height: MODEL_ITEM_H,
        paddingHorizontal: 8,
        borderRadius: 7,
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 8,
        backgroundColor: props.selected ? paletteToken("card_active") : "transparent",
        cursor: "pointer",
      }}
      hoverStyle={{
        backgroundColor: props.selected
          ? paletteToken("card_active_hover")
          : paletteToken("panel_hover"),
      }}
      onPress={props.onSelect}
    >
      <Text
        style={{
          flex: 1,
          minWidth: 0,
          fontSize: 12,
          fontFamily: MONO_FONT,
          fontWeight: props.selected ? "600" : "400",
          color: props.selected ? paletteToken("accent_fg") : paletteToken("fg"),
          numberOfLines: 1,
        }}
      >
        {props.model}
      </Text>
      <If when={props.selected}>
        <Icon name="check" size={13} color={paletteToken("accent_fg")} />
      </If>
    </View>
  );
}
