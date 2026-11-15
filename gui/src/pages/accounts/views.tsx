// 账号页多视图（网格 / 列表 / 紧凑）；状态占位与骨架见 states.tsx。
// 共享逻辑复用 card.tsx 与 meters.tsx 导出的原语，不重复实现徽章或配额色阶。

import { Text, View, type JSX } from "nsh";
import { Icon } from "../../chrome/icons";
import { CARD_GAP, SKILL_CARD_W } from "../../layout";
import { paletteToken } from "../../services/themeMode";
import { t } from "../../services/translation";
import {
  AccountActionButtons,
  AccountCard,
  LastUsedMeta,
  PlanBadge,
  QuotaErrorBadge,
  ReauthRow,
  SelectCheckbox,
  ServingBadge,
} from "./card";
import {
  clampPercent,
  formatCreditValue,
  hasQuota,
  loginLine,
  maskAccountText,
  percentText,
  remainingTone,
  windowLabel,
  windowOrder,
} from "./logic";
import { toneColor } from "./meters";
import type { AccountView } from "./types";

export interface AccountsViewProps {
  accounts: AccountView[];
  servingId: string | null;
  busyId: string | null;
  batchBusy: boolean;
  refreshingIds: string[];
  selectedIds: string[];
  masked: boolean;
  onToggleSelect: (id: string) => void;
  onRefresh: (id: string) => void;
  onActivate: (id: string) => void;
  onDelete: (id: string) => void;
  onReauth: (id: string) => void;
}

/** 网格卡片视图：桌面 3 列（1040px），窄窗自适应折行。 */
export function AccountGridView(props: AccountsViewProps): JSX.Element {
  return (
    <View
      testID="acct-grid-view"
      style={{
        display: "flex",
        flexDirection: "row",
        flexWrap: "wrap",
        gap: CARD_GAP,
        width: "100%",
      }}
    >
      {props.accounts.map((acct) => (
        <AccountCard
          account={acct}
          serving={props.servingId === acct.id}
          busy={props.busyId === acct.id || props.batchBusy}
          refreshing={props.refreshingIds.includes(acct.id)}
          selected={props.selectedIds.includes(acct.id)}
          masked={props.masked}
          onToggleSelect={() => props.onToggleSelect(acct.id)}
          onRefresh={() => props.onRefresh(acct.id)}
          onActivate={() => props.onActivate(acct.id)}
          onDelete={() => props.onDelete(acct.id)}
          onReauth={() => props.onReauth(acct.id)}
        />
      ))}
    </View>
  );
}

