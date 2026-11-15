import { Text, View, type JSX } from "nsh";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import { Icon } from "../../chrome/icons";
import { selectPage } from "../../shell/events";
import { clearFilters, github, onlyUpdates } from "./state";
import { openGitHubSignIn } from "./githubSignInSlot";

// 四套空状态 + 错误横幅 — my_skills/empty_state.rs + view.rs 对位：
// 1. 空库（render_empty_installed）：🧩 媒面 + 去市场 CTA；
// 2. 空搜索（render_empty_search）：搜索媒面 + 清除筛选（query 为空时
//    提示「没有符合条件的技能」，否则「请尝试其他搜索词」）；
// 3. 无待更新（render_empty_updates）：ok 勾媒面 + 显示全部；
// 4. 频道（render_channels_scope）：Layers 媒面 + 三态门（Unknown 静态 /
//    SignedOut 登录提示+按钮 / Connected 已登录行）。
// 登录对话框本体在 githubSignInSlot.tsx（github_sign_in.rs 对位）：本
// 文件只挂三态门与打开入口。

/** centered_fill (empty_state.rs)：填满滚动区居中，不是顶部一行字。 */
function CenteredFill(props: {
  children?: JSX.Element | (JSX.Element | null)[] | null;
}): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        flex: 1,
        width: "100%",
        minHeight: "100%",
        alignItems: "center",
        justifyContent: "center",
        gap: 12,
        padding: 20,
      }}
    >
      {props.children}
    </View>
  );
}

/** 圆片媒面（media_chip）：card 底 + border + glyph。 */
function MediaChip(props: { children: JSX.Element }): JSX.Element {
  return (
    <View
      style={{
        padding: 12,
        borderRadius: 999,
        backgroundColor: paletteToken("card"),
        borderWidth: 1,
        borderColor: paletteToken("border"),
      }}
    >
      {props.children}
    </View>
  );
}

function Title(props: { children: string }): JSX.Element {
  return (
    <Text style={{ fontSize: 18, fontWeight: "600", color: paletteToken("fg") }}>
      {props.children}
    </Text>
  );
}

function Desc(props: { children: string }): JSX.Element {
  return (
    <Text
      style={{
        maxWidth: 400,
        fontSize: 12,
        textAlign: "center",
        color: paletteToken("fg_muted"),
      }}
    >
      {props.children}
    </Text>
  );
}

function PrimaryButton(props: {
  testID: string;
  glyph?: "store" | "gitBranch";
  label: string;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 6,
        height: 32,
        paddingHorizontal: 12,
        borderRadius: 8,
        cursor: "pointer",
        backgroundColor: paletteToken("accent"),
      }}
      hoverStyle={{ backgroundColor: paletteToken("accent_hover") }}
      onPress={props.onPress}
    >
      {props.glyph ? <Icon name={props.glyph} size={14} color={paletteToken("on_accent")} /> : null}
      <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("on_accent") }}>
        {props.label}
      </Text>
    </View>
  );
}

/** 空库（empty_state.rs render_empty_installed）。 */
export function EmptyInstalled(): JSX.Element {
  return (
    <CenteredFill>
      <Text style={{ fontSize: 30 }}>🧩</Text>
      <Title>{t("emptyState.mySkillsTitle")}</Title>
      <Desc>{t("emptyState.mySkillsDesc")}</Desc>
      <PrimaryButton
        testID="my-skills-goto-marketplace"
        glyph="store"
        label={t("emptyState.mySkillsCta")}
        onPress={() => selectPage.publish("marketplace")}
      />
    </CenteredFill>
  );
}

/** 空搜索（empty_state.rs render_empty_search）。 */
export function EmptySearch(props: { query: string }): JSX.Element {
  const empty = props.query.trim().length === 0;
  return (
    <CenteredFill>
      <MediaChip>
        <Icon name="search" size={24} color={paletteToken("fg_muted")} />
      </MediaChip>
      <Title>{t("emptyState.noMatchingTitle")}</Title>
      <Desc>{empty ? t("mySkills.noMatching") : t("skillCards.tryDifferent")}</Desc>
      <View
        testID="my-skills-clear-search"
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 6,
          height: 28,
          paddingHorizontal: 12,
          borderRadius: 6,
          cursor: "pointer",
          borderWidth: 1,
          borderColor: paletteToken("border"),
          backgroundColor: paletteToken("card"),
        }}
        hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
        onPress={() => clearFilters()}
      >
        <Text style={{ fontSize: 12, color: paletteToken("fg") }}>
          {t("settings.clearAgentFilters")}
        </Text>
      </View>
    </CenteredFill>
  );
}

/** 无待更新（empty_state.rs render_empty_updates）。 */
export function EmptyUpdates(): JSX.Element {
  return (
    <CenteredFill>
      <MediaChip>
        <Icon name="check" size={24} color={paletteToken("ok")} />
      </MediaChip>
      <Title>{t("emptyState.upToDateTitle")}</Title>
      <Desc>{t("emptyState.upToDateDesc")}</Desc>
      <View
        testID="my-skills-show-all"
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          height: 28,
          paddingHorizontal: 12,
          borderRadius: 6,
          cursor: "pointer",
          borderWidth: 1,
          borderColor: paletteToken("border"),
          backgroundColor: paletteToken("card"),
        }}
        hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
        onPress={() => onlyUpdates.set(false)}
      >
        <Text style={{ fontSize: 12, color: paletteToken("fg") }}>
          {t("toolbar.showAllSkills")}
        </Text>
      </View>
    </CenteredFill>
  );
}

/** 频道三态门（empty_state.rs channels_gate）：Unknown 静态；SignedOut
 * 提示+登录按钮（打开 github_sign_in.rs 对位的设备授权对话框）；
 * Connected 已登录行。 */
export function ChannelsScope(): JSX.Element {
  return (
    <CenteredFill>
      <MediaChip>
        <Icon name="layers" size={28} color={paletteToken("violet")} />
      </MediaChip>
      <Title>{t("emptyState.channelsTitle")}</Title>
      <Desc>{t("emptyState.channelsDesc")}</Desc>
      {(() => {
        // 派生读取落在 JSX 表达式里（github signal 的响应式绑定）。
        const status = github();
        if (status === null) return null;
        if (status.state === "connected") {
          return (
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
              {tf("sharedChannels.signedInAs", { login: status.login ?? "" })}
            </Text>
          );
        }
        return (
          <View style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 12 }}>
            <Desc>{t("sharedChannels.signInHint")}</Desc>
            <PrimaryButton
              testID="channels-github-sign-in"
              glyph="gitBranch"
              label={t("sharedChannels.signInButton")}
              onPress={openGitHubSignIn}
            />
          </View>
        );
      })()}
    </CenteredFill>
  );
}

/** 列表读取失败横幅（view.rs error 分支：danger 弱底卡居中）。 */
export function ErrorBanner(props: { error: string }): JSX.Element {
  return (
    <CenteredFill>
      <View
        testID="my-skills-error"
        style={{
          maxWidth: 420,
          padding: 16,
          borderRadius: 8,
          backgroundColor: paletteToken("danger_bg"),
          borderWidth: 1,
          borderColor: paletteToken("danger_border"),
        }}
      >
        <Text
          style={{
            fontSize: 12,
            textAlign: "center",
            color: paletteToken("danger_fg"),
          }}
        >
          {tf("settings.connectionFailed", { error: props.error })}
        </Text>
      </View>
    </CenteredFill>
  );
}
