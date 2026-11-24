// 可复用的全局账号额度下拉窗面板（340pt 宽）：同时服务于系统状态栏/托盘弹出窗
// （residentWindow.ts 的 <Window kind="popup">）与主窗口侧栏底部快捷入口（Sidebar.tsx
// 的 <Overlay>）。严格遵守 nsh 铁律：组件体不预读 signal/props，颜色全在渲染期走
// paletteToken()，横向可省略文本一律带 flexShrink: 1 + minWidth: 0。

import { Scroll, Text, View, computed, signal, untracked, type JSX } from "nsh";
import { IconSpin } from "../../chrome/feedback";
import { Icon, type IconName } from "../../chrome/icons";
import { Tip } from "../../chrome/tooltip";
import { paletteToken } from "../../services/themeMode";
import { t } from "../../services/translation";
import { nextQuickAutoRefreshMs } from "./autoRefresh";
import {
  QUOTA_POPOVER_BODY_MAX_H,
  QUOTA_POPOVER_FOOTER_H,
  QUOTA_POPOVER_HEADER_H,
  QUOTA_POPOVER_MAX_H,
  QUOTA_POPOVER_W,
  buildPopoverAgentGroups,
  countPopoverAlerts,
  latestFetchedStamp,
  type PopoverAgentGroup,
} from "./quotaPopoverLogic";
import { AgentQuotaCard } from "./quotaPopoverParts";
import { qt, qtf } from "./quotaPopoverStrings";
import type { AccountsState, BannerState } from "./state";

const EMPTY_REFRESHING_SET: ReadonlySet<string> = new Set();

export interface QuotaPopoverPanelProps {
  state: AccountsState;
  /** 挂载在独立无边框透明原生弹出窗内时交由系统窗口按轮廓绘制外阴影，避免内阴影在窗口边界被裁切。 */
  inPopupWindow?: boolean;
  /** 控制面板是否处于激活/可见态；隐藏态下断开对共享账号状态的响应式订阅并停止所有旋转动画。 */
  active?: boolean;
  /** 点击「打开工作台」（可选聚焦到指定 agent）。 */
  onOpenWorkbench: (agentId?: string) => void;
  /** 点击「+ 添加」直接拉起主窗并打开添加账号对话框。 */
  onOpenAdd: (agentId?: string) => void;
  /** 点击「重新授权」直接拉起主窗并为该账号发起重授权。 */
  onReauthAccount: (accountId: string) => void;
}

