import { Text, View, effect, type JSX } from "nsh";
import {
  seedTrayQuotaPrefs,
  setTrayQuotaEnabled,
  trayQuotaEnabled,
} from "../../shell/trayQuotaPrefs";
import { Input } from "nsh/base";
import { isLight, paletteToken } from "../../services/themeMode";
import { t } from "../../services/translation";
import { Icon } from "../../chrome/icons";
import { TextSlider } from "../../chrome/segmented";
import {
  CollapseChevron,
  FieldLabel,
  MONO_FONT,
  MetaChip,
  SectionShell,
  SettingRow,
  SettingsGroup,
  SettingsSwitch,
  card,
} from "./parts";
import { st } from "./settingsStrings";
import { SETTINGS_SECTIONS, type SettingsSectionId } from "./sections";
import { PROXY_SCHEMES, type ProxyScheme } from "./bridge";
import type { SettingsState } from "./state";
import { SkillUpdatesEmbedded } from "./sections/skillUpdates";
import { MirrorEmbedded } from "./sections/ghMirror";
import { DoctorInlinePanel, DoctorRunButton, doctorPageState } from "./sections/networkDoctor";

// 常规（外观 / 语言 / 后台运行 / 技能更新）与网络（诊断 / 代理 / GitHub 加速）
// 两大分区 + 从 parts 透传 SettingsSwitch 兼容过渡导入。颜色统一走 paletteToken()，
// 组件体只执行一次，所有动态信号均在 JSX 表达式与回调内读取。

export { SettingsSwitch } from "./parts";

function sectionOf(id: SettingsSectionId) {
  return SETTINGS_SECTIONS.find((section) => section.id === id)!;
}

function normalizeSelected(language: string): number {
  const normalized = language.trim().toLowerCase();
  return normalized === "en" || normalized.startsWith("en-") ? 1 : 0;
}

/** 常规分区：外观与语言分组卡 + 运行与自动更新分组卡。 */
export function GeneralSection(props: { state: SettingsState }): JSX.Element {
  const state = props.state;
  effect(() => seedTrayQuotaPrefs());
  return (
    <SectionShell section={sectionOf("general")} grouped>
      <SettingsGroup title={st("settings.group.appearance")}>
        <View style={{ display: "flex", flexDirection: "column", width: "100%" }}>
          <SettingRow
            icon={isLight() ? "sun" : "moon"}
            iconTone="amber"
            title={t("settings.backgroundStyle")}
            hint={t("settings.appearanceHint")}
            control={
              <TextSlider
                motionId="appearance-motion"
                options={[
                  { id: "appearance-paper", label: t("settings.backgroundPaper") },
                  { id: "appearance-current", label: t("settings.backgroundCurrent") },
                ]}
                selected={isLight() ? 0 : 1}
                onClick={(ix) => state.setBackgroundStyle(ix === 0 ? "paper" : "current")}
              />
            }
          />
          <SettingRow
            icon="languages"
            iconTone="info"
            borderTop
            title={t("settings.language")}
            hint={t("settings.languageHint")}
            control={
              <TextSlider
                motionId="ui-lang-motion"
                options={[
                  { id: "ui-lang-zh-CN", label: "简体中文" },
                  { id: "ui-lang-en", label: "English" },
                ]}
                selected={normalizeSelected(state.prefs().language)}
                onClick={(ix) => state.setLanguage(ix === 0 ? "zh-CN" : "en")}
              />
            }
          />
        </View>
      </SettingsGroup>
      <SettingsGroup title={st("settings.group.runtimeUpdates")}>
        <View style={{ display: "flex", flexDirection: "column", width: "100%" }}>
          <SettingRow
            icon="laptop"
            iconTone="cyan"
            title={t("settings.backgroundRun")}
            hint={t("settings.backgroundRunHint")}
            control={
              <SettingsSwitch
                id="bg-run-toggle"
                checked={state.prefs().background_run}
                onToggle={() => state.setBackgroundRun(!state.prefs().background_run)}
              />
            }
          />
          <SettingRow
            icon="gauge"
            iconTone="amber"
            borderTop
            title={st("settings.trayQuota")}
            hint={st("settings.trayQuotaHint")}
            control={
              <SettingsSwitch
                id="tray-quota-toggle"
                checked={trayQuotaEnabled()}
                onToggle={() => setTrayQuotaEnabled(!trayQuotaEnabled())}
              />
            }
          />
          <SkillUpdatesEmbedded borderTop />
        </View>
      </SettingsGroup>
    </SectionShell>
  );
}

