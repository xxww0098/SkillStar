// 全局额度下拉窗纯计算层 —— 分组过滤、备选号配额排序与
// 屏幕边界防溢出定位。无副作用、不读 signal，可直接单测锁定全部几何与语义。

import type { Bounds, Point } from "nsh";
import {
  accountName,
  clampPercent,
  formatStamp,
  isServing,
  loginLine,
  maskAccountText,
  overallQuota,
} from "./logic";
import type { AccountView, AgentLive, AgentRow } from "./types";

export const QUOTA_POPOVER_W = 340;
/** 顶栏高度（quotaPopover.tsx 顶栏行高，随样式一起改）。 */
export const QUOTA_POPOVER_HEADER_H = 40;
/**
 * 内容视口上限：卡片列表比它高就交给 <Scroll> 滚动，而不是被面板裁切。
 * 视口框写明确限高，短内容仍收缩到内容高度（面板不会留白）。
 */
export const QUOTA_POPOVER_BODY_MAX_H = 392;
/** 底栏高度（quotaPopover.tsx 底栏行高）。 */
export const QUOTA_POPOVER_FOOTER_H = 34;
/** 面板最大高度 = 顶栏 + 内容视口上限 + 底栏 + 上下各 1px 描边。 */
export const QUOTA_POPOVER_MAX_H =
  QUOTA_POPOVER_HEADER_H + QUOTA_POPOVER_BODY_MAX_H + QUOTA_POPOVER_FOOTER_H + 2;
/** 首次弹出时给系统窗口的估算高度（弹出位置夹紧用）。 */
export const QUOTA_POPOVER_ESTIMATED_H = 420;
const SCREEN_MARGIN = 8;
const TRAY_OFFSET = 6;

/** 下拉窗中一个已配置或本机在役 Agent 的聚合模型。 */
export interface PopoverAgentGroup {
  agentId: string;
  agentName: string;
  /** 当前展示在主槽位的账号（在役优先，无在役时取首张入库账号；仅本机未入库时为 null）。 */
  primary: AccountView | null;
  /** primary 是否真正在本机客户端生效（磁盘真相）。 */
  primaryServing: boolean;
  /** 本机已登录但尚未导入账号库时的身份提示。 */
  untrackedIdentity: string | null;
  /** 除 primary 外的其余入库账号，按综合剩余配额降序排列（最充裕的排最前）。 */
  alternates: AccountView[];
  /** 该 Agent 入库总账号数。 */
  totalCount: number;
  /** 主账号是否存在异常（需重授权 / 读取错误 / 任一窗口低于 10%）。 */
  hasAlert: boolean;
}

function accountHasLowWindow(account: AccountView): boolean {
  const windows = account.usage?.windows ?? [];
  return windows.some((w) => clampPercent(w.remainingPercent) < 10);
}

export function accountNeedsAttention(account: AccountView | null): boolean {
  if (account === null) return false;
  if (account.requiresReauth) return true;
  if ((account.usage?.error ?? "").trim().length > 0) return true;
  return accountHasLowWindow(account);
}

/** 紧凑身份短文本：优先用户名，否则邮箱/accountId，并支持隐私脱敏。 */
export function compactIdentity(account: AccountView, masked: boolean): string {
  const name = accountName(account);
  const raw = name.length > 0 ? name : loginLine(account);
  return maskAccountText(raw, masked);
}

/**
 * 按受管 Agent 注册顺序构建下拉窗分组：
 * - 仅保留已有入库账号，或 `reconcile()` 探测到非空本机身份的客户端；
 * - 主槽优先选在役账号，其余备选按 `overallQuota` 综合剩余率降序排列。
 */
