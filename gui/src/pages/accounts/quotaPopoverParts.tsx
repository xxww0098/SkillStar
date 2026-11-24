// 全局账号额度下拉窗子部件：单 Agent 在役卡片、well 紧凑双列窗口槽、点数行、
// 失效重授权条与手风琴展开的备选切号行。独立成模块以保持各文件低于 800 行。

import { Text, View, computed, type JSX } from "nsh";
import { AgentIcon } from "../../chrome/agentRail";
import { IconSpin } from "../../chrome/feedback";
import { Icon } from "../../chrome/icons";
import { Tip } from "../../chrome/tooltip";
import { paletteToken } from "../../services/themeMode";
import { t } from "../../services/translation";
import { openAccountContextMenu } from "./accountContextMenu";
import { PlanBadge, QuotaErrorBadge, ServingBadge } from "./card";
import { maskAccountText } from "./logic";
import { AlternateAccountRow } from "./quotaPopoverAlternates";
import { compactIdentity, type PopoverAgentGroup } from "./quotaPopoverLogic";
import { CompactQuotaWell } from "./quotaPopoverMeters";
import { qt } from "./quotaPopoverStrings";
import type { AccountView } from "./types";

function isIdIn(ids: ReadonlySet<string> | readonly string[], id: string): boolean {
  return "has" in ids ? ids.has(id) : ids.includes(id);
}

