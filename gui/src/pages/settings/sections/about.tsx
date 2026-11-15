// 「关于」分区 UI — 状态机与桥接在 ./aboutState.ts（保持单文件 < 800 行）。

import { Image, Text, View, effect, openUrl, type JSX } from "nsh";
import os from "os";
import { Settings } from "skillstar-gui/native";
import { paletteToken, type PaletteToken } from "../../../services/themeMode";
import { t, tf } from "../../../services/translation";
import { Icon, type IconName } from "../../../chrome/icons";
import {
  card,
  MONO_FONT,
  SectionShell,
  SettingsButton,
  SettingsGroup,
  StatusPill,
} from "../parts";
import { SETTINGS_SECTIONS } from "../sections";
import { st } from "../settingsStrings";
import {
  GH_INSTALL_PAGE_URL,
  RELEASES_PAGE_URL,
  createAboutState,
  displayHome,
  platformKey,
  platformLabel,
  realAboutBridge,
  type AboutState,
} from "./aboutState";

export {
  createAboutState,
  displayHome,
  platformKey,
  platformLabel,
  realAboutBridge,
  type AboutBridge,
  type AboutSnapshotDto,
  type AboutState,
  type GitStatusDto,
  type ReleaseOutcomeDto,
  type ReleaseRecordDto,
} from "./aboutState";

const BRAND_ICON_URI = Settings.brandIconDataUri();

// ---- 页面（SectionShell grouped 双卡栈） -------------------------------------

const pageState = createAboutState(realAboutBridge);

export function AboutSection(): JSX.Element {
  return <AboutWorkbench state={pageState} />;
}

export function AboutWorkbench(props: { state: AboutState }): JSX.Element {
  const state = props.state;
  effect(() => state.start());
  return (
    <SectionShell
      section={SETTINGS_SECTIONS.find((section) => section.id === "about")!}
      grouped
    >
      <>
        <View style={card()}>
          <VersionRow state={state} />
        </View>
        <SettingsGroup title={st("settings.group.toolchain")}>
          <View style={{ display: "flex", flexDirection: "column", width: "100%" }}>
            <GitRow state={state} />
            <GhRow state={state} />
            <DataPathRow state={state} />
          </View>
        </SettingsGroup>
      </>
    </SectionShell>
  );
}

// ---- 卡 1：品牌与版本发布英雄卡 -----------------------------------------------

function VersionRow(props: { state: AboutState }): JSX.Element {
  const state = props.state;
  return (
    <View
      style={{
        paddingHorizontal: 18,
        paddingVertical: 18,
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 14,
        flexWrap: "wrap",
      }}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 14,
          minWidth: 0,
        }}
      >
        <View
          style={{
            width: 48,
            height: 48,
            borderRadius: 12,
            borderWidth: 1,
            borderColor: paletteToken("border"),
            backgroundColor: paletteToken("well"),
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            flexShrink: 0,
          }}
        >
          <Image
            src={BRAND_ICON_URI}
            fadeDuration={0}
            style={{
              width: 38,
              height: 38,
              borderRadius: 9,
              objectFit: "cover",
            }}
          />
        </View>
        <View style={{ display: "flex", flexDirection: "column", gap: 3, minWidth: 0 }}>
          <View
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              gap: 8,
              flexWrap: "wrap",
            }}
          >
            <Text style={{ fontSize: 18, fontWeight: "600", color: paletteToken("fg") }}>
              SkillStar
            </Text>
            <StatusPill text={`v${state.snapshot()?.version ?? ""}`} tone="accent" mono />
          </View>
          <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), lineHeight: 18 }}>
            {t("sidebar.tagline")}
          </Text>
        </View>
      </View>
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "flex-end",
          gap: 8,
          flexWrap: "wrap",
        }}
      >
        <ReleaseChip state={state} />
        <View
          testID="about-releases"
          role="button"
          style={{
            width: 30,
            height: 30,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            borderRadius: 9,
            borderWidth: 1,
            borderColor: paletteToken("border"),
            backgroundColor: paletteToken("card"),
            cursor: "pointer",
          }}
          hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
          activeStyle={{ backgroundColor: paletteToken("well") }}
          onPress={() => {
            void openUrl(RELEASES_PAGE_URL);
          }}
        >
          <Icon name="externalLink" size={14} color={paletteToken("fg_muted")} />
        </View>
        <SettingsButton
          testID="about-check-update"
          label={state.releaseChecking() ? t("settings.checkingUpdate") : t("settings.checkUpdate")}
          icon={state.releaseChecking() ? "loader" : "refresh"}
          spinning={state.releaseChecking()}
          variant="secondary"
          size="sm"
          busy={state.releaseChecking()}
          onPress={() => state.runReleaseCheck()}
        />
      </View>
    </View>
  );
}

