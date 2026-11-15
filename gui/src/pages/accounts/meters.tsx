// 额度区 —— 卡片中段的 well 软底内框：>=2 个窗口时按双列并排（chunkPairs），
// 每个窗口三层：顶行（标签 + 可选 chip + 右对齐剩余百分比）、中行（4px 进度条）、
// 底行（紧凑重置倒计时，完整复位时刻挂在 Tooltip）。Credits 置于内框底部。
//
// 分档：≥80 ok、≥40 warn、≥10 amber、<10 danger；颜色全走 paletteToken()。

import { Text, View, type JSX } from "nsh";
import { Tip } from "../../chrome/tooltip";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import {
  chunkPairs,
  clampPercent,
  formatCreditValue,
  hasQuota,
  nowSecs,
  percentText,
  remainingTone,
  resetState,
  windowChip,
  windowLabel,
  windowOrder,
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

/** 额度区主体（soft well 内框）：双列窗口格子 + credits 行；无数据时居中提示。 */
export function QuotaArea(props: { usage: UsageSnapshot | null | undefined }): JSX.Element {
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
      }}
    >
      {(() => {
        const usage = props.usage;
        if (!hasQuota(usage)) {
          return (
            <Text
              testID="acct-quota-empty"
              style={{
                fontSize: 12,
                color: paletteToken("fg_muted"),
                textAlign: "center",
                paddingVertical: 14,
              }}
            >
              {t("accountsPage.noQuota")}
            </Text>
          );
        }
        const pairs = chunkPairs(windowOrder(usage?.windows ?? []));
        const credits = usage?.credits ?? null;
        return (
          <>
            {pairs.map((pair) => (
              <View
                style={{
                  display: "flex",
                  flexDirection: "row",
                  alignItems: "flex-start",
                  gap: 10,
                  width: "100%",
                }}
              >
                {pair.map((window) => (
                  <WindowCell window={window} />
                ))}
              </View>
            ))}
            {credits !== null ? (
              <CreditsRow credits={credits} hasWindows={pairs.length > 0} />
            ) : null}
          </>
        );
      })()}
    </View>
  );
}

/** 单个窗口格：顶行（标签/chip + 右侧百分比）/ 4px 条 / 紧凑倒计时。 */
function WindowCell(props: { window: UsageWindow }): JSX.Element {
  const tipText = () => {
    const reset = resetState(props.window.resetsAt, nowSecs());
    const base = windowLabel(props.window);
    if (reset.kind === "in") {
      return `${base} · ${tf("accountsPage.resetIn", { span: reset.span, when: reset.when })}`;
    }
    if (reset.kind === "done") return `${base} · ${t("accountsPage.resetDone")}`;
    return base;
  };
  return (
    <View tooltip={<Tip text={tipText()} />} style={{ flex: 1, minWidth: 0 }}>
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
            }}
          >
            <Text
              style={{
                fontSize: 11,
                fontWeight: "600",
                color: paletteToken("fg_muted"),
                numberOfLines: 1,
                textOverflow: "ellipsis",
              }}
            >
              {windowLabel(props.window)}
            </Text>
            {(() => {
              const chip = windowChip(props.window);
              if (chip === null) return null;
              return (
                <Text
                  style={{
                    fontSize: 10,
                    fontWeight: "500",
                    fontVariant: "tabular-nums",
                    color: paletteToken("fg_faint"),
                    flexShrink: 0,
                  }}
                >
                  {chip}
                </Text>
              );
            })()}
          </View>
          <Text
            testID={`acct-pct-${props.window.key}`}
            style={{
              flexShrink: 0,
              textAlign: "right",
              fontSize: 11,
              fontWeight: "700",
              fontVariant: "tabular-nums",
              color: toneColor(remainingTone(clampPercent(props.window.remainingPercent))),
            }}
          >
            {percentText(props.window.remainingPercent)}
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
              width: `${clampPercent(props.window.remainingPercent)}%`,
              borderRadius: 999,
              backgroundColor: toneColor(
                remainingTone(clampPercent(props.window.remainingPercent)),
              ),
            }}
          />
        </View>
        {(() => {
          const reset = resetState(props.window.resetsAt, nowSecs());
          if (reset.kind === "none") return null;
          return (
            <Text
              style={{
                fontSize: 10,
                color: paletteToken("fg_faint"),
                fontVariant: "tabular-nums",
                numberOfLines: 1,
                textOverflow: "ellipsis",
              }}
            >
              {reset.kind === "done" ? t("accountsPage.resetDone") : reset.span}
            </Text>
          );
        })()}
      </View>
    </View>
  );
}

/** credits：不是窗口，置于内框底部。 */
function CreditsRow(props: { credits: Credits; hasWindows: boolean }): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 8,
        width: "100%",
        paddingTop: props.hasWindows ? 6 : 0,
        borderTopWidth: props.hasWindows ? 1 : 0,
        borderTopColor: paletteToken("border_soft"),
      }}
    >
      <Text
        style={{
          fontSize: 11,
          fontWeight: "600",
          color: paletteToken("fg_muted"),
          numberOfLines: 1,
          textOverflow: "ellipsis",
        }}
      >
        {props.credits.label.trim().toLowerCase() === "credits"
          ? `${t("accountsPage.availableCredits")}:`
          : `${props.credits.label}:`}
      </Text>
      <Text
        testID="acct-credits"
        style={{
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