/** 紧凑卡片视图：单卡压缩至姓名+徽章+单行状态芯片+底部时间/快捷操作。 */
export function AccountCompactView(props: AccountsViewProps): JSX.Element {
  return (
    <View
      testID="acct-compact-view"
      style={{
        display: "flex",
        flexDirection: "row",
        flexWrap: "wrap",
        gap: CARD_GAP,
        width: "100%",
      }}
    >
      {props.accounts.map((acct) => {
        const serving = props.servingId === acct.id;
        const selected = props.selectedIds.includes(acct.id);
        const refreshing = props.refreshingIds.includes(acct.id);
        const busy = props.busyId === acct.id || props.batchBusy;
        const windows = acct.usage ? windowOrder(acct.usage.windows) : [];

        return (
          <View
            testID={`acct-card-${acct.id}`}
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 8,
              width: SKILL_CARD_W,
              paddingVertical: 10,
              paddingHorizontal: 12,
              borderRadius: 12,
              borderWidth: 1,
              backgroundColor: selected ? paletteToken("card_active") : paletteToken("card"),
              borderColor:
                selected || serving ? paletteToken("accent") : paletteToken("border"),
            }}
            hoverStyle={{
              backgroundColor: selected
                ? paletteToken("card_active_hover")
                : paletteToken("card_hover"),
              borderColor: selected || serving ? paletteToken("accent") : paletteToken("edge"),
            }}
          >
            <View
              style={{
                display: "flex",
                flexDirection: "row",
                alignItems: "center",
                gap: 6,
              }}
            >
              <SelectCheckbox
                testID={`acct-select-${acct.id}`}
                checked={selected}
                label={acct.displayName}
                onPress={() => props.onToggleSelect(acct.id)}
              />
              <Text
                testID={`acct-name-${acct.id}`}
                style={{
                  flexShrink: 1,
                  minWidth: 0,
                  numberOfLines: 1,
                  textOverflow: "ellipsis",
                  fontSize: 13,
                  fontWeight: "600",
                  color: paletteToken("fg"),
                }}
              >
                {maskAccountText(acct.displayName, props.masked)}
              </Text>
              {serving ? <ServingBadge id={acct.id} compact /> : null}
              {acct.usage?.error ? (
                <QuotaErrorBadge id={acct.id} error={acct.usage.error} />
              ) : null}
              <View style={{ marginLeft: "auto", flexShrink: 0 }}>
                <PlanBadge id={acct.id} account={acct} compact />
              </View>
            </View>

            {acct.requiresReauth ? (
              <ReauthRow id={acct.id} busy={busy} onReauth={() => props.onReauth(acct.id)} />
            ) : windows.length > 0 ? (
              <View
                style={{
                  display: "flex",
                  flexDirection: "row",
                  flexWrap: "wrap",
                  gap: 6,
                }}
              >
                {windows.map((w) => {
                  const color = toneColor(remainingTone(w.remainingPercent));
                  return (
                    <View
                      style={{
                        display: "flex",
                        flexDirection: "row",
                        alignItems: "center",
                        gap: 5,
                        paddingHorizontal: 7,
                        paddingVertical: 3,
                        borderRadius: 6,
                        borderWidth: 1,
                        borderColor: paletteToken("border_soft"),
                        backgroundColor: paletteToken("well"),
                      }}
                    >
                      <View
                        style={{
                          width: 6,
                          height: 6,
                          borderRadius: 999,
                          backgroundColor: color,
                          flexShrink: 0,
                        }}
                      />
                      <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
                        {windowLabel(w)}
                      </Text>
                      <Text
                        testID={`acct-pct-${w.key}`}
                        style={{
                          fontSize: 11,
                          fontWeight: "700",
                          color,
                          fontVariant: "tabular-nums",
                        }}
                      >
                        {percentText(w.remainingPercent)}
                      </Text>
                    </View>
                  );
                })}
              </View>
            ) : acct.usage?.credits ? (
              <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
                {`${acct.usage.credits.label} ${formatCreditValue(acct.usage.credits, t("accountsPage.unlimited"))}`}
              </Text>
            ) : (
              <Text style={{ fontSize: 11, color: paletteToken("fg_faint") }}>
                {t("accountsPage.noQuota")}
              </Text>
            )}

            <View
              style={{
                display: "flex",
                flexDirection: "row",
                alignItems: "center",
                justifyContent: "space-between",
                gap: 6,
                paddingTop: 6,
                borderTopWidth: 1,
                borderTopColor: paletteToken("border_soft"),
              }}
            >
              <LastUsedMeta account={acct} />
              <AccountActionButtons
                account={acct}
                serving={serving}
                busy={busy}
                refreshing={refreshing}
                onRefresh={() => props.onRefresh(acct.id)}
                onActivate={() => props.onActivate(acct.id)}
                onDelete={() => props.onDelete(acct.id)}
              />
            </View>
          </View>
        );
      })}
    </View>
  );
}