/**
 * release_check_chip：分支在内容函数里求值、于 JSX 表达式处调用——
 * 组件体内直接 return 三元会把信号读取冻在首渲染。
 */
function ReleaseChip(props: { state: AboutState }): JSX.Element {
  const state = props.state;
  return <>{releaseChipContent(state)}</>;
}

function releaseChipContent(state: AboutState): JSX.Element | null {
  const outcome = state.releaseCheck()?.outcome ?? null;
  if (state.releaseChecking()) {
    return <StatusPill text={t("settings.checkingUpdate")} tone="neutral" />;
  }
  if (outcome?.kind === "up_to_date") {
    return <StatusPill text={t("settings.upToDate")} tone="ok" icon="check" />;
  }
  if (outcome?.kind === "no_published_release") {
    return <StatusPill text={t("settings.noPublishedRelease")} tone="neutral" />;
  }
  if (outcome?.kind === "failed") {
    return <StatusPill text={t("settings.updateCheckFailed")} tone="danger" />;
  }
  if (outcome?.kind === "available") {
    return (
      <View
        testID="about-update-available"
        role="button"
        style={{
          height: 24,
          paddingHorizontal: 9,
          borderRadius: 999,
          borderWidth: 1,
          borderColor: paletteToken("accent_soft_edge"),
          backgroundColor: paletteToken("accent_soft"),
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 5,
          cursor: "pointer",
        }}
        hoverStyle={{ backgroundColor: paletteToken("card_active_hover") }}
        activeStyle={{ backgroundColor: paletteToken("panel_active") }}
        onPress={() => {
          if (outcome.kind === "available") void openUrl(outcome.url);
        }}
      >
        <Icon name="download" size={12} color={paletteToken("accent_fg")} />
        <Text style={{ fontSize: 11, fontWeight: "600", color: paletteToken("accent_fg") }}>
          {tf("settings.updateFoundDesc", { version: outcome.tag })}
        </Text>
      </View>
    );
  }
  return null;
}

// ---- 卡 2：环境与工具链诊断 --------------------------------------------------

type ToolStateTone = "ok" | "danger" | "neutral";

const TOOL_TONE: Record<
  ToolStateTone,
  { bg: PaletteToken; border: PaletteToken; icon: PaletteToken }
> = {
  ok: { bg: "ok_bg", border: "ok_border", icon: "ok" },
  danger: { bg: "danger_bg", border: "danger_border", icon: "danger" },
  neutral: { bg: "well", border: "border", icon: "fg_muted" },
};

