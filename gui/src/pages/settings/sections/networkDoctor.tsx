// 「网络诊断」模块 — 作为 NetworkSection 的右上角动作 + 独立诊断报告卡，
// 同时保留 DoctorWorkbench 供 networkDoctor.test.tsx 独立测试。

import { Text, View, signal, type JSX, type Signal } from "nsh";
import { SettingsNetwork } from "skillstar-gui/native";
import { paletteToken } from "../../../services/themeMode";
import { t } from "../../../services/translation";
import { Icon, type IconName } from "../../../chrome/icons";
import { IconSpin } from "../../../chrome/feedback";
import {
  MONO_FONT,
  SectionShell,
  SettingsButton,
  StatusPill,
  card,
  type StatusTone,
} from "../parts";
import { NETWORK_DOCTOR_SECTION } from "../sections";

export interface HostCheckDto {
  id: string;
  label: string;
  url: string;
  /** "ok" | "fail" | "skip"。 */
  status: string;
  latency_ms: number | null;
  detail: string | null;
}

export interface DiagnosisDto {
  proxy_enabled: boolean;
  proxy_type: string | null;
  checks: HostCheckDto[];
  recommendations: string[];
}

function asHostCheck(value: unknown): HostCheckDto {
  const raw = (value ?? {}) as Partial<HostCheckDto>;
  return {
    id: typeof raw.id === "string" ? raw.id : "",
    label: typeof raw.label === "string" ? raw.label : "",
    url: typeof raw.url === "string" ? raw.url : "",
    status: typeof raw.status === "string" ? raw.status : "skip",
    latency_ms: typeof raw.latency_ms === "number" ? raw.latency_ms : null,
    detail: typeof raw.detail === "string" ? raw.detail : null,
  };
}

function asDiagnosis(value: unknown): DiagnosisDto {
  const raw = (value ?? {}) as Partial<DiagnosisDto>;
  return {
    proxy_enabled: raw.proxy_enabled === true,
    proxy_type: typeof raw.proxy_type === "string" ? raw.proxy_type : null,
    checks: Array.isArray(raw.checks) ? raw.checks.map(asHostCheck) : [],
    recommendations: Array.isArray(raw.recommendations)
      ? raw.recommendations.filter((key): key is string => typeof key === "string")
      : [],
  };
}

export interface DoctorBridge {
  diagnose(): Promise<DiagnosisDto>;
}

/** 真桥：native facade 直调（页面默认使用）。 */
export const realDoctorBridge: DoctorBridge = {
  diagnose() {
    return SettingsNetwork.diagnoseNetwork().then(asDiagnosis);
  },
};

/** 机器键 → i18n key；未知键原样透传。 */
export function recommendationKey(key: string): string {
  switch (key) {
    case "enable_github_mirrors":
      return "settings.networkDoctorRecEnableMirrors";
    case "enable_proxy":
      return "settings.networkDoctorRecEnableProxy";
    case "use_socks5h":
      return "settings.networkDoctorRecUseSocks5h";
    case "check_proxy_reachability":
      return "settings.networkDoctorRecCheckProxy";
    case "use_marketplace_wrap":
      return "settings.networkDoctorRecMarketplaceWrap";
    case "all_github_paths_blocked":
      return "settings.networkDoctorRecAllBlocked";
    default:
      return key;
  }
}

export interface DoctorState {
  diagnosing: Signal<boolean>;
  diagnosis: Signal<DiagnosisDto | null>;
  error: Signal<string | null>;
  run(): void;
  dismiss(): void;
}

export function createDoctorState(bridge: DoctorBridge): DoctorState {
  const diagnosing = signal(false);
  const diagnosis = signal<DiagnosisDto | null>(null);
  const error = signal<string | null>(null);
  return {
    diagnosing,
    diagnosis,
    error,
    run() {
      if (diagnosing()) return;
      diagnosing.set(true);
      error.set(null);
      bridge.diagnose().then(
        (result) => {
          diagnosis.set(result);
          diagnosing.set(false);
        },
        (reason: unknown) => {
          error.set(String(reason));
          diagnosing.set(false);
        }
      );
    },
    dismiss() {
      if (diagnosing()) return;
      diagnosis.set(null);
      error.set(null);
    },
  };
}