/** 列表视图：紧凑表格式单行展示多窗口进度条、最近使用与操作列。 */
export function AccountListView(props: AccountsViewProps): JSX.Element {
  return (
    <View
      testID="acct-list-view"
      style={{
        display: "flex",
        flexDirection: "column",
        width: "100%",
        borderRadius: 14,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        overflow: "hidden",
      }}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 12,
          height: 36,
          paddingHorizontal: 14,
          backgroundColor: paletteToken("well"),
          borderBottomWidth: 1,
          borderBottomColor: paletteToken("border"),
        }}
      >
        <View style={{ width: 22, flexShrink: 0 }} />
        <Text
          style={{
            flex: 2.2,
            minWidth: 180,
            fontSize: 11,
            fontWeight: "600",
            color: paletteToken("fg_muted"),
          }}
        >
          {t("accountsPage.colAccount")}
        </Text>
        <Text
          style={{
            flex: 3,
            minWidth: 220,
            fontSize: 11,
            fontWeight: "600",
            color: paletteToken("fg_muted"),
          }}
        >
          {t("accountsPage.colQuota")}
        </Text>
        <Text
          style={{
            width: 116,
            flexShrink: 0,
            fontSize: 11,
            fontWeight: "600",
            color: paletteToken("fg_muted"),
          }}
        >
          {t("accountsPage.colLastUsed")}
        </Text>
        <Text
          style={{
            width: 100,
            flexShrink: 0,
            textAlign: "right",
            fontSize: 11,
            fontWeight: "600",
            color: paletteToken("fg_muted"),
          }}
        >
          {t("accountsPage.colActions")}
        </Text>
      </View>

      {props.accounts.map((acct, index) => {
        const serving = props.servingId === acct.id;
        const selected = props.selectedIds.includes(acct.id);
        const refreshing = props.refreshingIds.includes(acct.id);
        const busy = props.busyId === acct.id || props.batchBusy;

        return (
          <View
            testID={`acct-card-${acct.id}`}
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              gap: 12,
              minHeight: 56,
              paddingHorizontal: 14,
              paddingVertical: 10,
              borderTopWidth: index > 0 ? 1 : 0,
              borderTopColor: paletteToken("border_soft"),
              backgroundColor: selected ? paletteToken("card_active") : "transparent",
            }}
            hoverStyle={{
              backgroundColor: selected
                ? paletteToken("card_active_hover")
                : paletteToken("panel_hover"),
            }}
          >
            <SelectCheckbox
              testID={`acct-select-${acct.id}`}
              checked={selected}
              label={acct.displayName}
              onPress={() => props.onToggleSelect(acct.id)}
            />

            <View
              style={{
                display: "flex",
                flexDirection: "column",
                gap: 2,
                flex: 2.2,
                minWidth: 180,
              }}
            >
              <View
                style={{
                  display: "flex",
                  flexDirection: "row",
                  alignItems: "center",
                  gap: 6,
                }}
              >
                <Text
                  testID={`acct-name-${acct.id}`}
                  style={{
                    flexShrink: 1,
                    minWidth: 0,
                    numberOfLines: 1,
                    textOverflow: "ellipsis",
                    fontSize: 13,
                    fontWeight: "600",
                    color: paletteToken("fg"),
                  }}
                >
                  {maskAccountText(acct.displayName, props.masked)}
                </Text>
                {serving ? <ServingBadge id={acct.id} compact /> : null}
                <PlanBadge id={acct.id} account={acct} compact />
                {acct.usage?.error ? (
                  <QuotaErrorBadge id={acct.id} error={acct.usage.error} />
                ) : null}
              </View>
              <Text
                style={{
                  numberOfLines: 1,
                  textOverflow: "ellipsis",
                  fontSize: 11,
                  color: paletteToken("fg_muted"),
                }}
              >
                {maskAccountText(loginLine(acct), props.masked)}
              </Text>
            </View>

            <View style={{ flex: 3, minWidth: 220 }}>
              {acct.requiresReauth ? (
                <ReauthRow id={acct.id} busy={busy} onReauth={() => props.onReauth(acct.id)} />
              ) : !hasQuota(acct.usage) ? (
                <Text style={{ fontSize: 11, color: paletteToken("fg_faint") }}>
                  {t("accountsPage.noQuota")}
                </Text>
              ) : acct.usage && acct.usage.windows.length === 0 && acct.usage.credits ? (
                <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
                  {`${acct.usage.credits.label} · ${formatCreditValue(acct.usage.credits, t("accountsPage.unlimited"))}`}
                </Text>
              ) : (
                <View
                  style={{
                    display: "flex",
                    flexDirection: "row",
                    alignItems: "center",
                    flexWrap: "wrap",
                    gap: 10,
                  }}
                >
                  {windowOrder(acct.usage?.windows ?? []).map((w) => {
                    const pct = clampPercent(w.remainingPercent);
                    const color = toneColor(remainingTone(w.remainingPercent));
                    return (
                      <View
                        style={{
                          display: "flex",
                          flexDirection: "column",
                          gap: 3,
                          minWidth: 88,
                          flex: 1,
                          maxWidth: 140,
                        }}
                      >
                        <View
                          style={{
                            display: "flex",
                            flexDirection: "row",
                            alignItems: "center",
                            justifyContent: "space-between",
                            gap: 6,
                          }}
                        >
                          <Text
                            style={{
                              numberOfLines: 1,
                              textOverflow: "ellipsis",
                              fontSize: 11,
                              fontWeight: "500",
                              color: paletteToken("fg_muted"),
                            }}
                          >
                            {windowLabel(w)}
                          </Text>
                          <Text
                            testID={`acct-pct-${w.key}`}
                            style={{
                              fontSize: 11,
                              fontWeight: "700",
                              color,
                              fontVariant: "tabular-nums",
                            }}
                          >
                            {percentText(w.remainingPercent)}
                          </Text>
                        </View>
                        <View
                          style={{
                            width: "100%",
                            height: 4,
                            borderRadius: 999,
                            backgroundColor: paletteToken("well"),
                            overflow: "hidden",
                          }}
                        >
                          <View
                            style={{
                              width: `${pct}%`,
                              height: "100%",
                              borderRadius: 999,
                              backgroundColor: color,
                            }}
                          />
                        </View>
                      </View>
                    );
                  })}
                </View>
              )}
            </View>

            <View style={{ width: 116, flexShrink: 0 }}>
              <LastUsedMeta account={acct} />
            </View>

            <View
              style={{
                display: "flex",
                flexDirection: "row",
                justifyContent: "flex-end",
                width: 100,
                flexShrink: 0,
              }}
            >
              <AccountActionButtons
                account={acct}
                serving={serving}
                busy={busy}
                refreshing={refreshing}
                onRefresh={() => props.onRefresh(acct.id)}
                onActivate={() => props.onActivate(acct.id)}
                onDelete={() => props.onDelete(acct.id)}
              />
            </View>
          </View>
        );
      })}
    </View>
  );
}
