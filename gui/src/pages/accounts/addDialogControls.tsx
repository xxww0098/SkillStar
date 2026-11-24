// Controls shared by the account import dialog.
import { Text, View, type JSX } from "nsh";
import { Input } from "nsh/base";
import { IconSpin } from "../../chrome/feedback";
import { Icon, type IconName } from "../../chrome/icons";
import { paletteToken } from "../../services/themeMode";
import { dt } from "./addDialogStrings";
import type { AccountsState } from "./state";
import type { OAuthModeOption } from "./types";

export function PrimaryActionButton(props: {
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
      accessibilityState={{ disabled: props.disabled, busy: props.loading }}
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
      onPress={() => {
        if (!props.disabled) props.onPress();
      }}
    >
      <If
        when={props.loading}
        else={<Icon name={props.icon} size={14} color={paletteToken("on_accent")} />}
      >
        <IconSpin name="loader" size={14} color={paletteToken("on_accent")} spin />
      </If>
      <Text style={{ fontSize: 13, fontWeight: "600", color: paletteToken("on_accent") }}>
        {props.label}
      </Text>
    </View>
  );
}

export function SecondaryButton(props: {
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

/** 依据 AgentRow.oauthModes 能力元数据显示多路登录腿与可选参数（如 IdC region/startUrl）。 */
export function OAuthModeConfigPanel(props: { state: AccountsState }): JSX.Element {
  const state = props.state;
  const modes = (): OAuthModeOption[] => {
    const id = state.add.agent();
    if (!id) return [];
    const row = state.agents().find((a) => a.id === id);
    return row?.supportsOauth === false ? [] : (row?.oauthModes ?? []);
  };
  const activeModeId = (): string => {
    const list = modes();
    const cur = state.add.oauthMode();
    return list.some((m) => m.id === cur) ? cur : (list[0]?.id ?? "");
  };
  const hasField = (field: string): boolean => {
    const id = activeModeId();
    const mode = modes().find((m) => m.id === id);
    return mode?.fields?.includes(field) === true;
  };
  const editable = (): boolean => state.add.busy() === null;

  return (
    <If when={modes().length > 0}>
      <View style={{ display: "flex", flexDirection: "column", gap: 10 }}>
        <View style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg_muted") }}>
            {dt("addDialog.oauthModeLabel")}
          </Text>
          <View
            testID="acct-oauth-modes"
            style={{
              display: "flex",
              flexDirection: "row",
              gap: 4,
              padding: 3,
              borderRadius: 10,
              borderWidth: 1,
              borderColor: paletteToken("border"),
              backgroundColor: paletteToken("well"),
            }}
          >
            <Each of={modes()} key={(m) => m.id}>
              {(mode) => (
                <View
                  testID={`acct-oauth-mode-${mode().id}`}
                  role="button"
                  accessibilityState={{
                    selected: activeModeId() === mode().id,
                    disabled: !editable(),
                  }}
                  style={{
                    flex: 1,
                    height: 28,
                    borderRadius: 7,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    backgroundColor:
                      activeModeId() === mode().id ? paletteToken("panel") : "transparent",
                    borderWidth: activeModeId() === mode().id ? 1 : 0,
                    borderColor: paletteToken("edge"),
                    cursor: editable() ? "pointer" : "default",
                    opacity: editable() ? 1 : 0.65,
                  }}
                  hoverStyle={
                    activeModeId() === mode().id || !editable()
                      ? undefined
                      : { backgroundColor: paletteToken("panel_hover") }
                  }
                  onPress={() => {
                    if (editable()) state.selectOauthMode(mode().id);
                  }}
                >
                  <Text
                    style={{
                      fontSize: 12,
                      fontWeight: activeModeId() === mode().id ? "600" : "500",
                      color:
                        activeModeId() === mode().id
                          ? paletteToken("fg")
                          : paletteToken("fg_muted"),
                    }}
                  >
                    {mode().label}
                  </Text>
                </View>
              )}
            </Each>
          </View>
        </View>

        <If when={hasField("idcRegion")}>
          <ModeTextField
            testID="acct-idc-region"
            label={dt("addDialog.idcRegionLabel")}
            placeholder={dt("addDialog.idcRegionPlaceholder")}
            value={() => state.add.idcRegion()}
            onChange={(val) => state.add.idcRegion.set(val)}
          />
        </If>

        <If when={hasField("idcStartUrl")}>
          <ModeTextField
            testID="acct-idc-start-url"
            label={dt("addDialog.idcStartUrlLabel")}
            placeholder={dt("addDialog.idcStartUrlPlaceholder")}
            hint={dt("addDialog.idcStartUrlHint")}
            value={() => state.add.idcStartUrl()}
            onChange={(val) => state.add.idcStartUrl.set(val)}
          />
        </If>
      </View>
    </If>
  );
}

function ModeTextField(props: {
  testID: string;
  label: string;
  placeholder: string;
  hint?: string;
  value: () => string;
  onChange: (value: string) => void;
}): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 5 }}>
      <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg_muted") }}>
        {props.label}
      </Text>
      <View
        style={{
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
          testID={props.testID}
          value={props.value()}
          placeholder={props.placeholder}
          onChange={(e: { value: string }) => props.onChange(e.value)}
          style={{
            fontSize: 12,
            color: paletteToken("fg"),
            padding: 0,
            backgroundColor: "transparent",
            borderWidth: 0,
          }}
        />
      </View>
      <If when={Boolean(props.hint)}>
        <Text style={{ fontSize: 11, color: paletteToken("fg_faint") }}>{props.hint ?? ""}</Text>
      </If>
    </View>
  );
}