/** 网络分区：右上角一键诊断 + 独立呼吸卡片栈（诊断报告 / 网络代理 / GitHub 加速）。 */
export function NetworkSection(props: { state: SettingsState }): JSX.Element {
  const state = props.state;
  return (
    <SectionShell
      section={sectionOf("network")}
      grouped
      action={<DoctorRunButton state={doctorPageState} />}
    >
      <DoctorInlinePanel state={doctorPageState} />
      <ProxyBlock state={state} />
      <MirrorEmbedded />
    </SectionShell>
  );
}

function ProxyBlock(props: { state: SettingsState }): JSX.Element {
  const state = props.state;
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
          testID="proxy-collapse"
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
          onPress={() => state.setProxyExpanded(!state.proxyExpanded())}
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
                borderColor: paletteToken("info_border"),
                backgroundColor: paletteToken("info_bg"),
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
              }}
            >
              <Icon name="globe" size={15} color={paletteToken("info")} />
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
                  {t("settings.networkProxy")}
                </Text>
                <If when={state.proxy().enabled && state.proxy().host.length > 0}>
                  {() => (
                    <MetaChipForProxy
                      scheme={state.proxy().proxy_type}
                      host={state.proxy().host}
                      port={state.proxy().port}
                    />
                  )}
                </If>
              </View>
              <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), lineHeight: 18 }}>
                {t("settings.proxyHint")}
              </Text>
            </View>
          </View>
          <CollapseChevron open={state.proxyExpanded()} />
        </View>
        <View style={{ paddingRight: 18, flexShrink: 0 }}>
          <SettingsSwitch
            id="proxy-enabled"
            checked={state.proxy().enabled}
            onToggle={() => state.commitProxy({ enabled: !state.proxy().enabled })}
          />
        </View>
      </View>
      <If when={state.proxyExpanded()}>{() => <ProxyForm state={state} />}</If>
    </View>
  );
}

function MetaChipForProxy(props: { scheme: ProxyScheme; host: string; port: number }): JSX.Element {
  return (
    <MetaChip
      text={`${props.scheme.toUpperCase()}://${props.host}:${props.port}`}
      mono
      tone="info"
    />
  );
}

function ProxyForm(props: { state: SettingsState }): JSX.Element {
  const state = props.state;
  const form = state.proxyForm;
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 12,
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
          alignItems: "flex-start",
        }}
      >
        <FieldLabel title={t("settings.proxyType")} style={{ width: 128, flexShrink: 0 }}>
          <ProxyTypeMenu state={state} />
        </FieldLabel>
        <FieldLabel title={t("settings.proxyHost")} style={{ flex: 1 }}>
          <ProxyInput
            testID="proxy-host"
            mono
            placeholder="127.0.0.1"
            value={form().host}
            onChange={(value) => state.editProxyField("host", value)}
            onCommit={() => state.commitProxy()}
          />
        </FieldLabel>
        <FieldLabel title={t("settings.proxyPort")} style={{ width: 92, flexShrink: 0 }}>
          <ProxyInput
            testID="proxy-port"
            mono
            placeholder="7897"
            value={form().port}
            onChange={(value) => state.editProxyField("port", value)}
            onCommit={() => state.commitProxy()}
          />
        </FieldLabel>
      </View>
      <If when={state.proxy().proxy_type === "socks5" || state.proxy().proxy_type === "socks5h"}>
        {() => (
          <View
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              gap: 8,
              paddingHorizontal: 12,
              paddingVertical: 9,
              borderRadius: 10,
              borderWidth: 1,
              borderColor: paletteToken("info_border"),
              backgroundColor: paletteToken("info_bg"),
              width: "100%",
            }}
          >
            <Icon name="info" size={14} color={paletteToken("info")} />
            <Text
              style={{
                flex: 1,
                minWidth: 0,
                fontSize: 12,
                color: paletteToken("fg"),
                lineHeight: 18,
              }}
            >
              {t("settings.proxySocks5hHint")}
            </Text>
          </View>
        )}
      </If>
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          gap: 12,
          width: "100%",
          alignItems: "flex-end",
        }}
      >
        <FieldLabel title={t("settings.proxyUsername")} style={{ flex: 1 }}>
          <ProxyInput
            testID="proxy-user"
            placeholder={t("common.optional")}
            value={form().user}
            onChange={(value) => state.editProxyField("user", value)}
            onCommit={() => state.commitProxy()}
          />
        </FieldLabel>
        <FieldLabel title={t("settings.proxyPassword")} style={{ flex: 1 }}>
          <ProxyInput
            testID="proxy-pass"
            masked
            placeholder={t("common.optional")}
            value={form().pass}
            onChange={(value) => state.editProxyField("pass", value)}
            onCommit={() => state.commitProxy()}
          />
        </FieldLabel>
        <FieldLabel title={t("settings.proxyBypass")} style={{ flex: 1 }}>
          <ProxyInput
            testID="proxy-bypass"
            mono
            placeholder={t("settings.proxyBypassPlaceholder")}
            value={form().bypass}
            onChange={(value) => state.editProxyField("bypass", value)}
            onCommit={() => state.commitProxy()}
          />
        </FieldLabel>
      </View>
      <If when={state.proxyStatus() !== null}>
        {() => <ProxyStatusRow state={state} />}
      </If>
    </View>
  );
}

