// 「技能更新」设置块 — 可独立渲染（SkillUpdatesWorkbench 供测试）或嵌入 GeneralSection。

import { Text, View, effect, signal, type JSX, type Signal } from "nsh";
import { SettingsUpdates } from "skillstar-gui/native";
import { paletteToken } from "../../../services/themeMode";
import { t } from "../../../services/translation";
import { Icon } from "../../../chrome/icons";
import { TextSlider } from "../../../chrome/segmented";
import { ChoicePills, SectionShell, SettingRow } from "../parts";
import { SKILL_UPDATES_SECTION } from "../sections";

/** skill_updates.json 的两字段（SkillUpdateConfig 的 serde 形状）。 */
export interface SkillUpdatesDto {
  auto_update: boolean;
  interval_minutes: number;
}

export interface SkillUpdatesBridge {
  load(): SkillUpdatesDto;
  save(config: SkillUpdatesDto): string | null;
}

function asSkillUpdates(value: unknown): SkillUpdatesDto {
  const raw = (value ?? {}) as Partial<SkillUpdatesDto>;
  return {
    auto_update: raw.auto_update === true,
    interval_minutes:
      typeof raw.interval_minutes === "number" && Number.isFinite(raw.interval_minutes)
        ? raw.interval_minutes
        : 60,
  };
}

/** 真桥：SettingsUpdates facade 直调（页面默认使用）。 */
export const realSkillUpdatesBridge: SkillUpdatesBridge = {
  load() {
    return asSkillUpdates(SettingsUpdates.loadSkillUpdates());
  },
  save(config) {
    try {
      SettingsUpdates.saveSkillUpdates(config.auto_update, config.interval_minutes);
      return null;
    } catch (error) {
      return String(error);
    }
  },
};

export interface SkillUpdatesState {
  updates: Signal<SkillUpdatesDto>;
  start(): void;
  setAutoUpdate(auto: boolean): void;
  setIntervalMinutes(minutes: number): void;
}

export function createSkillUpdatesState(bridge: SkillUpdatesBridge): SkillUpdatesState {
  const updates = signal<SkillUpdatesDto>({ auto_update: false, interval_minutes: 60 });

  const save = (next: SkillUpdatesDto): void => {
    updates.set(next);
    const error = bridge.save(next);
    if (error !== null) {
      console.warn(`failed to save skill update prefs: ${error}`);
    }
  };

  return {
    updates,
    start() {
      updates.set(bridge.load());
    },
    setAutoUpdate(auto) {
      save({ ...updates(), auto_update: auto });
    },
    setIntervalMinutes(minutes) {
      save({ ...updates(), interval_minutes: minutes });
    },
  };
}

const pageState = createSkillUpdatesState(realSkillUpdatesBridge);

/** 嵌入 GeneralSection 的受控行（挂载时读盘一次）。 */
export function SkillUpdatesEmbedded(props: { borderTop?: boolean }): JSX.Element {
  effect(() => pageState.start());
  return <SkillUpdatesRows state={pageState} borderTop={props.borderTop} />;
}

/** 独立工作台（供 skillUpdates.test.tsx 注入假桥测试）。 */
export function SkillUpdatesWorkbench(props: { state: SkillUpdatesState }): JSX.Element {
  const state = props.state;
  effect(() => state.start());
  return (
    <SectionShell section={SKILL_UPDATES_SECTION}>
      <SkillUpdatesRows state={state} />
    </SectionShell>
  );
}

/** 技能更新开关行 + 自动更新时的频率托盘。 */
export function SkillUpdatesRows(props: {
  state: SkillUpdatesState;
  borderTop?: boolean;
}): JSX.Element {
  const state = props.state;
  return (
    <SettingRow
      icon="refresh"
      iconTone="accent"
      title={t("settings.skillUpdates")}
      hint={t("settings.skillUpdatesHint")}
      borderTop={props.borderTop}
      control={
        <TextSlider
          motionId="skill-update-mode-motion"
          options={[
            { id: "skill-update-mode-manual", label: t("settings.skillUpdatesManual") },
            { id: "skill-update-mode-auto", label: t("settings.skillUpdatesAuto") },
          ]}
          selected={state.updates().auto_update ? 1 : 0}
          onClick={(ix) => state.setAutoUpdate(ix === 1)}
        />
      }
    >
      <If when={state.updates().auto_update}>{() => <IntervalRow state={state} />}</If>
    </SettingRow>
  );
}

/** 自动更新检查频率托盘（仅 auto_update 为 true 时挂载，false 时完全卸载）。 */
function IntervalRow(props: { state: SkillUpdatesState }): JSX.Element {
  const state = props.state;
  return (
    <View
      style={{
        width: "100%",
        paddingHorizontal: 18,
        paddingVertical: 12,
        borderTopWidth: 1,
        borderTopColor: paletteToken("border_soft"),
        backgroundColor: paletteToken("well"),
        display: "flex",
        flexDirection: "row",
        flexWrap: "wrap",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 10,
      }}
    >
      <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6 }}>
        <Icon name="clock" size={13} color={paletteToken("fg_muted")} />
        <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg_muted") }}>
          {t("settings.skillUpdatesInterval")}
        </Text>
      </View>
      <ChoicePills
        idPrefix="skill-update-interval"
        options={[
          { id: "15", label: t("settings.skillUpdatesEvery15") },
          { id: "30", label: t("settings.skillUpdatesEvery30") },
          { id: "60", label: t("settings.skillUpdatesEvery60") },
          { id: "360", label: t("settings.skillUpdatesEvery360") },
          { id: "1440", label: t("settings.skillUpdatesEveryDay") },
        ]}
        current={String(state.updates().interval_minutes)}
        onPick={(id) => {
          const minutes = Number.parseInt(id, 10);
          if (Number.isFinite(minutes)) {
            state.setIntervalMinutes(minutes);
          }
        }}
      />
    </View>
  );
}
