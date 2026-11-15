// 添加账号三标签弹窗（OAuth 授权 | Token / JSON | 本地导入）。
// 对齐 cockpit-tools 顶部多方式切换体验：不再先进入「选择菜单」，打开即直达默认方式，
// 默认落到哪个 Tab 完全由后端 AgentRow.supportsOauth 决定。
// 保留全部已有 testID，新增 acct-tab-oauth / acct-tab-local / acct-add-copy-url。

import { Overlay, Text, View, openUrl, type JSX } from "nsh";
import { Input } from "nsh/base";
import { AgentIcon } from "../../chrome/agentRail";
import { promptVeil, SHEET_SHADOW } from "../../chrome/dialog";
import { IconSpin } from "../../chrome/feedback";
import { Icon, type IconName } from "../../chrome/icons";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import { isFormError, type AccountsState, type AddStep } from "./state";
import { flowTakesPaste, type AgentRow, type LoginStart } from "./types";

function currentAgentRow(state: AccountsState): AgentRow | undefined {
  const id = state.add.agent();
  if (!id) return undefined;
  return state.agents().find((a) => a.id === id);
}

function agentDisplayName(state: AccountsState): string {
  const id = state.add.agent();
  if (!id) return "";
  return currentAgentRow(state)?.displayName ?? id;
}

function supportsOauth(state: AccountsState): boolean {
  return currentAgentRow(state)?.supportsOauth !== false;
}

function localSourcePath(state: AccountsState): string {
  return currentAgentRow(state)?.localSource?.trim() ?? "";
}

/** 居中模态遮罩与三标签表单对话框。 */
export function AddAccountDialog(props: { state: AccountsState }): JSX.Element {
  const state = props.state;
  return (
    <View style={{ display: state.add.agent() ? "flex" : "none" }}>
      {state.add.agent() ? (
        <Overlay
          style={{ position: "fixed", top: 0, left: 0, right: 0, bottom: 0, zIndex: 60 }}
          onCancel={() => state.closeAdd()}
        >
          <View
            testID="acct-add-veil"
            style={{
              position: "absolute",
              top: 0,
              left: 0,
              right: 0,
              bottom: 0,
              backgroundColor: promptVeil(),
            }}
            onPress={() => state.closeAdd()}
          />

          <View
            style={{
              position: "absolute",
              top: 0,
              left: 0,
              right: 0,
              bottom: 0,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              padding: 20,
            }}
          >
            <View
              testID="acct-add-card"
              style={{
                display: "flex",
                flexDirection: "column",
                gap: 14,
                width: 488,
                maxWidth: "100%",
                padding: 18,
                borderRadius: 16,
                borderWidth: 1,
                borderColor: paletteToken("border"),
                backgroundColor: paletteToken("card"),
                boxShadow: SHEET_SHADOW,
              }}
              onPress={() => {
                // 吞掉弹窗内部点击，防止穿透到 veil 关闭弹窗。
              }}
            >
              <DialogHeader state={state} />
              <AddMethodTabs state={state} />

              {state.add.error() ? <DialogError text={state.add.error() ?? ""} /> : null}

              {state.add.step() === "oauth" ? (
                <OAuthTabBody state={state} />
              ) : state.add.step() === "token" ? (
                <TokenTabBody state={state} />
              ) : (
                <LocalTabBody state={state} />
              )}
            </View>
          </View>
        </Overlay>
      ) : null}
    </View>
  );
}

function DialogHeader(props: { state: AccountsState }): JSX.Element {
  const state = props.state;
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 10,
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
            fontSize: 16,
            fontWeight: "600",
            color: paletteToken("fg"),
          }}
        >
          {t("accountsPage.addAccount")}
        </Text>
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 5,
            height: 22,
            paddingHorizontal: 8,
            borderRadius: 999,
            borderWidth: 1,
            borderColor: paletteToken("border"),
            backgroundColor: paletteToken("well"),
          }}
        >
          <AgentIcon
            testID="acct-add-agent-icon"
            id={state.add.agent() ?? "codex"}
            size={14}
          />
          <Text
            style={{
              fontSize: 11,
              fontWeight: "600",
              color: paletteToken("fg_muted"),
            }}
          >
            {agentDisplayName(state)}
          </Text>
        </View>
      </View>

      <View
        testID="acct-add-close"
        role="button"
        accessibilityLabel={t("common.close")}
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          width: 28,
          height: 28,
          borderRadius: 999,
          cursor: "pointer",
        }}
        hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
        onPress={() => state.closeAdd()}
      >
        <Icon name="x" size={14} color={paletteToken("fg_muted")} />
      </View>
    </View>
  );
}

