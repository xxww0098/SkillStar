// 全局账号额度下拉窗子部件：手风琴展开区的备选账号单行与紧凑配额胶囊。

import { Text, View, computed, type JSX } from "nsh";
import { IconSpin } from "../../chrome/feedback";
import { Tip } from "../../chrome/tooltip";
import { paletteToken } from "../../services/themeMode";
import { t } from "../../services/translation";
import { openAccountContextMenu } from "./accountContextMenu";
import { PlanBadge } from "./card";
import { clampPercent, formatCreditValue, percentText, remainingTone } from "./logic";
import { toneColor } from "./meters";
import { compactIdentity } from "./quotaPopoverLogic";
import type { AccountView, Credits } from "./types";

export function AlternateAccountRow(props: {
  account: AccountView;
  masked: boolean;
  switching: boolean;
  refreshing: boolean;
  disabled: boolean;
  onActivate: () => void;
  onRefresh: () => void;
  onReauth: () => void;
  onOpenAgent: () => void;
}): JSX.Element {
  return (
    <View
      testID={`quota-alt-${props.account.id}`}
      onContextMenu={(e) =>
        openAccountContextMenu(e, {
          account: props.account,
          serving: false,
          busy: props.disabled,
          refreshing: props.refreshing,
          onActivate: props.onActivate,
          onRefresh: props.onRefresh,
          onReauth: props.onReauth,
          onOpenAgent: props.onOpenAgent,
        })
      }
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 6,
        minHeight: 32,
        paddingHorizontal: 7,
        paddingVertical: 4,
        borderRadius: 7,
        backgroundColor: paletteToken("well"),
        overflow: "hidden",
      }}
      hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 5,
          flex: 1,
          minWidth: 0,
          overflow: "hidden",
        }}
      >
        <Text
          style={{
            flexShrink: 1,
            minWidth: 0,
            numberOfLines: 1,
            textOverflow: "ellipsis",
            fontSize: 11,
            fontWeight: "500",
            color: paletteToken("fg"),
          }}
        >
          {compactIdentity(props.account, props.masked)}
        </Text>
        <PlanBadge id={`pop-alt-${props.account.id}`} account={props.account} compact />
      </View>

      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 5,
          flexShrink: 0,
        }}
      >
        <If
          when={props.account.requiresReauth}
          else={<AlternateQuotaPills account={props.account} />}
        >
          <View
            testID={`quota-alt-reauth-${props.account.id}`}
            role="button"
            style={{
              paddingHorizontal: 6,
              height: 20,
              borderRadius: 5,
              backgroundColor: paletteToken("danger_bg"),
              borderWidth: 1,
              borderColor: paletteToken("danger_border"),
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              cursor: "pointer",
            }}
            onPress={props.onReauth}
          >
            <Text style={{ fontSize: 10, fontWeight: "600", color: paletteToken("danger_fg") }}>
              {t("accountsPage.reauthorize")}
            </Text>
          </View>
        </If>

        <View
          testID={`quota-switch-${props.account.id}`}
          role="button"
          tooltip={<Tip text={t("accountsPage.switch")} />}
          accessibilityLabel={t("accountsPage.switch")}
          accessibilityState={{ disabled: props.disabled, busy: props.switching }}
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            width: 24,
            height: 24,
            borderRadius: 6,
            borderWidth: 1,
            borderColor: paletteToken("ok_border"),
            backgroundColor: paletteToken("ok_bg"),
            opacity: props.disabled && !props.switching ? 0.48 : 1,
            cursor: props.disabled ? "default" : "pointer",
            flexShrink: 0,
          }}
          hoverStyle={
            props.disabled
              ? undefined
              : {
                  backgroundColor: paletteToken("ok_hover"),
                  borderColor: paletteToken("ok"),
                }
          }
          onPress={props.disabled ? undefined : props.onActivate}
        >
          <IconSpin name="play" size={11} color={paletteToken("ok")} spin={props.switching} />
        </View>
      </View>
    </View>
  );
}

function AlternateQuotaPills(props: { account: AccountView }): JSX.Element {
  const topWindows = computed(() => (props.account.usage?.windows ?? []).slice(0, 2));
  let lastCredits: Credits | null = null;
  const credits = computed(() => {
    if (topWindows().length > 0) return null;
    const next = props.account.usage?.credits ?? null;
    if (next !== null) lastCredits = next;
    return next;
  });
  const creditsOrLast = computed(() => credits() ?? lastCredits);

  return (
    <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 4 }}>
      <If
        when={topWindows().length > 0}
        else={
          <If when={credits() !== null}>
            <Text
              style={{
                fontSize: 10,
                fontWeight: "600",
                fontVariant: "tabular-nums",
                color: paletteToken("fg_muted"),
              }}
            >
              {formatCreditValue(creditsOrLast()!, t("accountsPage.unlimited"))}
            </Text>
          </If>
        }
      >
        <Each of={topWindows()} key={(w) => w.key}>
          {(w) => {
            const pct = computed(() => clampPercent(w().remainingPercent));
            const color = computed(() => toneColor(remainingTone(pct())));
            return (
              <View
                style={{
                  display: "flex",
                  flexDirection: "row",
                  alignItems: "center",
                  gap: 3,
                  paddingHorizontal: 5,
                  height: 18,
                  borderRadius: 5,
                  backgroundColor: paletteToken("card"),
                  borderWidth: 1,
                  borderColor: paletteToken("border_soft"),
                }}
              >
                <View
                  style={{
                    width: 5,
                    height: 5,
                    borderRadius: 999,
                    backgroundColor: color(),
                  }}
                />
                <Text
                  style={{
                    fontSize: 10,
                    fontWeight: "700",
                    fontVariant: "tabular-nums",
                    color: color(),
                  }}
                >
                  {percentText(pct())}
                </Text>
              </View>
            );
          }}
        </Each>
      </If>
    </View>
  );
}
