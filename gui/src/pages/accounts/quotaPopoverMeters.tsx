// 全局账号额度下拉窗子部件：well 紧凑双列窗口槽、剩余进度条与点数行。

import { Text, View, computed, type JSX } from "nsh";
import { Icon } from "../../chrome/icons";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import {
  clampPercent,
  formatCreditValue,
  formatSpan,
  hasQuota,
  nowSecs,
  percentText,
  remainingTone,
  resetCardCount,
  windowChip,
  windowLabel,
  windowRows,
} from "./logic";
import { toneColor } from "./meters";
import type { AccountView, Credits, UsageWindow } from "./types";

export function CompactQuotaWell(props: { account: AccountView }): JSX.Element {
  const usage = computed(() => props.account.usage);
  const available = computed(() => hasQuota(usage()));
  const noQuotaError = computed(() => (usage()?.error ?? "").trim());
  const rows = computed(() => (available() ? windowRows(usage()?.windows ?? []) : []));
  let lastCredits: Credits | null = null;
  const credits = computed(() => {
    const next = available() ? (usage()?.credits ?? null) : null;
    if (next !== null) lastCredits = next;
    return next;
  });
  const creditsOrLast = computed(() => credits() ?? lastCredits);
  const hasWindows = computed(() => rows().length > 0);
  let lastResetCards: number | null = null;
  const resetCards = computed(() => {
    const next = available() ? resetCardCount(usage()) : null;
    if (next !== null) lastResetCards = next;
    return next;
  });
  const resetCardsOrLast = computed(() => resetCards() ?? lastResetCards);
  const hasAboveResetCards = computed(() => rows().length > 0 || credits() !== null);

  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 6,
        width: "100%",
        paddingVertical: 7,
        paddingHorizontal: 8,
        borderRadius: 8,
        borderWidth: 1,
        borderColor: paletteToken("border_soft"),
        backgroundColor: paletteToken("well"),
        overflow: "hidden",
      }}
    >
      <If
        when={available()}
        else={
          <Text
            style={{
              fontSize: 11,
              color:
                noQuotaError().length > 0 ? paletteToken("danger_fg") : paletteToken("fg_faint"),
              textAlign: "center",
              paddingVertical: 2,
              numberOfLines: 2,
              textOverflow: "ellipsis",
            }}
          >
            {noQuotaError().length > 0 ? noQuotaError() : t("accountsPage.noQuota")}
          </Text>
        }
      >
        <Each of={rows()} key="index">
          {(row) => (
            <View
              style={{
                display: "flex",
                flexDirection: "row",
                alignItems: "flex-start",
                gap: 8,
                width: "100%",
              }}
            >
              <Each of={row()} key={(w) => w.key}>
                {(w) => <CompactWindowSlot window={w()} />}
              </Each>
            </View>
          )}
        </Each>
        <If when={credits() !== null}>
          <CompactCreditsLine credits={creditsOrLast()!} hasWindows={hasWindows()} />
        </If>
        <If when={resetCards() !== null}>
          <CompactResetCardsLine
            count={resetCardsOrLast()!}
            hasAbove={hasAboveResetCards()}
          />
        </If>
      </If>
    </View>
  );
}

function CompactWindowSlot(props: { window: UsageWindow }): JSX.Element {
  const pct = computed(() => clampPercent(props.window.remainingPercent));
  const color = computed(() => toneColor(remainingTone(pct())));
  const metaText = computed(() => {
    const chip = windowChip(props.window);
    if (chip !== null) return chip;
    const resetsAt = props.window.resetsAt;
    if (resetsAt === null || resetsAt === undefined || resetsAt <= 0) return null;
    const remaining = resetsAt - nowSecs();
    return remaining <= 0 ? t("accountsPage.resetDone") : formatSpan(remaining);
  });

  return (
    <View style={{ flex: 1, minWidth: 0 }}>
      <View style={{ display: "flex", flexDirection: "column", gap: 3, width: "100%" }}>
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 4,
            width: "100%",
            minWidth: 0,
          }}
        >
          <View
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              gap: 4,
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
                fontWeight: "600",
                color: paletteToken("fg_muted"),
              }}
            >
              {windowLabel(props.window)}
            </Text>
            <If when={metaText() !== null}>
              <Text
                style={{
                  fontSize: 10,
                  fontVariant: "tabular-nums",
                  color: paletteToken("fg_faint"),
                  flexShrink: 0,
                }}
              >
                {metaText() ?? ""}
              </Text>
            </If>
          </View>
          <Text
            style={{
              flexShrink: 0,
              fontSize: 11,
              fontWeight: "700",
              fontVariant: "tabular-nums",
              color: color(),
            }}
          >
            {percentText(props.window.remainingPercent)}
          </Text>
        </View>
        <View
          style={{
            width: "100%",
            height: 4,
            borderRadius: 999,
            backgroundColor: paletteToken("border"),
            overflow: "hidden",
            display: "flex",
          }}
        >
          <View
            style={{
              height: "100%",
              width: `${pct()}%`,
              borderRadius: 999,
              backgroundColor: color(),
            }}
          />
        </View>
      </View>
    </View>
  );
}

function CompactCreditsLine(props: { credits: Credits; hasWindows: boolean }): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 6,
        width: "100%",
        paddingTop: props.hasWindows ? 5 : 0,
        borderTopWidth: props.hasWindows ? 1 : 0,
        borderTopColor: paletteToken("border_soft"),
      }}
    >
      <Text
        style={{
          flexShrink: 1,
          minWidth: 0,
          numberOfLines: 1,
          textOverflow: "ellipsis",
          fontSize: 11,
          fontWeight: "600",
          color: paletteToken("fg_muted"),
        }}
      >
        {props.credits.label.trim().toLowerCase() === "credits"
          ? t("accountsPage.availableCredits")
          : props.credits.label}
      </Text>
      <Text
        style={{
          flexShrink: 0,
          fontSize: 11,
          fontWeight: "700",
          fontVariant: "tabular-nums",
          color: paletteToken("fg"),
        }}
      >
        {formatCreditValue(props.credits, t("accountsPage.unlimited"))}
      </Text>
    </View>
  );
}

function CompactResetCardsLine(props: { count: number; hasAbove: boolean }): JSX.Element {
  const positive = computed(() => props.count > 0);
  return (
    <View
      testID="quota-reset-cards"
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 6,
        width: "100%",
        paddingTop: props.hasAbove ? 5 : 0,
        borderTopWidth: props.hasAbove ? 1 : 0,
        borderTopColor: paletteToken("border_soft"),
      }}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 4,
          flexShrink: 1,
          minWidth: 0,
        }}
      >
        <Icon name="rotateCcw" size={10} color={paletteToken("fg_muted")} />
        <Text
          style={{
            flexShrink: 1,
            minWidth: 0,
            numberOfLines: 1,
            textOverflow: "ellipsis",
            fontSize: 11,
            fontWeight: "600",
            color: paletteToken("fg_muted"),
          }}
        >
          {t("accountsPage.resetCards")}
        </Text>
      </View>
      <Text
        style={{
          flexShrink: 0,
          fontSize: 11,
          fontWeight: "700",
          fontVariant: "tabular-nums",
          color: positive() ? paletteToken("ok") : paletteToken("fg_muted"),
        }}
      >
        {tf("accountsPage.resetCardsCount", { count: props.count })}
      </Text>
    </View>
  );
}
