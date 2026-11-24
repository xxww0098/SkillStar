// 额度区 —— 卡片中段的 well 软底内框：>=2 个窗口时按双列并排（chunkPairs），
// 每个窗口三层：顶行（标签 + 可选 chip + 右对齐剩余百分比）、中行（4px 进度条）、
// 底行（紧凑重置倒计时）。Credits 置于内框底部。数据行不挂 Tooltip。
//
// 分档：≥80 ok、≥40 warn、≥10 amber、<10 danger；颜色全走 paletteToken()。

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
import type { Credits, UsageSnapshot, UsageWindow } from "./types";

/** 剩余率分档 → 色（填充与百分比同色）。 */
export function toneColor(tone: ReturnType<typeof remainingTone>): string {
  switch (tone) {
    case "ok":
      return paletteToken("ok");
    case "warn":
      return paletteToken("warn");
    case "amber":
      return paletteToken("amber");
    case "danger":
      return paletteToken("danger");
  }
}

function resetCountdownText(resetsAt: number | null | undefined): string | null {
  if (resetsAt === null || resetsAt === undefined || resetsAt <= 0) return null;
  const remaining = resetsAt - nowSecs();
  return remaining <= 0 ? t("accountsPage.resetDone") : formatSpan(remaining);
}

/** 额度区主体（soft well 内框）：双列窗口格子 + credits 行；无数据时居中提示。 */
export function QuotaArea(props: { usage: UsageSnapshot | null | undefined }): JSX.Element {
  const available = computed(() => hasQuota(props.usage));
  const rows = computed(() => (available() ? windowRows(props.usage?.windows ?? []) : []));
  const credits = () => props.usage?.credits ?? null;
  const resetCards = () => resetCardCount(props.usage);
  const errorText = () => (props.usage?.error ?? "").trim();

  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 8,
        width: "100%",
        padding: 10,
        borderRadius: 10,
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
            testID="acct-quota-empty"
            style={{
              fontSize: errorText().length > 0 ? 11 : 12,
              color: errorText().length > 0 ? paletteToken("danger_fg") : paletteToken("fg_muted"),
              textAlign: "center",
              paddingVertical: errorText().length > 0 ? 8 : 14,
              numberOfLines: 2,
              textOverflow: "ellipsis",
            }}
          >
            {errorText().length > 0 ? errorText() : t("accountsPage.noQuota")}
          </Text>
        }
      >
        <>
          <Each of={rows()} key="index">
            {(row) => (
              <View
                style={{
                  display: "flex",
                  flexDirection: "row",
                  alignItems: "flex-start",
                  gap: 10,
                  width: "100%",
                }}
              >
                <Each of={row()} key="index">
                  {(win) => <WindowCell window={win} />}
                </Each>
              </View>
            )}
          </Each>
          <If when={credits() !== null}>
            <CreditsRow
              credits={() => credits()!}
              hasWindows={() => rows().length > 0}
            />
          </If>
          <If when={resetCards() !== null}>
            <ResetCardsRow
              count={() => resetCards()!}
              hasAbove={() => rows().length > 0 || credits() !== null}
            />
          </If>
        </>
      </If>
    </View>
  );
}