export function QuotaPopoverPanel(props: QuotaPopoverPanelProps): JSX.Element {
  const expandedAgent = signal<string | null>(null);
  const isActive = (): boolean => props.active ?? true;

  let cachedGroups: PopoverAgentGroup[] = [];
  const groups = computed(() => {
    if (!isActive()) return cachedGroups;
    cachedGroups = buildPopoverAgentGroups(
      props.state.agents(),
      props.state.accounts(),
      props.state.live(),
    );
    return cachedGroups;
  });
  const hasGroups = computed(() => groups().length > 0);
  const alertCount = computed(() => countPopoverAlerts(groups()));

  let cachedAccountIds: string[] = [];
  const allAccountIds = computed(() => {
    if (!isActive()) return cachedAccountIds;
    cachedAccountIds = props.state.accounts().map((row) => row.id);
    return cachedAccountIds;
  });
  const hasAccounts = computed(() => allAccountIds().length > 0);

  const refreshingSet = computed<ReadonlySet<string>>(() =>
    isActive() ? new Set(props.state.refreshingIds()) : EMPTY_REFRESHING_SET,
  );
  const anyRefreshing = computed(() =>
    isActive()
      ? props.state.batchRefreshing() || props.state.refreshingIds().length > 0
      : false,
  );
  let cachedMasked = false;
  const masked = computed(() => {
    if (!isActive()) return cachedMasked;
    cachedMasked = props.state.privacyMode();
    return cachedMasked;
  });
  const busyId = computed(() => (isActive() ? props.state.busyId() : null));
  const batchBusy = computed(() => (isActive() ? props.state.batchBusy() !== null : false));
  const batchRefreshing = computed(() => (isActive() ? props.state.batchRefreshing() : false));
  const refreshAllDisabled = computed(
    () => !hasAccounts() || busyId() !== null || batchBusy() || batchRefreshing(),
  );

  let cachedAutoRefresh = untracked(() => props.state.autoRefreshInterval());
  const autoRefreshInterval = computed(() => {
    if (!isActive()) return cachedAutoRefresh;
    cachedAutoRefresh = props.state.autoRefreshInterval();
    return cachedAutoRefresh;
  });
  let cachedUpdatedStamp: string | null = null;
  const updatedStamp = computed(() => {
    if (!isActive()) return cachedUpdatedStamp;
    cachedUpdatedStamp = latestFetchedStamp(props.state.accounts());
    return cachedUpdatedStamp;
  });

  const activeBanner = computed<BannerState | null>(() =>
    isActive() ? props.state.banner() : null,
  );
  const activeError = computed<string | null>(() =>
    isActive() ? props.state.error() : null,
  );
  const feedbackError = computed<{ text: string; retryId?: string } | null>(() => {
    const b = activeBanner();
    if (b !== null) return { text: b.text, retryId: b.retryId };
    const err = activeError();
    return err ? { text: err } : null;
  });

  const toggleExpand = (agentId: string): void => {
    expandedAgent.set(expandedAgent() === agentId ? null : agentId);
  };

  const renderGroupList = (): JSX.Element => (
    <View style={{ display: "flex", flexDirection: "column", gap: 6, width: "100%" }}>
      <Each of={groups()} key={(group) => group.agentId}>
        {(group) => (
          <AgentQuotaCard
            group={group()}
            expanded={expandedAgent() === group().agentId}
            masked={masked()}
            busyId={busyId()}
            batchBusy={batchBusy()}
            refreshingIds={refreshingSet()}
            onToggleExpand={() => toggleExpand(group().agentId)}
            onActivate={(id) => props.state.activate(id)}
            onRefresh={(id) => props.state.refresh(id)}
            onReauth={(id) => props.onReauthAccount(id)}
            onOpenAgent={() => props.onOpenWorkbench(group().agentId)}
          />
        )}
      </Each>
    </View>
  );

  return (
    <View
      testID="quota-popover-panel"
      desktop={{ windowDrag: false }}
      style={{
        display: "flex",
        flexDirection: "column",
        width: QUOTA_POPOVER_W,
        maxHeight: QUOTA_POPOVER_MAX_H,
        borderRadius: 14,
        borderCurve: "continuous",
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("panel"),
        boxShadow: props.inPopupWindow
          ? undefined
          : "0 14px 38px rgba(0,0,0,0.32)",
        overflow: "hidden",
      }}
    >
      {/* 顶栏：标题 + 异常计数胶囊 + 隐私开关 / 全库刷新 / 打开工作台 */}
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 8,
          height: QUOTA_POPOVER_HEADER_H,
          paddingHorizontal: 12,
          borderTopLeftRadius: 13,
          borderTopRightRadius: 13,
          borderBottomWidth: 1,
          borderBottomColor: paletteToken("border_soft"),
          flexShrink: 0,
        }}
      >
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 6,
            flex: 1,
            minWidth: 0,
            overflow: "hidden",
          }}
        >
          <Icon name="gauge" size={14} color={paletteToken("accent_fg")} />
          <Text
            style={{
              fontSize: 13,
              fontWeight: "700",
              color: paletteToken("fg"),
              flexShrink: 0,
            }}
          >
            {qt("quotaPopover.title")}
          </Text>
          <If when={alertCount() > 0}>
            <View
              testID="quota-popover-alerts"
              style={{
                display: "flex",
                flexDirection: "row",
                alignItems: "center",
                paddingHorizontal: 6,
                height: 18,
                borderRadius: 999,
                borderWidth: 1,
                borderColor: paletteToken("danger_border"),
                backgroundColor: paletteToken("danger_bg"),
                flexShrink: 0,
              }}
            >
              <Text
                style={{
                  fontSize: 10,
                  fontWeight: "600",
                  fontVariant: "tabular-nums",
                  color: paletteToken("danger_fg"),
                }}
              >
                {qtf("quotaPopover.alertsCount", { count: alertCount() })}
              </Text>
            </View>
          </If>
        </View>

        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 4,
            flexShrink: 0,
          }}
        >
          <HeaderIconBtn
            testID="quota-popover-privacy"
            icon={masked() ? "eyeOff" : "eye"}
            tip={masked() ? t("accountsPage.privacyShow") : t("accountsPage.privacyHide")}
            active={masked()}
            onPress={() => props.state.togglePrivacy()}
          />
          <HeaderIconBtn
            testID="quota-popover-refresh-all"
            icon="refresh"
            tip={
              anyRefreshing()
                ? t("accountsPage.refreshingAll")
                : t("accountsPage.refreshAll")
            }
            spin={anyRefreshing()}
            disabled={refreshAllDisabled()}
            onPress={() => props.state.refreshAll(allAccountIds())}
          />
          <HeaderIconBtn
            testID="quota-popover-open-workbench"
            icon="externalLink"
            tip={qt("quotaPopover.openWorkbench")}
            onPress={() => props.onOpenWorkbench()}
          />
        </View>
      </View>

      {/* 悬浮窗内操作失败（切号 / 刷新 / 列表拉取）的紧凑可关闭错误条 */}
      <If when={feedbackError() !== null}>
        <PopoverErrorStrip
          text={feedbackError()?.text ?? ""}
          retryId={feedbackError()?.retryId}
          onRetry={(id) => props.state.refresh(id)}
          onClose={() => props.state.dismissError()}
        />
      </If>

      {/* 中段内容区：无账号展示空态；有账号一律挂 <Scroll>（视口框明确限高
          QUOTA_POPOVER_BODY_MAX_H，短内容收缩到内容高度；带错误条时 flexShrink 收缩
          保证顶栏、错误条与底栏不被 QUOTA_POPOVER_MAX_H 裁切）。 */}
      <If
        when={!hasGroups()}
        else={
          <Scroll
            testID="quota-popover-scroll"
            style={{
              width: "100%",
              maxHeight: QUOTA_POPOVER_BODY_MAX_H,
              flexShrink: 1,
              minHeight: 0,
            }}
            contentStyle={{
              display: "flex",
              flexDirection: "column",
              padding: 8,
              width: "100%",
            }}
          >
            {renderGroupList()}
          </Scroll>
        }
      >
        <EmptyPopoverBody onOpenWorkbench={() => props.onOpenWorkbench()} />
      </If>

      {/* 底栏：自动轮询/最近同步时刻 + 添加账号 + 工作台入口 */}
      <PopoverFooter
        autoRefreshInterval={autoRefreshInterval()}
        updatedStamp={updatedStamp()}
        onCycleAutoRefresh={() => {
          const next = nextQuickAutoRefreshMs(untracked(() => props.state.autoRefreshInterval()));
          props.state.setAutoRefreshInterval(next);
          if (next !== null) {
            props.state.refreshStale();
          }
        }}
        onAdd={() => props.onOpenAdd(untracked(() => groups()[0]?.agentId))}
        onOpenWorkbench={() => props.onOpenWorkbench()}
      />
    </View>
  );
}

