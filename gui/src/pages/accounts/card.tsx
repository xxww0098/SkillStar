// 单账号卡片与共享视觉原语（复选框、套餐徽章、当前状态标签、底部快捷操作）。
// 与 cockpit-tools 的「顶部选择+账号+当前+套餐 / 副标题或重授权 / well 配额双栏 / 底部时间+操作」对齐；
// 颜色全走 paletteToken()，组件体内不预读 props/signal 以保持 nsh getter 响应式。

import { Text, View, type JSX } from "nsh";
import { IconSpin } from "../../chrome/feedback";
import { Icon, type IconName } from "../../chrome/icons";
import { Tip } from "../../chrome/tooltip";
import { SKILL_CARD_W } from "../../layout";
import { paletteToken } from "../../services/themeMode";
import { t } from "../../services/translation";
import {
  lastUsedStamp,
  loginLine,
  maskAccountText,
  planKind,
  planOf,
  withAlpha,
  type PlanKind,
} from "./logic";
import { QuotaArea } from "./meters";
import type { AccountView } from "./types";

/** 套餐徽章配色，全部来自语义 token 以兼容暗/亮主题对比度。 */
export function planColors(kind: PlanKind): { bg: string; fg: string; border?: string } {
  switch (kind) {
    case "ultra":
      return {
        bg: paletteToken("amber_bg"),
        fg: paletteToken("amber_fg"),
        border: paletteToken("amber_border"),
      };
    case "pro":
      return {
        bg: paletteToken("accent"),
        fg: paletteToken("on_accent"),
      };
    case "plus":
      return {
        bg: paletteToken("ok_bg"),
        fg: paletteToken("ok"),
        border: paletteToken("ok_border"),
      };
    case "team":
      return {
        bg: withAlpha(paletteToken("violet"), 0.14),
        fg: paletteToken("violet_fg"),
        border: withAlpha(paletteToken("violet"), 0.28),
      };
    case "enterprise":
      return {
        bg: paletteToken("warn_bg"),
        fg: paletteToken("warn"),
        border: paletteToken("warn_border"),
      };
    case "free":
    case "none":
      return {
        bg: paletteToken("well"),
        fg: paletteToken("fg_muted"),
        border: paletteToken("border"),
      };
  }
}

/** 网格视图单账号卡片。 */
export function AccountCard(props: {
  account: AccountView;
  serving: boolean;
  busy: boolean;
  refreshing: boolean;
  selected: boolean;
  masked: boolean;
  onToggleSelect: () => void;
  onRefresh: () => void;
  onActivate: () => void;
  onDelete: () => void;
  onReauth: () => void;
}): JSX.Element {
  return (
    <View
      testID={`acct-card-${props.account.id}`}
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 10,
        width: SKILL_CARD_W,
        minHeight: 228,
        paddingVertical: 12,
        paddingHorizontal: 14,
        borderRadius: 16,
        borderWidth: 1,
        backgroundColor: props.selected ? paletteToken("card_active") : paletteToken("card"),
        borderColor:
          props.selected || props.serving ? paletteToken("accent") : paletteToken("border"),
      }}
      hoverStyle={{
        backgroundColor: props.selected
          ? paletteToken("card_active_hover")
          : paletteToken("card_hover"),
        borderColor:
          props.selected || props.serving ? paletteToken("accent") : paletteToken("edge"),
      }}
    >
      <CardTop
        account={props.account}
        serving={props.serving}
        selected={props.selected}
        masked={props.masked}
        onToggleSelect={props.onToggleSelect}
      />

      {props.account.requiresReauth ? (
        <ReauthRow id={props.account.id} busy={props.busy} onReauth={props.onReauth} />
      ) : (
        <LoginSubline account={props.account} masked={props.masked} />
      )}

      <QuotaArea usage={props.account.usage} />

      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 8,
          marginTop: "auto",
          paddingTop: 10,
          borderTopWidth: 1,
          borderTopColor: paletteToken("border_soft"),
        }}
      >
        <LastUsedMeta account={props.account} />
        <AccountActionButtons
          account={props.account}
          serving={props.serving}
          busy={props.busy}
          refreshing={props.refreshing}
          onRefresh={props.onRefresh}
          onActivate={props.onActivate}
          onDelete={props.onDelete}
        />
      </View>
    </View>
  );
}