function ToolIconBox(props: { icon: IconName; stateTone: ToolStateTone }): JSX.Element {
  const tokens = () => TOOL_TONE[props.stateTone];
  return (
    <View
      style={{
        width: 36,
        height: 36,
        borderRadius: 10,
        flexShrink: 0,
        borderWidth: 1,
        borderColor: paletteToken(tokens().border),
        backgroundColor: paletteToken(tokens().bg),
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      <Icon name={props.icon} size={16} color={paletteToken(tokens().icon)} />
    </View>
  );
}

/** git_row：整个内容块在 JSX 表达式里求值（snapshot 变化即重算）。 */
function GitRow(props: { state: AboutState }): JSX.Element {
  const state = props.state;
  return (
    <View style={{ display: "flex", flexDirection: "column", width: "100%" }}>
      {gitRowContent(state)}
    </View>
  );
}

function gitRowContent(state: AboutState): JSX.Element {
  const status = state.snapshot()?.git_status ?? null;
  const tone: ToolStateTone =
    status?.status === "Installed"
      ? "ok"
      : status?.status === "NotInstalled"
        ? "danger"
        : "neutral";
  return (
    <>
      <View
        style={{
          paddingHorizontal: 18,
          paddingVertical: 14,
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 12,
        }}
      >
        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 12 }}>
          <ToolIconBox icon="gitBranch" stateTone={tone} />
          <Text style={{ fontSize: 14, fontWeight: "500", color: paletteToken("fg") }}>Git</Text>
        </View>
        {status?.status === "Installed" ? (
          <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
            {status.version ? (
              <Text
                style={{
                  fontSize: 12,
                  fontFamily: MONO_FONT,
                  fontVariant: ["tabular-nums"],
                  color: paletteToken("fg_muted"),
                }}
              >
                {status.version}
              </Text>
            ) : null}
            <StatusPill text={t("settings.gitInstalled")} tone="ok" icon="check" />
          </View>
        ) : status?.status === "NotInstalled" ? (
          <SettingsButton
            testID="git-install"
            label={t("settings.gitInstall")}
            icon="externalLink"
            variant="secondary"
            size="sm"
            onPress={() => {
              void openUrl(status.download_url);
            }}
          />
        ) : null}
      </View>
      {status?.status === "NotInstalled" && status.install_instructions.length > 0 ? (
        <View
          style={{
            paddingHorizontal: 18,
            paddingVertical: 12,
            display: "flex",
            flexDirection: "column",
            gap: 8,
            backgroundColor: paletteToken("well"),
            borderTopWidth: 1,
            borderTopColor: paletteToken("border_soft"),
          }}
        >
          <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6 }}>
            <Icon name="terminal" size={14} color={paletteToken("fg_muted")} />
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
              {tf("settings.gitInstallCommandLabel", { os: status.os })}
            </Text>
          </View>
          {status.install_instructions.map((instruction) => (
            <CommandRow
              state={state}
              id={`git-${instruction.label}`}
              label={instruction.label}
              command={instruction.command}
            />
          ))}
        </View>
      ) : null}
    </>
  );
}

/** gh_row：安装命令取 facade 快照的 gh_install_command。 */
function GhRow(props: { state: AboutState }): JSX.Element {
  const state = props.state;
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        width: "100%",
        borderTopWidth: 1,
        borderTopColor: paletteToken("border_soft"),
      }}
    >
      {ghRowContent(state)}
    </View>
  );
}

function ghRowContent(state: AboutState): JSX.Element {
  const snapshot = state.snapshot();
  const installed = snapshot?.gh_installed === true;
  const key = platformKey(os.platform());
  const command = snapshot?.gh_install_command ?? null;
  const tone: ToolStateTone = installed ? "ok" : "danger";
  return (
    <>
      <View
        style={{
          paddingHorizontal: 18,
          paddingVertical: 14,
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 12,
        }}
      >
        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 12 }}>
          <ToolIconBox icon="terminal" stateTone={tone} />
          <Text style={{ fontSize: 14, fontWeight: "500", color: paletteToken("fg") }}>
            GitHub CLI
          </Text>
        </View>
        {installed ? (
          <StatusPill text={t("settings.ghInstalled")} tone="ok" icon="check" />
        ) : (
          <SettingsButton
            testID="gh-install"
            label={t("settings.ghInstall")}
            icon="externalLink"
            variant="secondary"
            size="sm"
            onPress={() => {
              void openUrl(GH_INSTALL_PAGE_URL);
            }}
          />
        )}
      </View>
      {!installed ? (
        <View
          style={{
            paddingHorizontal: 18,
            paddingVertical: 12,
            backgroundColor: paletteToken("well"),
            borderTopWidth: 1,
            borderTopColor: paletteToken("border_soft"),
            display: "flex",
            flexDirection: "column",
            gap: 8,
          }}
        >
          <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6 }}>
            <Icon name="terminal" size={14} color={paletteToken("fg_muted")} />
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
              {tf("settings.ghInstallCommandLabel", {
                platform: t(`settings.ghInstallPlatform_${key}`),
              })}
            </Text>
          </View>
          {command !== null ? (
            <CommandRow state={state} id="gh" label="" command={command} />
          ) : (
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
              {t("settings.ghInstallCommandUnavailable")}
            </Text>
          )}
        </View>
      ) : null}
    </>
  );
}