function HeaderIconBtn(props: {
  testID: string;
  icon: IconName;
  tip: string;
  active?: boolean;
  spin?: boolean;
  disabled?: boolean;
  onPress: () => void;
}): JSX.Element {
  return (
    <View tooltip={<Tip text={props.tip} />}>
      <View
        testID={props.testID}
        role="button"
        accessibilityLabel={props.tip}
        accessibilityState={{ disabled: props.disabled === true, busy: props.spin === true }}
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          width: 26,
          height: 26,
          borderRadius: 7,
          borderWidth: 1,
          borderColor: props.active ? paletteToken("accent_soft_edge") : "transparent",
          backgroundColor: props.active ? paletteToken("accent_soft") : "transparent",
          opacity: props.disabled && !props.spin ? 0.45 : 1,
          cursor: props.disabled ? "default" : "pointer",
        }}
        hoverStyle={
          props.disabled
            ? undefined
            : {
                backgroundColor: props.active
                  ? paletteToken("accent_soft")
                  : paletteToken("panel_hover"),
              }
        }
        onPress={props.disabled ? undefined : props.onPress}
      >
        <If
          when={props.spin !== undefined}
          else={
            <Icon
              name={props.icon}
              size={13}
              color={props.active ? paletteToken("accent") : paletteToken("fg_muted")}
            />
          }
        >
          <IconSpin
            name={props.icon}
            size={13}
            color={props.active ? paletteToken("accent") : paletteToken("fg_muted")}
            spin={props.spin === true}
          />
        </If>
      </View>
    </View>
  );
}