function AddMethodTabs(props: { state: AccountsState }): JSX.Element {
  const tabs: { id: AddStep; testID: string; icon: IconName; label: () => string }[] = [
    { id: "oauth", testID: "acct-tab-oauth", icon: "globe", label: () => t("accountsPage.tabOauth") },
    { id: "token", testID: "acct-add-token", icon: "fileText", label: () => t("accountsPage.tabToken") },
    { id: "local", testID: "acct-tab-local", icon: "laptop", label: () => t("accountsPage.tabLocal") },
  ];

  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 4,
        padding: 3,
        borderRadius: 10,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("well"),
      }}
    >
      {tabs.map((tab) => (
        <View
          testID={tab.testID}
          role="button"
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            justifyContent: "center",
            gap: 6,
            flex: 1,
            height: 30,
            paddingHorizontal: 8,
            borderRadius: 8,
            cursor: "pointer",
            backgroundColor:
              props.state.add.step() === tab.id ? paletteToken("card") : "transparent",
            borderWidth: props.state.add.step() === tab.id ? 1 : 0,
            borderColor:
              props.state.add.step() === tab.id ? paletteToken("border") : "transparent",
          }}
          hoverStyle={{
            backgroundColor:
              props.state.add.step() === tab.id
                ? paletteToken("card")
                : paletteToken("panel_hover"),
          }}
          onPress={() => props.state.selectAddTab(tab.id)}
        >
          <Icon
            name={tab.icon}
            size={13}
            color={
              props.state.add.step() === tab.id
                ? paletteToken("accent")
                : paletteToken("fg_muted")
            }
          />
          <Text
            style={{
              numberOfLines: 1,
              textOverflow: "ellipsis",
              fontSize: 12,
              fontWeight: props.state.add.step() === tab.id ? "600" : "500",
              color:
                props.state.add.step() === tab.id
                  ? paletteToken("fg")
                  : paletteToken("fg_muted"),
            }}
          >
            {tab.label()}
          </Text>
        </View>
      ))}
    </View>
  );
}

function DialogError(props: { text: string }): JSX.Element {
  return (
    <View
      testID="acct-add-error"
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 8,
        paddingVertical: 8,
        paddingHorizontal: 10,
        borderRadius: 10,
        borderWidth: 1,
        borderColor: paletteToken("danger_border"),
        backgroundColor: paletteToken("danger_bg"),
      }}
    >
      <Icon name="alertTriangle" size={14} color={paletteToken("danger")} />
      <Text
        style={{
          flex: 1,
          minWidth: 0,
          fontSize: 12,
          color: paletteToken("danger_fg"),
        }}
      >
        {isFormError(props.text) ? t(props.text) : props.text}
      </Text>
    </View>
  );
}

function OAuthTabBody(props: { state: AccountsState }): JSX.Element {
  const state = props.state;
  return (
    <View style={{ display: "flex", flexDirection: "column" }}>
      {state.add.login() ? (
        <OAuthWaitingPanel state={state} login={state.add.login()!} />
      ) : (
        <View style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          <View
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 6,
              padding: 12,
              borderRadius: 12,
              borderWidth: 1,
              borderColor: paletteToken("border_soft"),
              backgroundColor: paletteToken("well"),
            }}
          >
            <Text style={{ fontSize: 13, fontWeight: "600", color: paletteToken("fg") }}>
              {t("accountsPage.oauthEntry")}
            </Text>
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), lineHeight: 18 }}>
              {t("accountsPage.oauthEntryHint")}
            </Text>
          </View>

          {!supportsOauth(state) ? (
            <View
              style={{
                display: "flex",
                flexDirection: "column",
                gap: 8,
                padding: 10,
                borderRadius: 10,
                borderWidth: 1,
                borderColor: paletteToken("warn_border"),
                backgroundColor: paletteToken("warn_bg"),
              }}
            >
              <Text style={{ fontSize: 12, color: paletteToken("fg"), lineHeight: 17 }}>
                {tf("accountsPage.noOauthHint", { agent: agentDisplayName(state) })}
              </Text>
              <View style={{ display: "flex", flexDirection: "row", gap: 8 }}>
                <SecondaryButton
                  label={t("accountsPage.tabLocal")}
                  onPress={() => state.selectAddTab("local")}
                />
                <SecondaryButton
                  label={t("accountsPage.tabToken")}
                  onPress={() => state.selectAddTab("token")}
                />
              </View>
            </View>
          ) : null}

          <PrimaryActionButton
            testID="acct-add-oauth"
            icon="globe"
            label={t("accountsPage.startOauthBtn")}
            loading={state.add.busy() === "start"}
            disabled={state.add.busy() !== null}
            onPress={() => void state.startLogin()}
          />
        </View>
      )}
    </View>
  );
}