/** 页面模块级单例，供 NetworkSection 消费。 */
export const doctorPageState = createDoctorState(realDoctorBridge);

/** 独立工作台（供 networkDoctor.test.tsx 独立运行）。 */
export function DoctorWorkbench(props: { state: DoctorState }): JSX.Element {
  const state = props.state;
  return (
    <SectionShell section={NETWORK_DOCTOR_SECTION} action={<DoctorRunButton state={state} />}>
      <DoctorBody state={state} />
    </SectionShell>
  );
}

/** 独立诊断报告卡：空闲时不渲染，运行中或有结果/错误时渲染独立 card()。 */
export function DoctorInlinePanel(props: { state: DoctorState }): JSX.Element | null {
  const state = props.state;
  return (
    <If when={state.diagnosing() || state.error() !== null || state.diagnosis() !== null}>
      {() => (
        <View style={card({ display: "flex", flexDirection: "column" })}>
          <View
            style={{
              paddingHorizontal: 18,
              paddingVertical: 11,
              borderBottomWidth: 1,
              borderBottomColor: paletteToken("border_soft"),
              backgroundColor: paletteToken("panel"),
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              justifyContent: "space-between",
              gap: 12,
              width: "100%",
            }}
          >
            <View
              style={{
                display: "flex",
                flexDirection: "row",
                alignItems: "center",
                gap: 8,
                minWidth: 0,
                flex: 1,
              }}
            >
              <Icon name="activity" size={14} color={paletteToken("accent")} />
              <Text
                style={{
                  fontSize: 13,
                  fontWeight: "600",
                  color: paletteToken("fg"),
                  numberOfLines: 1,
                }}
              >
                {t("settings.networkDoctor")}
              </Text>
              <DoctorSummaryPill state={state} />
            </View>
            <If when={!state.diagnosing()}>
              {() => (
                <View
                  testID="doctor-dismiss"
                  role="button"
                  style={{
                    width: 24,
                    height: 24,
                    borderRadius: 6,
                    flexShrink: 0,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    cursor: "pointer",
                  }}
                  hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
                  onPress={() => state.dismiss()}
                >
                  <Icon name="x" size={14} color={paletteToken("fg_muted")} />
                </View>
              )}
            </If>
          </View>
          <If when={state.diagnosing() && state.error() === null && state.diagnosis() === null}>
            {() => (
              <View
                style={{
                  paddingHorizontal: 18,
                  paddingVertical: 14,
                  display: "flex",
                  flexDirection: "row",
                  alignItems: "center",
                  gap: 10,
                  width: "100%",
                }}
              >
                <IconSpin name="loader" size={14} color={paletteToken("accent")} spin />
                <Text
                  style={{
                    flex: 1,
                    minWidth: 0,
                    fontSize: 12,
                    color: paletteToken("fg_muted"),
                    lineHeight: 18,
                  }}
                >
                  {t("settings.networkDoctorHint")}
                </Text>
              </View>
            )}
          </If>
          <If when={state.error() !== null}>
            {() => <DoctorErrorBox error={state.error() ?? ""} />}
          </If>
          <If when={state.diagnosis() !== null}>
            {() => <DoctorFindings state={state} />}
          </If>
        </View>
      )}
    </If>
  );
}