/** 单个窗口格：顶行（标签/chip + 右侧百分比）/ 4px 条 / 紧凑倒计时。 */
function WindowCell(props: { window: () => UsageWindow }): JSX.Element {
  const pct = () => clampPercent(props.window().remainingPercent);
  const color = () => toneColor(remainingTone(pct()));
  const chip = () => windowChip(props.window());
  const reset = () => resetCountdownText(props.window().resetsAt);

  return (
    <View style={{ flex: 1, minWidth: 0 }}>
      <View style={{ display: "flex", flexDirection: "column", gap: 4, width: "100%" }}>
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 4,
            width: "100%",
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
              testID={`acct-win-label-${props.window().key}`}
              style={{
                flexShrink: 1,
                minWidth: 0,
                fontSize: 11,
                fontWeight: "600",
                color: paletteToken("fg_muted"),
                numberOfLines: 1,
                textOverflow: "ellipsis",
              }}
            >
              {windowLabel(props.window())}
            </Text>
            <If when={chip() !== null}>
              <Text
                style={{
                  fontSize: 10,
                  fontWeight: "500",
                  fontVariant: "tabular-nums",
                  color: paletteToken("fg_faint"),
                  flexShrink: 0,
                }}
              >
                {chip() ?? ""}
              </Text>
            </If>
          </View>
          <Text
            testID={`acct-pct-${props.window().key}`}
            style={{
              flexShrink: 0,
              textAlign: "right",
              fontSize: 11,
              fontWeight: "700",
              fontVariant: "tabular-nums",
              color: color(),
            }}
          >
            {percentText(props.window().remainingPercent)}
          </Text>
        </View>
        {/* 轨道 4px：填充宽度即读数，不做动画。 */}
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
        <If when={reset() !== null}>
          <Text
            style={{
              fontSize: 10,
              color: paletteToken("fg_faint"),
              fontVariant: "tabular-nums",
              numberOfLines: 1,
              textOverflow: "ellipsis",
            }}
          >
            {reset() ?? ""}
          </Text>
        </If>
      </View>
    </View>
  );
}

/** credits：不是窗口，置于内框底部。 */
function CreditsRow(props: { credits: () => Credits; hasWindows: () => boolean }): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 8,
        width: "100%",
        paddingTop: props.hasWindows() ? 6 : 0,
        borderTopWidth: props.hasWindows() ? 1 : 0,
        borderTopColor: paletteToken("border_soft"),
      }}
    >
      <Text
        style={{
          flexShrink: 1,
          minWidth: 0,
          fontSize: 11,
          fontWeight: "600",
          color: paletteToken("fg_muted"),
          numberOfLines: 1,
          textOverflow: "ellipsis",
        }}
      >
        {props.credits().label.trim().toLowerCase() === "credits"
          ? `${t("accountsPage.availableCredits")}:`
          : `${props.credits().label}:`}
      </Text>
      <Text
        testID="acct-credits"
        style={{
          flexShrink: 0,
          fontSize: 11,
          fontWeight: "700",
          fontVariant: "tabular-nums",
          color: paletteToken("fg"),
        }}
      >
        {formatCreditValue(props.credits(), t("accountsPage.unlimited"))}
      </Text>
    </View>
  );
}

/** 重置卡数量：置于内框底部（credits 之下），非进度条窗口。 */
function ResetCardsRow(props: { count: () => number; hasAbove: () => boolean }): JSX.Element {
  const positive = () => props.count() > 0;
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 8,
        width: "100%",
        paddingTop: props.hasAbove() ? 6 : 0,
        borderTopWidth: props.hasAbove() ? 1 : 0,
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
        <Icon name="rotateCcw" size={11} color={paletteToken("fg_muted")} />
        <Text
          style={{
            flexShrink: 1,
            minWidth: 0,
            fontSize: 11,
            fontWeight: "600",
            color: paletteToken("fg_muted"),
            numberOfLines: 1,
            textOverflow: "ellipsis",
          }}
        >
          {`${t("accountsPage.resetCards")}:`}
        </Text>
      </View>
      <View
        testID="acct-reset-cards"
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          paddingHorizontal: 7,
          height: 18,
          borderRadius: 999,
          borderWidth: 1,
          borderColor: positive() ? paletteToken("ok_border") : paletteToken("border"),
          backgroundColor: positive() ? paletteToken("ok_bg") : paletteToken("card"),
          flexShrink: 0,
        }}
      >
        <Text
          style={{
            fontSize: 11,
            fontWeight: "700",
            fontVariant: "tabular-nums",
            color: positive() ? paletteToken("ok") : paletteToken("fg_muted"),
          }}
        >
          {tf("accountsPage.resetCardsCount", { count: props.count() })}
        </Text>
      </View>
    </View>
  );
}