export function buildPopoverAgentGroups(
  agents: AgentRow[],
  accounts: AccountView[],
  live: AgentLive[],
): PopoverAgentGroup[] {
  const byAgent = new Map<string, AccountView[]>();
  for (const row of accounts) {
    const bucket = byAgent.get(row.agent);
    if (bucket !== undefined) {
      bucket.push(row);
    } else {
      byAgent.set(row.agent, [row]);
    }
  }

  const liveByAgent = new Map<string, AgentLive>();
  for (const entry of live) {
    liveByAgent.set(entry.agent, entry);
  }

  // 若 agents 尚未初始化但 accounts 已有数据，回退按出现顺序补齐。
  const agentList: AgentRow[] =
    agents.length > 0
      ? agents
      : Array.from(byAgent.keys()).map((id) => ({ id, displayName: id }));

  const groups: PopoverAgentGroup[] = [];
  for (const agent of agentList) {
    const rows = byAgent.get(agent.id) ?? [];
    const liveRow = liveByAgent.get(agent.id);
    const rawReconcileId = liveRow?.accountId ?? null;
    const reconcileId =
      rawReconcileId === null || rawReconcileId.trim().length === 0 ? null : rawReconcileId;
    const liveIdentity = (liveRow?.identity ?? liveRow?.displayName ?? "").trim();

    if (rows.length === 0 && liveIdentity.length === 0) {
      continue;
    }

    if (rows.length === 0) {
      groups.push({
        agentId: agent.id,
        agentName: agent.displayName,
        primary: null,
        primaryServing: false,
        untrackedIdentity: liveIdentity,
        alternates: [],
        totalCount: 0,
        hasAlert: false,
      });
      continue;
    }

    const serving = rows.find((row) => isServing(row, reconcileId)) ?? null;
    const primary = serving ?? rows[0];
    const alternates = rows
      .filter((row) => row.id !== primary.id)
      .slice()
      .sort((a, b) => {
        const qa = overallQuota(a);
        const qb = overallQuota(b);
        if (qa !== qb) return qb - qa;
        return a.createdAt - b.createdAt;
      });

    groups.push({
      agentId: agent.id,
      agentName: agent.displayName,
      primary,
      primaryServing: serving !== null,
      untrackedIdentity: null,
      alternates,
      totalCount: rows.length,
      hasAlert: accountNeedsAttention(primary),
    });
  }

  return groups;
}

/** 汇总所有主槽位需要关注的异常数（顶栏胶囊用）。 */
export function countPopoverAlerts(groups: PopoverAgentGroup[]): number {
  let count = 0;
  for (const g of groups) {
    if (g.hasAlert) count += 1;
  }
  return count;
}

/** 最近一次额度抓取时间戳（取所有快照的最大 fetchedAt）。 */
export function latestFetchedStamp(accounts: AccountView[]): string | null {
  let latest = 0;
  for (const row of accounts) {
    const ts = row.usage?.fetchedAt ?? 0;
    if (ts > latest) latest = ts;
  }
  if (latest <= 0) return null;
  const stamp = formatStamp(latest);
  return stamp.length > 0 ? stamp : null;
}



/**
 * 根据托盘图标 `bounds` 与显示器可用工作区 `visibleBounds` 计算弹出窗左上角屏幕坐标：
 * - macOS 状态栏在屏幕顶部：窗口水平居中于图标下方 `6pt`，左右夹紧在 `visibleBounds` 内；
 * - Windows 通知区默认在屏幕底部（若任务栏置顶则自动翻转向下）：优先置于图标上方并夹紧边界；
 * - `trayBounds` 缺失时（如 Linux 面板），回退到主显示器右上角安全区。
 */
export function computeTrayPopoverOrigin(opts: {
  trayBounds: Bounds | null;
  visibleBounds: Bounds | null;
  panelWidth?: number;
  panelHeight?: number;
  platform: string;
}): Point {
  const width = opts.panelWidth ?? QUOTA_POPOVER_W;
  const height = opts.panelHeight ?? QUOTA_POPOVER_ESTIMATED_H;
  const area: Bounds = opts.visibleBounds ?? {
    origin: { x: 0, y: 0 },
    size: { width: 1440, height: 900 },
  };

  const minX = area.origin.x + SCREEN_MARGIN;
  const maxX = Math.max(minX, area.origin.x + area.size.width - width - SCREEN_MARGIN);
  // 有托盘图标锚点时以 visibleBounds 顶边为下限，保留菜单栏下方 TRAY_OFFSET（6pt）紧贴感。
  const minY = opts.trayBounds !== null ? area.origin.y : area.origin.y + SCREEN_MARGIN;
  const maxY = Math.max(minY, area.origin.y + area.size.height - height - SCREEN_MARGIN);

  if (opts.trayBounds === null) {
    return { x: maxX, y: minY };
  }

  const icon = opts.trayBounds;
  const centeredX = Math.round(icon.origin.x + icon.size.width / 2 - width / 2);
  const x = Math.min(maxX, Math.max(minX, centeredX));

  const iconCenterY = icon.origin.y + icon.size.height / 2;
  const screenMidY = area.origin.y + area.size.height / 2;
  const openDownwards = opts.platform === "macos" || iconCenterY <= screenMidY;

  const rawY = openDownwards
    ? Math.round(icon.origin.y + icon.size.height + TRAY_OFFSET)
    : Math.round(icon.origin.y - height - TRAY_OFFSET);
  const y = Math.min(maxY, Math.max(minY, rawY));

  return { x, y };
}