function CardTop(props: {
  account: AccountView;
  serving: boolean;
  selected: boolean;
  masked: boolean;
  onToggleSelect: () => void;
}): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 6,
      }}
    >
      <SelectCheckbox
        testID={`acct-select-${props.account.id}`}
        checked={props.selected}
        label={props.account.displayName}
        onPress={props.onToggleSelect}
      />

      <Text
        testID={`acct-name-${props.account.id}`}
        style={{
          flexShrink: 1,
          minWidth: 0,
          numberOfLines: 1,
          textOverflow: "ellipsis",
          fontSize: 14,
          fontWeight: "600",
          color: paletteToken("fg"),
        }}
      >
        {maskAccountText(props.account.displayName, props.masked)}
      </Text>

      {props.serving ? <ServingBadge id={props.account.id} /> : null}

      {props.account.usage?.error ? (
        <QuotaErrorBadge id={props.account.id} error={props.account.usage.error} />
      ) : null}

      <View style={{ marginLeft: "auto", flexShrink: 0 }}>
        <PlanBadge id={props.account.id} account={props.account} />
      </View>
    </View>
  );
}

/** 单行复选按钮，支持卡片 / 列表 / 紧凑视图共用。 */
export function SelectCheckbox(props: {
  testID: string;
  checked: boolean;
  label: string;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      role="button"
      accessibilityLabel={props.label}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        width: 22,
        height: 22,
        borderRadius: 6,
        flexShrink: 0,
        cursor: "pointer",
      }}
      hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
      onPress={props.onPress}
    >
      <Icon
        name={props.checked ? "squareCheck" : "square"}
        size={15}
        color={props.checked ? paletteToken("accent") : paletteToken("fg_faint")}
      />
    </View>
  );
}

/** 绿色「当前」生效标签。 */
export function ServingBadge(props: { id: string; compact?: boolean }): JSX.Element {
  return (
    <View
      testID={`acct-current-${props.id}`}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        paddingHorizontal: props.compact ? 6 : 7,
        height: props.compact ? 18 : 20,
        borderRadius: 999,
        borderWidth: 1,
        borderColor: paletteToken("ok_border"),
        backgroundColor: paletteToken("ok_bg"),
        flexShrink: 0,
      }}
    >
      <Text
        style={{
          fontSize: 11,
          fontWeight: "600",
          color: paletteToken("ok"),
        }}
      >
        {t("accountsPage.current")}
      </Text>
    </View>
  );
}

/** 套餐徽章（ULTRA / PRO / PLUS / TEAM / ENTERPRISE / FREE）。 */
export function PlanBadge(props: {
  id: string;
  account: AccountView;
  compact?: boolean;
}): JSX.Element {
  return (
    <View style={{ display: planOf(props.account) ? "flex" : "none", flexShrink: 0 }}>
      {planOf(props.account) ? (
        <View
          testID={`acct-plan-${props.id}`}
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            paddingHorizontal: props.compact ? 6 : 7,
            height: props.compact ? 18 : 20,
            borderRadius: 999,
            borderWidth: planColors(planKind(planOf(props.account))).border ? 1 : 0,
            borderColor: planColors(planKind(planOf(props.account))).border,
            backgroundColor: planColors(planKind(planOf(props.account))).bg,
            flexShrink: 0,
          }}
        >
          <Text
            style={{
              fontSize: 11,
              fontWeight: "600",
              textTransform: "uppercase",
              color: planColors(planKind(planOf(props.account))).fg,
            }}
          >
            {planOf(props.account) ?? ""}
          </Text>
        </View>
      ) : null}
    </View>
  );
}

/** 额度读取失败提示小角标，完整错误收进 tooltip。 */
export function QuotaErrorBadge(props: { id: string; error: string }): JSX.Element {
  return (
    <View tooltip={<Tip text={`${t("accountsPage.quotaFailed")}: ${props.error}`} />}>
      <View
        testID={`acct-quota-error-${props.id}`}
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          width: 20,
          height: 20,
          borderRadius: 999,
          borderWidth: 1,
          borderColor: paletteToken("danger_border"),
          backgroundColor: paletteToken("danger_bg"),
          flexShrink: 0,
        }}
      >
        <Icon name="alertTriangle" size={11} color={paletteToken("danger")} />
      </View>
    </View>
  );
}

function LoginSubline(props: { account: AccountView; masked: boolean }): JSX.Element {
  return (
    <Text
      style={{
        numberOfLines: 1,
        textOverflow: "ellipsis",
        fontSize: 12,
        fontWeight: "400",
        color: paletteToken("fg_muted"),
      }}
    >
      {maskAccountText(loginLine(props.account), props.masked)}
    </Text>
  );
}