function OAuthWaitingPanel(props: { state: AccountsState; login: LoginStart }): JSX.Element {
  const state = props.state;
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      <View
        testID="acct-add-waiting"
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 10,
          paddingVertical: 10,
          paddingHorizontal: 12,
          borderRadius: 12,
          borderWidth: 1,
          borderColor: paletteToken("info_border"),
          backgroundColor: paletteToken("info_bg"),
        }}
      >
        <IconSpin name="loader" size={16} color={paletteToken("info")} spin />
        <View style={{ display: "flex", flexDirection: "column", gap: 2, flex: 1, minWidth: 0 }}>
          <Text style={{ fontSize: 13, fontWeight: "600", color: paletteToken("fg") }}>
            {t("accountsPage.waiting")}
          </Text>
          <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
            {t("accountsPage.waitingHint")}
          </Text>
        </View>
      </View>

      {props.login.userCode ? (
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 10,
            padding: 12,
            borderRadius: 12,
            borderWidth: 1,
            borderColor: paletteToken("border"),
            backgroundColor: paletteToken("well"),
          }}
        >
          <View style={{ display: "flex", flexDirection: "column", gap: 2 }}>
            <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
              {t("accountsPage.userCode")}
            </Text>
            <Text
              testID="acct-add-user-code"
              style={{
                fontSize: 18,
                fontWeight: "700",
                color: paletteToken("fg"),
                fontVariant: "tabular-nums",
              }}
            >
              {props.login.userCode}
            </Text>
          </View>
          <SecondaryButton
            icon="copy"
            label={state.add.copied() ? t("accountsPage.copied") : t("common.copy")}
            onPress={() => void state.copyAuthText(props.login.userCode ?? "")}
          />
        </View>
      ) : null}

      <View
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 8,
          padding: 10,
          borderRadius: 10,
          borderWidth: 1,
          borderColor: paletteToken("border_soft"),
          backgroundColor: paletteToken("well"),
        }}
      >
        <Text style={{ fontSize: 11, fontWeight: "600", color: paletteToken("fg_muted") }}>
          {t("accountsPage.authUrlLabel")}
        </Text>
        <Text
          style={{
            numberOfLines: 1,
            textOverflow: "ellipsis",
            fontSize: 11,
            color: paletteToken("fg_faint"),
          }}
        >
          {props.login.authUrl}
        </Text>
        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
          <SecondaryButton
            testID="acct-add-open"
            icon="externalLink"
            label={t("accountsPage.openBrowser")}
            onPress={() => void openUrl(props.login.verificationUri || props.login.authUrl)}
          />
          <SecondaryButton
            testID="acct-add-copy-url"
            icon={state.add.copied() ? "check" : "copy"}
            label={state.add.copied() ? t("accountsPage.copied") : t("accountsPage.copyUrl")}
            onPress={() => void state.copyAuthText(props.login.authUrl)}
          />
        </View>
      </View>

      {flowTakesPaste(props.login.flow) ? (
        <View style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg_muted") }}>
            {t("accountsPage.callbackLabel")}
          </Text>
          <View
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              gap: 8,
            }}
          >
            <View
              style={{
                flex: 1,
                minWidth: 0,
                height: 34,
                paddingHorizontal: 10,
                borderRadius: 10,
                borderWidth: 1,
                borderColor: paletteToken("border"),
                backgroundColor: paletteToken("input"),
                display: "flex",
                justifyContent: "center",
              }}
            >
              <Input
                testID="acct-add-callback"
                value={state.add.callback()}
                placeholder={t("accountsPage.callbackPlaceholder")}
                onChange={(e: { value: string }) => state.add.callback.set(e.value)}
                style={{
                  fontSize: 12,
                  color: paletteToken("fg"),
                  padding: 0,
                  backgroundColor: "transparent",
                  borderWidth: 0,
                }}
              />
            </View>

            <View
              testID="acct-add-callback-submit"
              role="button"
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                height: 34,
                paddingHorizontal: 12,
                borderRadius: 10,
                backgroundColor: paletteToken("accent"),
                opacity: state.add.busy() !== "finish" ? 0.55 : 1,
                cursor: state.add.busy() !== "finish" ? "default" : "pointer",
                flexShrink: 0,
              }}
              hoverStyle={
                state.add.busy() !== "finish"
                  ? undefined
                  : { backgroundColor: paletteToken("accent_hover") }
              }
              onPress={
                state.add.busy() !== "finish" ? undefined : () => void state.submitCallback()
              }
            >
              <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("on_accent") }}>
                {t("accountsPage.submitCallback")}
              </Text>
            </View>
          </View>
        </View>
      ) : null}

      <View style={{ display: "flex", flexDirection: "row", justifyContent: "flex-end" }}>
        <SecondaryButton
          testID="acct-add-cancel"
          label={t("common.cancel")}
          onPress={() => state.closeAdd()}
        />
      </View>
    </View>
  );
}

