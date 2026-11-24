import { Text, View, signal, type JSX } from "nsh";
import { Icon } from "../chrome/icons";
import { Tip } from "../chrome/tooltip";
import { locale } from "../services/locale";
import { paletteToken } from "../services/themeMode";
import { toast } from "./events";
import { defaultSidebarGitHubBridge, type SidebarGitHubBridge } from "./githubAuthBridge";

function labelFor(
  state: string,
  login: string | null,
  polling: boolean,
  userCode: string | null,
): { text: string; tip: string } {
  const isZh = locale() === "zh-CN";
  if (polling) {
    const text = isZh ? "等待授权" : "Authorizing…";
    const tip = userCode
      ? isZh
        ? `等待 GitHub 授权 (${userCode})，点击查看`
        : `Waiting for GitHub authorization (${userCode})`
      : text;
    return { text, tip };
  }
  if (state === "connected") {
    const handle = login && login.trim() !== "" ? `@${login}` : "GitHub";
    return {
      text: handle,
      tip: isZh ? `${handle} · 点击打开共享频道` : `${handle} · Open shared channels`,
    };
  }
  if (state === "expired") {
    const text = isZh ? "已过期" : "Expired";
    return {
      text,
      tip: isZh ? "GitHub 会话已过期，点击重新登录" : "GitHub session expired, click to sign in",
    };
  }
  const text = isZh ? "未登录" : "Sign in";
  return {
    text,
    tip: isZh ? "未登录 GitHub，点击授权登录" : "Sign in with GitHub",
  };
}

function accentColorFor(state: string, polling: boolean): string {
  if (polling) return paletteToken("amber_fg");
  if (state === "connected") return paletteToken("accent_fg");
  if (state === "expired") return paletteToken("danger");
  return paletteToken("fg_muted");
}

function smallActionTip(kind: "refresh" | "logout"): string {
  const isZh = locale() === "zh-CN";
  if (kind === "refresh") return isZh ? "刷新 GitHub 会话" : "Refresh GitHub auth";
  return isZh ? "退出 GitHub 登录" : "Sign out of GitHub";
}

export function SidebarGitHubEntry(props: {
  collapsed: boolean;
  bridge?: SidebarGitHubBridge;
}): JSX.Element {
  const bridge = props.bridge ?? defaultSidebarGitHubBridge;
  const hovered = signal(false);
  const busy = signal(false);

  const summary = () => bridge.summary();
  const isPolling = () => bridge.polling() || summary().polling;
  const info = () =>
    labelFor(summary().state, summary().login, isPolling(), summary().userCode);
  const isConnected = () => !isPolling() && summary().state === "connected";
  const showRefresh = () => !isPolling() && (summary().state === "connected" || summary().state === "expired");

  const handlePrimaryPress = () => {
    if (isConnected()) {
      bridge.openChannelsWorkspace();
    } else {
      bridge.openSignIn();
    }
  };

  const handleRefresh = () => {
    if (busy()) return;
    busy.set(true);
    void bridge
      .refreshAuth()
      .catch((err: unknown) => toast(String(err), "error"))
      .finally(() => busy.set(false));
  };

  const handleLogout = () => {
    if (busy()) return;
    busy.set(true);
    void bridge
      .logout()
      .catch((err: unknown) => toast(String(err), "error"))
      .finally(() => busy.set(false));
  };

  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: props.collapsed ? "center" : "space-between",
        gap: 4,
        marginHorizontal: props.collapsed ? 6 : 8,
        marginBottom: 6,
        paddingHorizontal: props.collapsed ? 0 : 8,
        height: 30,
        borderRadius: 8,
        borderWidth: props.collapsed ? 0 : 1,
        borderColor: paletteToken("border_soft"),
        backgroundColor: props.collapsed ? "transparent" : paletteToken("panel"),
      }}
    >
      <View
        testID="sidebar-github-entry"
        role="button"
        accessibilityLabel={info().tip}
        tooltip={hovered() ? <Tip text={info().tip} /> : undefined}
        onMouseEnter={() => hovered.set(true)}
        onMouseLeave={() => hovered.set(false)}
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: props.collapsed ? "center" : "flex-start",
          gap: 6,
          flex: props.collapsed ? undefined : 1,
          minWidth: props.collapsed ? 28 : 0,
          width: props.collapsed ? 28 : undefined,
          height: props.collapsed ? 28 : "100%",
          borderRadius: 6,
          cursor: "pointer",
        }}
        hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
        onPress={handlePrimaryPress}
      >
        <Icon
          name="gitBranch"
          size={14}
          color={accentColorFor(summary().state, isPolling())}
        />
        <If when={!props.collapsed}>
          <Text
            testID="sidebar-github-status"
            style={{
              fontSize: 11,
              fontWeight: "500",
              color: isConnected()
                ? paletteToken("fg")
                : accentColorFor(summary().state, isPolling()),
              flex: 1,
              minWidth: 0,
              numberOfLines: 1,
            }}
          >
            {info().text}
          </Text>
        </If>
      </View>

      <If when={!props.collapsed && showRefresh()}>
        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 2, flexShrink: 0 }}>
          <View
            testID="sidebar-github-refresh"
            role="button"
            accessibilityLabel={smallActionTip("refresh")}
            style={{
              width: 20,
              height: 20,
              borderRadius: 5,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              cursor: busy() ? "default" : "pointer",
              opacity: busy() ? 0.5 : 1,
            }}
            hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
            onPress={handleRefresh}
          >
            <Icon name="refresh" size={12} color={paletteToken("fg_muted")} />
          </View>
          <If when={isConnected()}>
            <View
              testID="sidebar-github-logout"
              role="button"
              accessibilityLabel={smallActionTip("logout")}
              style={{
                width: 20,
                height: 20,
                borderRadius: 5,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                cursor: busy() ? "default" : "pointer",
                opacity: busy() ? 0.5 : 1,
              }}
              hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
              onPress={handleLogout}
            >
              <Icon name="x" size={12} color={paletteToken("fg_muted")} />
            </View>
          </If>
        </View>
      </If>
    </View>
  );
}