function summaryBadge(state: DoctorState): {
  tone: StatusTone;
  icon: IconName;
  mono: boolean;
  text: string;
} {
  if (state.diagnosing()) {
    return { tone: "accent", icon: "loader", mono: false, text: t("common.loading") };
  }
  if (state.error() !== null) {
    return { tone: "danger", icon: "xCircle", mono: false, text: t("common.failed") };
  }
  const checks = state.diagnosis()?.checks ?? [];
  if (checks.length === 0) {
    return { tone: "ok", icon: "checkCircle", mono: false, text: t("common.done") };
  }
  const okCount = checks.filter((item) => item.status === "ok").length;
  const failCount = checks.filter((item) => item.status === "fail").length;
  const tone: StatusTone = failCount === 0 ? "ok" : okCount === 0 ? "danger" : "warn";
  const icon: IconName =
    failCount === 0 ? "checkCircle" : okCount === 0 ? "xCircle" : "alertTriangle";
  return { tone, icon, mono: true, text: `${okCount}/${checks.length}` };
}

function DoctorSummaryPill(props: { state: DoctorState }): JSX.Element {
  const state = props.state;
  return (
    <StatusPill
      tone={summaryBadge(state).tone}
      icon={summaryBadge(state).icon}
      mono={summaryBadge(state).mono}
      text={summaryBadge(state).text}
    />
  );
}

/** 分区头「开始诊断」按钮：空闲带 activity 图标，运行中转 loader 并吞点击。 */
export function DoctorRunButton(props: { state: DoctorState }): JSX.Element {
  const state = props.state;
  return (
    <SettingsButton
      testID="doctor-run"
      label={t("settings.networkDoctorRun")}
      icon={state.diagnosing() ? "loader" : "activity"}
      spinning={state.diagnosing()}
      busy={state.diagnosing()}
      variant="accent"
      size="sm"
      onPress={() => state.run()}
    />
  );
}

/** 独立工作台正文（保留 hint 行供 standalone 测例检查）。 */
function DoctorBody(props: { state: DoctorState }): JSX.Element {
  const state = props.state;
  return (
    <View style={{ display: "flex", flexDirection: "column", width: "100%" }}>
      <View
        style={{
          paddingHorizontal: 18,
          paddingVertical: 13,
          width: "100%",
          backgroundColor:
            state.error() !== null || state.diagnosis() !== null
              ? paletteToken("panel")
              : paletteToken("card"),
          borderBottomWidth: state.error() !== null || state.diagnosis() !== null ? 1 : 0,
          borderBottomColor: paletteToken("border_soft"),
        }}
      >
        <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), lineHeight: 18 }}>
          {t("settings.networkDoctorHint")}
        </Text>
      </View>
      <If when={state.error() !== null}>
        {() => <DoctorErrorBox error={state.error() ?? ""} />}
      </If>
      <If when={state.diagnosis() !== null}>
        {() => <DoctorFindings state={state} />}
      </If>
    </View>
  );
}

function DoctorErrorBox(props: { error: string }): JSX.Element {
  return (
    <View style={{ paddingHorizontal: 18, paddingVertical: 12, width: "100%" }}>
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
          borderColor: paletteToken("danger_border"),
          backgroundColor: paletteToken("danger_bg"),
          width: "100%",
        }}
      >
        <Icon name="xCircle" size={14} color={paletteToken("danger")} />
        <Text
          style={{
            flex: 1,
            minWidth: 0,
            fontSize: 12,
            color: paletteToken("danger_fg"),
            lineHeight: 18,
          }}
        >
          {props.error}
        </Text>
      </View>
    </View>
  );
}