function TokenTabBody(props: { state: AccountsState }): JSX.Element {
  const state = props.state;
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      <View
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 4,
          padding: 12,
          borderRadius: 12,
          borderWidth: 1,
          borderColor: paletteToken("border_soft"),
          backgroundColor: paletteToken("well"),
        }}
      >
        <Text style={{ fontSize: 13, fontWeight: "600", color: paletteToken("fg") }}>
          {t("accountsPage.tokenEntry")}
        </Text>
        <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), lineHeight: 18 }}>
          {t("accountsPage.tokenEntryHint")}
        </Text>
      </View>

      <View style={{ display: "flex", flexDirection: "column", gap: 6 }}>
        <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg_muted") }}>
          {t("accountsPage.tokenLabel")}
        </Text>
        <View
          style={{
            height: 36,
            paddingHorizontal: 10,
            borderRadius: 10,
            borderWidth: 1,
            borderColor: paletteToken("border"),
            backgroundColor: paletteToken("input"),
            display: "flex",
            justifyContent: "center",
          }}
        >
          <Input
            testID="acct-add-token-input"
            value={state.add.token()}
            placeholder={t("accountsPage.tokenPlaceholder")}
            onChange={(e: { value: string }) => state.add.token.set(e.value)}
            style={{
              fontSize: 12,
              color: paletteToken("fg"),
              padding: 0,
              backgroundColor: "transparent",
              borderWidth: 0,
            }}
          />
        </View>
        <Text style={{ fontSize: 11, color: paletteToken("fg_faint") }}>
          {t("accountsPage.tokenHint")}
        </Text>
      </View>

      <PrimaryActionButton
        testID="acct-add-token-submit"
        icon="fileText"
        label={t("common.import")}
        loading={state.add.busy() === "token"}
        disabled={state.add.busy() !== null}
        onPress={() => void state.submitToken()}
      />
    </View>
  );
}

function LocalTabBody(props: { state: AccountsState }): JSX.Element {
  const state = props.state;
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      <View
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 6,
          padding: 12,
          borderRadius: 12,
          borderWidth: 1,
          borderColor: paletteToken("border_soft"),
          backgroundColor: paletteToken("well"),
        }}
      >
        <Text style={{ fontSize: 13, fontWeight: "600", color: paletteToken("fg") }}>
          {t("accountsPage.localEntry")}
        </Text>
        <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), lineHeight: 18 }}>
          {t("accountsPage.localEntryHint")}
        </Text>
        {localSourcePath(state) !== "" ? (
          <Text style={{ fontSize: 11, color: paletteToken("fg_faint"), lineHeight: 16 }}>
            {tf("accountsPage.localSourcePath", { source: localSourcePath(state) })}
          </Text>
        ) : null}
      </View>

      <PrimaryActionButton
        testID="acct-add-local"
        icon="laptop"
        label={t("accountsPage.localEntry")}
        loading={state.add.busy() === "import"}
        disabled={state.add.busy() !== null}
        onPress={() => void state.importLocal()}
      />
    </View>
  );
}

function PrimaryActionButton(props: {
  testID: string;
  icon: IconName;
  label: string;
  loading: boolean;
  disabled: boolean;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      role="button"
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "center",
        gap: 8,
        height: 36,
        paddingHorizontal: 14,
        borderRadius: 10,
        backgroundColor: paletteToken("accent"),
        opacity: props.disabled ? 0.55 : 1,
        cursor: props.disabled ? "default" : "pointer",
      }}
      hoverStyle={props.disabled ? undefined : { backgroundColor: paletteToken("accent_hover") }}
      onPress={props.disabled ? undefined : props.onPress}
    >
      {props.loading ? (
        <IconSpin name="loader" size={14} color={paletteToken("on_accent")} spin />
      ) : (
        <Icon name={props.icon} size={14} color={paletteToken("on_accent")} />
      )}
      <Text style={{ fontSize: 13, fontWeight: "600", color: paletteToken("on_accent") }}>
        {props.label}
      </Text>
    </View>
  );
}

function SecondaryButton(props: {
  testID?: string;
  icon?: IconName;
  label: string;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      role="button"
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "center",
        gap: 6,
        height: 30,
        paddingHorizontal: 10,
        borderRadius: 8,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("panel"),
        cursor: "pointer",
      }}
      hoverStyle={{
        backgroundColor: paletteToken("panel_hover"),
        borderColor: paletteToken("edge"),
      }}
      onPress={props.onPress}
    >
      {props.icon ? <Icon name={props.icon} size={13} color={paletteToken("fg_muted")} /> : null}
      <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg") }}>
        {props.label}
      </Text>
    </View>
  );
}