function ProxyStatusRow(props: { state: SettingsState }): JSX.Element {
  const state = props.state;
  const isSaved = () => state.proxyStatus() === t("common.saved");
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
        name={isSaved() ? "checkCircle" : "xCircle"}
        size={13}
        color={isSaved() ? paletteToken("ok") : paletteToken("danger")}
      />
      <Text
        style={{
          fontSize: 12,
          fontWeight: "500",
          color: isSaved() ? paletteToken("ok") : paletteToken("danger"),
        }}
      >
        {state.proxyStatus()}
      </Text>
    </View>
  );
}

function ProxyTypeMenu(props: { state: SettingsState }): JSX.Element {
  const state = props.state;
  return (
    <View style={{ width: "100%" }}>
      <View
        testID="proxy-type-button"
        role="button"
        style={{
          height: 36,
          paddingHorizontal: 12,
          width: "100%",
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "space-between",
          borderRadius: 10,
          borderWidth: 1,
          borderColor: state.proxyTypeOpen() ? paletteToken("edge") : paletteToken("border"),
          backgroundColor: paletteToken("input"),
          cursor: "pointer",
        }}
        hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
        onPress={() => state.setProxyTypeOpen(!state.proxyTypeOpen())}
      >
        <Text
          style={{
            fontSize: 13,
            fontWeight: "500",
            fontFamily: MONO_FONT,
            color: paletteToken("fg"),
          }}
        >
          {state.proxy().proxy_type.toUpperCase()}
        </Text>
        <CollapseChevron open={state.proxyTypeOpen()} />
      </View>
      <If when={state.proxyTypeOpen()}>
        {() => (
          <View
            style={{
              marginTop: 4,
              width: "100%",
              borderRadius: 10,
              borderWidth: 1,
              borderColor: paletteToken("border"),
              backgroundColor: paletteToken("card"),
              overflow: "hidden",
            }}
          >
            {PROXY_SCHEMES.map((scheme) => (
              <View
                testID={`proxy-type-${scheme}`}
                role="menuitem"
                style={{
                  paddingHorizontal: 12,
                  paddingVertical: 8,
                  width: "100%",
                  display: "flex",
                  flexDirection: "row",
                  alignItems: "center",
                  justifyContent: "space-between",
                  backgroundColor:
                    state.proxy().proxy_type === scheme
                      ? paletteToken("accent_soft")
                      : "transparent",
                  cursor: "pointer",
                }}
                hoverStyle={
                  state.proxy().proxy_type === scheme
                    ? undefined
                    : { backgroundColor: paletteToken("panel_hover") }
                }
                onPress={() => {
                  state.setProxyTypeOpen(false);
                  state.commitProxy({ proxyType: scheme });
                }}
              >
                <Text
                  style={{
                    fontSize: 13,
                    fontFamily: MONO_FONT,
                    fontWeight: state.proxy().proxy_type === scheme ? "600" : "400",
                    color:
                      state.proxy().proxy_type === scheme
                        ? paletteToken("accent_fg")
                        : paletteToken("fg"),
                  }}
                >
                  {scheme.toUpperCase()}
                </Text>
                <If when={state.proxy().proxy_type === scheme}>
                  {() => <Icon name="check" size={13} color={paletteToken("accent_fg")} />}
                </If>
              </View>
            ))}
          </View>
        )}
      </If>
    </View>
  );
}

function ProxyInput(props: {
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
    >
      <Input
        testID={props.testID}
        style={{
          flex: 1,
          minWidth: 0,
          fontSize: 13,
          color: paletteToken("fg"),
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