function DoctorFindings(props: { state: DoctorState }): JSX.Element {
  const state = props.state;
  return (
    <View style={{ display: "flex", flexDirection: "column", width: "100%" }}>
      <View style={{ display: "flex", flexDirection: "column", width: "100%" }}>
        {(state.diagnosis()?.checks ?? []).map((check, index) => (
          <CheckRow check={check} borderTop={index > 0} />
        ))}
      </View>
      <If when={(state.diagnosis()?.recommendations ?? []).length > 0}>
        {() => (
          <View
            style={{
              paddingHorizontal: 18,
              paddingTop: (state.diagnosis()?.checks.length ?? 0) > 0 ? 6 : 12,
              paddingBottom: 14,
              width: "100%",
            }}
          >
            <View
              style={{
                borderRadius: 10,
                borderWidth: 1,
                borderColor: paletteToken("warn_border"),
                backgroundColor: paletteToken("warn_bg"),
                paddingHorizontal: 12,
                paddingVertical: 10,
                display: "flex",
                flexDirection: "column",
                gap: 4,
                width: "100%",
              }}
            >
              <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6 }}>
                <Icon name="alertTriangle" size={13} color={paletteToken("warn")} />
                <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("warn") }}>
                  {t("settings.networkDoctorRecommendations")}
                </Text>
              </View>
              {(state.diagnosis()?.recommendations ?? []).map((key) => (
                <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), lineHeight: 18 }}>
                  {`• ${t(recommendationKey(key))}`}
                </Text>
              ))}
            </View>
          </View>
        )}
      </If>
    </View>
  );
}

function CheckRow(props: { check: HostCheckDto; borderTop: boolean }): JSX.Element {
  const statusIcon = (): { name: IconName; color: string } => {
    if (props.check.status === "ok") {
      return { name: "checkCircle", color: paletteToken("ok") };
    }
    if (props.check.status === "fail") {
      return { name: "xCircle", color: paletteToken("danger") };
    }
    return { name: "circleAlert", color: paletteToken("fg_muted") };
  };
  const pill = () => checkTone(props.check);
  return (
    <View
      style={{
        paddingHorizontal: 18,
        paddingVertical: 11,
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 12,
        width: "100%",
        ...(props.borderTop
          ? { borderTopWidth: 1, borderTopColor: paletteToken("border_soft") }
          : {}),
      }}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 10,
          flex: 1,
          minWidth: 0,
        }}
      >
        <Icon name={statusIcon().name} size={15} color={statusIcon().color} />
        <View style={{ minWidth: 0, flex: 1, display: "flex", flexDirection: "column", gap: 2 }}>
          <Text
            style={{
              fontSize: 13,
              fontWeight: "500",
              color: paletteToken("fg"),
              numberOfLines: 1,
            }}
          >
            {props.check.label}
          </Text>
          <If
            when={
              props.check.url !== "" ||
              (props.check.detail !== null && props.check.detail !== "")
            }
          >
            {() => (
              <View
                style={{
                  display: "flex",
                  flexDirection: "row",
                  flexWrap: "wrap",
                  alignItems: "center",
                  gap: 8,
                  minWidth: 0,
                }}
              >
                <If when={props.check.url !== ""}>
                  {() => (
                    <Text
                      style={{
                        fontSize: 12,
                        fontFamily: MONO_FONT,
                        color: paletteToken("fg_muted"),
                        numberOfLines: 1,
                        flexShrink: 1,
                        minWidth: 0,
                      }}
                    >
                      {props.check.url}
                    </Text>
                  )}
                </If>
                <If when={props.check.detail !== null && props.check.detail !== ""}>
                  {() => (
                    <Text
                      style={{
                        fontSize: 12,
                        fontFamily: MONO_FONT,
                        color:
                          props.check.status === "fail"
                            ? paletteToken("danger_fg")
                            : paletteToken("fg_muted"),
                        numberOfLines: 1,
                        flexShrink: 1,
                        minWidth: 0,
                      }}
                    >
                      {props.check.detail}
                    </Text>
                  )}
                </If>
              </View>
            )}
          </If>
        </View>
      </View>
      <StatusPill tone={pill().tone} mono text={pill().label} />
    </View>
  );
}

function checkTone(check: HostCheckDto): { tone: StatusTone; label: string } {
  if (check.status === "ok") {
    return {
      tone: "ok",
      label: check.latency_ms !== null ? `${check.latency_ms}ms` : "ok",
    };
  }
  if (check.status === "fail") {
    return { tone: "danger", label: "fail" };
  }
  return { tone: "neutral", label: check.status };
}