/** command_row：前导 `$ ` 提示符 + 等宽命令行 + 复制钮（copied 态在 JSX 里读）。 */
function CommandRow(props: {
  state: AboutState;
  id: string;
  label: string;
  command: string;
}): JSX.Element {
  const state = props.state;
  return (
    <View
      style={{
        borderRadius: 8,
        borderWidth: 1,
        borderColor: paletteToken("border_soft"),
        backgroundColor: paletteToken("panel"),
        paddingHorizontal: 12,
        paddingVertical: 8,
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 10,
      }}
    >
      <View style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 3 }}>
        {props.label !== "" ? (
          <Text style={{ fontSize: 11, fontWeight: "500", color: paletteToken("fg_muted") }}>
            {props.label}
          </Text>
        ) : null}
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 6,
            minWidth: 0,
          }}
        >
          <Text
            style={{
              fontSize: 12,
              fontFamily: MONO_FONT,
              color: paletteToken("fg_faint"),
              flexShrink: 0,
            }}
          >
            $
          </Text>
          <Text
            style={{
              fontSize: 12,
              fontFamily: MONO_FONT,
              color: paletteToken("fg"),
              flex: 1,
              minWidth: 0,
            }}
          >
            {props.command}
          </Text>
        </View>
      </View>
      <View
        testID={`copy-${props.id}`}
        role="button"
        style={{
          width: 28,
          height: 28,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          borderRadius: 7,
          flexShrink: 0,
          cursor: "pointer",
        }}
        hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
        activeStyle={{ backgroundColor: paletteToken("border_soft") }}
        onPress={() => state.copyCommand(props.id, props.command)}
      >
        <Icon
          name={state.copied() === props.id ? "check" : "copy"}
          size={14}
          color={state.copied() === props.id ? paletteToken("ok") : paletteToken("fg_muted")}
        />
      </View>
    </View>
  );
}

/** data_path_row：平台胶囊 + 等宽路径（~ 折算）+ 打开钮。 */
function DataPathRow(props: { state: AboutState }): JSX.Element {
  const state = props.state;
  return (
    <View
      style={{
        paddingHorizontal: 18,
        paddingVertical: 14,
        borderTopWidth: 1,
        borderTopColor: paletteToken("border_soft"),
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 12,
      }}
    >
      <ToolIconBox icon="hardDrive" stateTone="neutral" />
      <View
        style={{
          flex: 1,
          minWidth: 0,
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 8,
        }}
      >
        <StatusPill text={platformLabel(os.platform())} tone="neutral" />
        <Text
          style={{
            flex: 1,
            minWidth: 0,
            fontSize: 12,
            fontFamily: MONO_FONT,
            color: paletteToken("fg"),
            numberOfLines: 1,
          }}
        >
          {displayHome(state.snapshot()?.data_root ?? "", os.homedir())}
        </Text>
      </View>
      <View
        testID="about-open-data"
        role="button"
        style={{
          width: 30,
          height: 30,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          borderRadius: 8,
          borderWidth: 1,
          borderColor: paletteToken("border_soft"),
          backgroundColor: paletteToken("well"),
          flexShrink: 0,
          cursor: "pointer",
        }}
        hoverStyle={{
          backgroundColor: paletteToken("panel_hover"),
          borderColor: paletteToken("border"),
        }}
        activeStyle={{ backgroundColor: paletteToken("border_soft") }}
        onPress={() => state.openDataRoot()}
      >
        <Icon name="folderOpen" size={14} color={paletteToken("fg_muted")} />
      </View>
    </View>
  );
}