function EmptyPopoverBody(props: { onOpenWorkbench: () => void }): JSX.Element {
  return (
    <View
      testID="quota-popover-empty"
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 8,
        paddingHorizontal: 20,
        paddingVertical: 26,
      }}
    >
      <View
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          width: 36,
          height: 36,
          borderRadius: 10,
          borderWidth: 1,
          borderColor: paletteToken("border"),
          backgroundColor: paletteToken("card"),
        }}
      >
        <Icon name="users" size={16} color={paletteToken("fg_muted")} />
      </View>
      <Text
        style={{
          fontSize: 13,
          fontWeight: "600",
          color: paletteToken("fg"),
          textAlign: "center",
        }}
      >
        {qt("quotaPopover.emptyTitle")}
      </Text>
      <Text
        style={{
          fontSize: 12,
          color: paletteToken("fg_muted"),
          textAlign: "center",
          lineHeight: 17,
        }}
      >
        {qt("quotaPopover.emptyHint")}
      </Text>
      <View
        testID="quota-popover-empty-cta"
        role="button"
        style={{
          marginTop: 4,
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 6,
          height: 30,
          paddingHorizontal: 12,
          borderRadius: 8,
          backgroundColor: paletteToken("accent"),
          cursor: "pointer",
        }}
        hoverStyle={{ backgroundColor: paletteToken("accent_hover") }}
        onPress={props.onOpenWorkbench}
      >
        <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("on_accent") }}>
          {qt("quotaPopover.emptyAction")}
        </Text>
      </View>
    </View>
  );
}