export function AgentQuotaCard(props: {
  group: PopoverAgentGroup;
  expanded: boolean;
  masked: boolean;
  busyId: string | null;
  batchBusy: boolean;
  refreshingIds: ReadonlySet<string> | readonly string[];
  onToggleExpand: () => void;
  onActivate: (id: string) => void;
  onRefresh: (id: string) => void;
  onReauth: (id: string) => void;
  onOpenAgent: () => void;
}): JSX.Element {
  const canExpand = computed(() => props.group.alternates.length > 0);
  const primary = computed(() => props.group.primary);
  let lastPrimary: AccountView | null = null;
  const primaryOrLast = computed(() => {
    const p = primary();
    if (p !== null) lastPrimary = p;
    return p ?? lastPrimary;
  });
  const primaryId = computed(() => primaryOrLast()?.id ?? "");
  const primaryRefreshing = computed(() => {
    const p = primary();
    return p !== null && isIdIn(props.refreshingIds, p.id);
  });
  const primaryBusy = computed(() => {
    const p = primary();
    return (
      p !== null &&
      (props.busyId === p.id || props.batchBusy || isIdIn(props.refreshingIds, p.id))
    );
  });
  const isExpanded = computed(() => props.expanded && canExpand());
  const identityLine = (): string => {
    const p = primary();
    if (p !== null) {
      return compactIdentity(p, props.masked);
    }
    const raw = props.group.untrackedIdentity ?? "";
    return raw.length > 0
      ? maskAccountText(raw, props.masked)
      : qt("quotaPopover.untrackedLive");
  };

  return (
    <View
      testID={`quota-group-${props.group.agentId}`}
      onContextMenu={(e) => {
        const p = primary();
        if (p === null) return;
        openAccountContextMenu(e, {
          account: p,
          serving: props.group.primaryServing,
          busy: primaryBusy(),
          refreshing: primaryRefreshing(),
          onActivate: () => props.onActivate(p.id),
          onRefresh: () => props.onRefresh(p.id),
          onReauth: () => props.onReauth(p.id),
          onOpenAgent: props.onOpenAgent,
        });
      }}
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 7,
        width: "100%",
        paddingVertical: 9,
        paddingHorizontal: 10,
        borderRadius: 11,
        borderWidth: 1,
        borderColor: isExpanded()
          ? paletteToken("accent_soft_edge")
          : paletteToken("border"),
        backgroundColor: paletteToken("card"),
        overflow: "hidden",
      }}
    >
      {/* 顶部身份行：品牌图标 + 当前账号脱敏标识 + 套餐徽标 + 备选折叠开关 */}
      <View
        testID={`quota-group-head-${props.group.agentId}`}
        role={canExpand() ? "button" : undefined}
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 6,
          width: "100%",
          minWidth: 0,
          cursor: canExpand() ? "pointer" : "default",
        }}
        onPress={canExpand() ? props.onToggleExpand : undefined}
      >
        <AgentIcon id={props.group.agentId} size={16} />
        <Text
          testID={`quota-group-identity-${props.group.agentId}`}
          style={{
            flexShrink: 1,
            minWidth: 0,
            numberOfLines: 1,
            textOverflow: "ellipsis",
            fontSize: 12,
            fontWeight: "600",
            color: primary() !== null ? paletteToken("fg") : paletteToken("fg_muted"),
          }}
        >
          {identityLine()}
        </Text>

        <View
          style={{
            marginLeft: "auto",
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 4,
            flexShrink: 0,
          }}
        >
          <If when={primary() !== null}>
            <If when={Boolean(primaryOrLast()?.usage?.error)}>
              <QuotaErrorBadge
                id={`pop-${primaryId()}`}
                error={primaryOrLast()?.usage?.error ?? ""}
              />
            </If>
            <If
              when={props.group.primaryServing}
              else={
                <StandbyChip
                  onActivate={() => props.onActivate(primaryId())}
                  busy={primaryBusy()}
                />
              }
            >
              <ServingBadge id={`pop-${primaryId()}`} compact />
            </If>
            <PlanBadge id={`pop-${primaryId()}`} account={primaryOrLast()!} compact />
          </If>

          <If
            when={canExpand()}
            else={
              <If when={primary() !== null}>
                <View
                  testID={`quota-refresh-${primaryId()}`}
                  role="button"
                  tooltip={<Tip text={t("common.refresh")} />}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    width: 20,
                    height: 20,
                    borderRadius: 6,
                    cursor: primaryBusy() ? "default" : "pointer",
                  }}
                  hoverStyle={
                    primaryBusy() ? undefined : { backgroundColor: paletteToken("panel_hover") }
                  }
                  onPress={primaryBusy() ? undefined : () => props.onRefresh(primaryId())}
                >
                  <IconSpin
                    name="refresh"
                    size={11}
                    color={
                      primaryRefreshing() ? paletteToken("accent") : paletteToken("fg_faint")
                    }
                    spin={primaryRefreshing()}
                  />
                </View>
              </If>
            }
          >
            <View
              testID={`quota-group-expand-${props.group.agentId}`}
              style={{
                display: "flex",
                flexDirection: "row",
                alignItems: "center",
                gap: 2,
                paddingHorizontal: 5,
                height: 18,
                borderRadius: 6,
                backgroundColor: isExpanded()
                  ? paletteToken("accent_soft")
                  : paletteToken("well"),
              }}
            >
              <Text
                style={{
                  fontSize: 10,
                  fontWeight: "600",
                  fontVariant: "tabular-nums",
                  color: isExpanded()
                    ? paletteToken("accent_fg")
                    : paletteToken("fg_muted"),
                }}
              >
                {`1/${props.group.totalCount}`}
              </Text>
              <Icon
                name={isExpanded() ? "chevronUp" : "chevronDown"}
                size={10}
                color={isExpanded() ? paletteToken("accent_fg") : paletteToken("fg_muted")}
              />
            </View>
          </If>
        </View>
      </View>

      {/* 主账号配额槽 / 重新授权行 / 未入库提示 */}
      <If
        when={primary() === null}
        else={
          <If
            when={Boolean(primaryOrLast()?.requiresReauth)}
            else={<CompactQuotaWell account={primaryOrLast()!} />}
          >
            <CompactReauthBar
              id={primaryId()}
              busy={primaryBusy()}
              onReauth={() => props.onReauth(primaryId())}
            />
          </If>
        }
      >
        <View
          testID={`quota-untracked-${props.group.agentId}`}
          role="button"
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            justifyContent: "space-between",
            paddingHorizontal: 8,
            paddingVertical: 6,
            borderRadius: 8,
            borderWidth: 1,
            borderColor: paletteToken("border_soft"),
            backgroundColor: paletteToken("well"),
            cursor: "pointer",
          }}
          hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
          onPress={props.onOpenAgent}
        >
          <Text
            style={{
              flexShrink: 1,
              minWidth: 0,
              numberOfLines: 1,
              textOverflow: "ellipsis",
              fontSize: 11,
              color: paletteToken("fg_muted"),
            }}
          >
            {qt("quotaPopover.importToTrack")}
          </Text>
          <Icon name="chevronRight" size={12} color={paletteToken("fg_faint")} />
        </View>
      </If>

      {/* 手风琴展开区：同 Agent 下的备选账号一键切换列表 */}
      <If when={isExpanded()}>
        <View
          testID={`quota-alternates-${props.group.agentId}`}
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 4,
            paddingTop: 6,
            borderTopWidth: 1,
            borderTopColor: paletteToken("border_soft"),
            width: "100%",
          }}
        >
          <View
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              justifyContent: "space-between",
            }}
          >
            <Text style={{ fontSize: 10, fontWeight: "600", color: paletteToken("fg_faint") }}>
              {qt("quotaPopover.alternatesHeader")}
            </Text>
            <If when={primary() !== null}>
              <View
                testID={`quota-refresh-expanded-${primaryId()}`}
                role="button"
                style={{
                  display: "flex",
                  flexDirection: "row",
                  alignItems: "center",
                  gap: 3,
                  paddingHorizontal: 5,
                  paddingVertical: 2,
                  borderRadius: 5,
                  cursor: primaryBusy() ? "default" : "pointer",
                }}
                hoverStyle={
                  primaryBusy() ? undefined : { backgroundColor: paletteToken("panel_hover") }
                }
                onPress={primaryBusy() ? undefined : () => props.onRefresh(primaryId())}
              >
                <IconSpin
                  name="refresh"
                  size={10}
                  color={paletteToken("fg_muted")}
                  spin={primaryRefreshing()}
                />
                <Text style={{ fontSize: 10, color: paletteToken("fg_muted") }}>
                  {t("common.refresh")}
                </Text>
              </View>
            </If>
          </View>

          <Each of={props.group.alternates} key={(alt) => alt.id}>
            {(alt) => (
              <AlternateAccountRow
                account={alt()}
                masked={props.masked}
                switching={props.busyId === alt().id}
                refreshing={isIdIn(props.refreshingIds, alt().id)}
                disabled={
                  props.busyId !== null ||
                  props.batchBusy ||
                  isIdIn(props.refreshingIds, alt().id)
                }
                onActivate={() => props.onActivate(alt().id)}
                onRefresh={() => props.onRefresh(alt().id)}
                onReauth={() => props.onReauth(alt().id)}
                onOpenAgent={props.onOpenAgent}
              />
            )}
          </Each>
        </View>
      </If>
    </View>
  );
}