/** 登录凭据失效提示与重新授权按钮。 */
export function ReauthRow(props: {
  id: string;
  busy: boolean;
  onReauth: () => void;
}): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 8,
        paddingVertical: 5,
        paddingHorizontal: 8,
        borderRadius: 8,
        borderWidth: 1,
        borderColor: paletteToken("danger_border"),
        backgroundColor: paletteToken("danger_bg"),
      }}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 5,
          flex: 1,
          minWidth: 0,
        }}
      >
        <Icon name="alertTriangle" size={12} color={paletteToken("danger")} />
        <Text
          style={{
            numberOfLines: 1,
            textOverflow: "ellipsis",
            fontSize: 11,
            fontWeight: "600",
            color: paletteToken("danger_fg"),
          }}
        >
          {t("accountsPage.reauthNeeded")}
        </Text>
      </View>

      <View
        testID={`acct-reauth-${props.id}`}
        role="button"
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          height: 22,
          paddingHorizontal: 8,
          borderRadius: 6,
          backgroundColor: paletteToken("danger"),
          opacity: props.busy ? 0.5 : 1,
          cursor: props.busy ? "default" : "pointer",
          flexShrink: 0,
        }}
        hoverStyle={props.busy ? undefined : { backgroundColor: paletteToken("danger_hover") }}
        onPress={props.busy ? undefined : props.onReauth}
      >
        <Text
          style={{
            fontSize: 11,
            fontWeight: "600",
            color: paletteToken("on_accent"),
          }}
        >
          {t("accountsPage.reauthorize")}
        </Text>
      </View>
    </View>
  );
}

/** 底部「最近使用」时间戳。 */
export function LastUsedMeta(props: { account: AccountView }): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 4,
        minWidth: 0,
      }}
    >
      <Icon name="clock" size={12} color={paletteToken("fg_faint")} />
      <Text
        style={{
          numberOfLines: 1,
          textOverflow: "ellipsis",
          fontSize: 11,
          fontWeight: "400",
          color: paletteToken("fg_faint"),
          fontVariant: "tabular-nums",
        }}
      >
        {lastUsedStamp(props.account) ?? t("accountsPage.neverUsed")}
      </Text>
    </View>
  );
}

/** 账号快捷操作按钮组：切换（播放）/ 刷新 / 删除。 */
export function AccountActionButtons(props: {
  account: AccountView;
  serving: boolean;
  busy: boolean;
  refreshing: boolean;
  onRefresh: () => void;
  onActivate: () => void;
  onDelete: () => void;
}): JSX.Element {
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
      <ActionIconButton
        testID={`acct-switch-${props.account.id}`}
        icon={props.serving ? "checkCircle" : "play"}
        tip={props.serving ? t("accountsPage.current") : t("accountsPage.switch")}
        color={props.serving ? paletteToken("ok") : paletteToken("ok")}
        disabled={props.serving || props.busy}
        activeHighlight={!props.serving && !props.busy}
        onPress={props.onActivate}
      />

      <ActionIconButton
        testID={`acct-refresh-${props.account.id}`}
        icon="refresh"
        tip={t("common.refresh")}
        color={props.refreshing ? paletteToken("accent") : paletteToken("fg_muted")}
        disabled={props.busy || props.refreshing}
        spin={props.refreshing}
        onPress={props.onRefresh}
      />

      <ActionIconButton
        testID={`acct-delete-${props.account.id}`}
        icon="trash"
        tip={t("common.delete")}
        color={paletteToken("fg_muted")}
        dangerHover
        disabled={props.busy}
        onPress={props.onDelete}
      />
    </View>
  );
}

function ActionIconButton(props: {
  testID: string;
  icon: IconName;
  tip: string;
  color: string;
  disabled: boolean;
  spin?: boolean;
  activeHighlight?: boolean;
  dangerHover?: boolean;
  onPress: () => void;
}): JSX.Element {
  return (
    <View tooltip={<Tip text={props.tip} />}>
      <View
        testID={props.testID}
        role="button"
        accessibilityLabel={props.tip}
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          width: 28,
          height: 28,
          borderRadius: 8,
          borderWidth: 1,
          borderColor: props.activeHighlight
            ? paletteToken("ok_border")
            : paletteToken("border_soft"),
          backgroundColor: props.activeHighlight ? paletteToken("ok_bg") : paletteToken("bg"),
          opacity: props.disabled && !props.spin ? 0.48 : 1,
          cursor: props.disabled ? "default" : "pointer",
        }}
        hoverStyle={
          props.disabled
            ? undefined
            : props.dangerHover
              ? {
                  backgroundColor: paletteToken("danger_bg"),
                  borderColor: paletteToken("danger_border"),
                }
              : props.activeHighlight
                ? {
                    backgroundColor: paletteToken("ok_bg"),
                    borderColor: paletteToken("ok"),
                  }
                : {
                    backgroundColor: paletteToken("panel_hover"),
                    borderColor: paletteToken("edge"),
                  }
        }
        onPress={props.disabled ? undefined : props.onPress}
      >
        {props.spin !== undefined ? (
          <IconSpin name={props.icon} size={13} color={props.color} spin={props.spin} />
        ) : (
          <Icon name={props.icon} size={13} color={props.color} />
        )}
      </View>
    </View>
  );
}