function PopoverFooter(props: {
  autoRefreshInterval: number | null;
  updatedStamp: string | null;
  onCycleAutoRefresh: () => void;
  onAdd: () => void;
  onOpenWorkbench: () => void;
}): JSX.Element {
  const metaLine = () => {
    const ms = props.autoRefreshInterval;
    const autoText =
      ms === null
        ? qt("quotaPopover.autoOff")
        : ms < 60_000
          ? qtf("quotaPopover.autoEverySec", { n: Math.round(ms / 1000) })
          : qtf("quotaPopover.autoEveryMin", { n: Math.round(ms / 60_000) });
    const stamp = props.updatedStamp;
    return stamp !== null
      ? `${autoText} · ${qtf("quotaPopover.updatedAt", { stamp })}`
      : autoText;
  };

  return (
    <View
      testID="quota-popover-footer"
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 8,
        height: QUOTA_POPOVER_FOOTER_H,
        paddingHorizontal: 10,
        borderTopWidth: 1,
        borderTopColor: paletteToken("border_soft"),
        borderBottomLeftRadius: 13,
        borderBottomRightRadius: 13,
        flexShrink: 0,
      }}
    >
      <View
        style={{
          flex: 1,
          minWidth: 0,
          overflow: "hidden",
          display: "flex",
          flexDirection: "row",
        }}
      >
        <View
          testID="quota-popover-auto-cycle"
          role="button"
          tooltip={<Tip text={qt("quotaPopover.autoToggleTip")} />}
          accessibilityLabel={qt("quotaPopover.autoToggleTip")}
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 4,
            maxWidth: "100%",
            minWidth: 0,
            height: 24,
            paddingHorizontal: 5,
            borderRadius: 6,
            cursor: "pointer",
            overflow: "hidden",
          }}
          hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
          onPress={props.onCycleAutoRefresh}
        >
          <Icon
            name="clock"
            size={11}
            color={
              props.autoRefreshInterval !== null
                ? paletteToken("accent_fg")
                : paletteToken("fg_faint")
            }
          />
          <Text
            style={{
              flexShrink: 1,
              minWidth: 0,
              numberOfLines: 1,
              textOverflow: "ellipsis",
              fontSize: 10,
              fontVariant: "tabular-nums",
              color: paletteToken("fg_faint"),
            }}
          >
            {metaLine()}
          </Text>
        </View>
      </View>

      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 4,
          flexShrink: 0,
        }}
      >
        <View
          testID="quota-popover-add"
          role="button"
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 3,
            height: 24,
            paddingHorizontal: 7,
            borderRadius: 6,
            cursor: "pointer",
          }}
          hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
          onPress={props.onAdd}
        >
          <Icon name="plus" size={11} color={paletteToken("fg_muted")} />
          <Text style={{ fontSize: 11, fontWeight: "600", color: paletteToken("fg_muted") }}>
            {qt("quotaPopover.addShort")}
          </Text>
        </View>

        <View
          testID="quota-popover-footer-workbench"
          role="button"
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 3,
            height: 24,
            paddingHorizontal: 7,
            borderRadius: 6,
            cursor: "pointer",
          }}
          hoverStyle={{ backgroundColor: paletteToken("accent_soft") }}
          onPress={props.onOpenWorkbench}
        >
          <Text style={{ fontSize: 11, fontWeight: "600", color: paletteToken("accent_fg") }}>
            {qt("quotaPopover.workbenchShort")}
          </Text>
          <Icon name="chevronRight" size={11} color={paletteToken("accent_fg")} />
        </View>
      </View>
    </View>
  );
}

function PopoverErrorStrip(props: {
  text: string;
  retryId?: string;
  onRetry: (id: string) => void;
  onClose: () => void;
}): JSX.Element {
  return (
    <View
      testID="quota-popover-error"
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 6,
        marginHorizontal: 8,
        marginTop: 6,
        marginBottom: 2,
        paddingVertical: 5,
        paddingHorizontal: 8,
        borderRadius: 8,
        borderWidth: 1,
        borderColor: paletteToken("danger_border"),
        backgroundColor: paletteToken("danger_bg"),
        flexShrink: 0,
      }}
    >
      <View style={{ flexShrink: 0, display: "flex", alignItems: "center" }}>
        <Icon name="alertTriangle" size={12} color={paletteToken("danger_fg")} />
      </View>
      <Text
        testID="quota-popover-error-text"
        style={{
          flex: 1,
          minWidth: 0,
          fontSize: 11,
          lineHeight: 15,
          numberOfLines: 2,
          textOverflow: "ellipsis",
          color: paletteToken("danger_fg"),
        }}
      >
        {props.text}
      </Text>
      <If when={Boolean(props.retryId)}>
        <View
          testID="quota-popover-error-retry"
          role="button"
          tooltip={<Tip text={qt("quotaPopover.retry")} />}
          accessibilityLabel={qt("quotaPopover.retry")}
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            width: 20,
            height: 20,
            borderRadius: 5,
            cursor: "pointer",
            flexShrink: 0,
          }}
          hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
          onPress={() => {
            if (props.retryId) props.onRetry(props.retryId);
          }}
        >
          <Icon name="refresh" size={11} color={paletteToken("danger_fg")} />
        </View>
      </If>
      <View
        testID="quota-popover-error-close"
        role="button"
        tooltip={<Tip text={qt("quotaPopover.dismissError")} />}
        accessibilityLabel={qt("quotaPopover.dismissError")}
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          width: 20,
          height: 20,
          borderRadius: 5,
          cursor: "pointer",
          flexShrink: 0,
        }}
        hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
        onPress={props.onClose}
      >
        <Icon name="x" size={12} color={paletteToken("danger_fg")} />
      </View>
    </View>
  );
}