function StandbyChip(props: { busy: boolean; onActivate: () => void }): JSX.Element {
  return (
    <View
      role="button"
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 3,
        paddingHorizontal: 6,
        height: 18,
        borderRadius: 999,
        borderWidth: 1,
        borderColor: paletteToken("ok_border"),
        backgroundColor: paletteToken("ok_bg"),
        opacity: props.busy ? 0.5 : 1,
        cursor: props.busy ? "default" : "pointer",
        flexShrink: 0,
      }}
      onPress={props.busy ? undefined : props.onActivate}
    >
      <Icon name="play" size={9} color={paletteToken("ok")} />
      <Text style={{ fontSize: 10, fontWeight: "600", color: paletteToken("ok") }}>
        {t("accountsPage.switch")}
      </Text>
    </View>
  );
}

function CompactReauthBar(props: {
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
        gap: 6,
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
          overflow: "hidden",
        }}
      >
        <Icon name="alertTriangle" size={11} color={paletteToken("danger")} />
        <Text
          style={{
            flexShrink: 1,
            minWidth: 0,
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
        testID={`quota-reauth-${props.id}`}
        role="button"
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          height: 20,
          paddingHorizontal: 7,
          borderRadius: 5,
          backgroundColor: paletteToken("danger"),
          opacity: props.busy ? 0.5 : 1,
          cursor: props.busy ? "default" : "pointer",
          flexShrink: 0,
        }}
        hoverStyle={props.busy ? undefined : { backgroundColor: paletteToken("danger_hover") }}
        onPress={props.busy ? undefined : props.onReauth}
      >
        <Text style={{ fontSize: 10, fontWeight: "600", color: paletteToken("on_accent") }}>
          {t("accountsPage.reauthorize")}
        </Text>
      </View>
    </View>
  );
}
