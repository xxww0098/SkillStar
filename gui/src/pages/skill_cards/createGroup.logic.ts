// CreateGroupDialog 的纯状态逻辑，独立成模块以便无窗口单测（对位
// create_group.rs 里散在 render/create 里的计算；样板 noticeBoard.ts）。
// 每条函数都对位 GPUI 原实现的同一段：
//   - filterRows        create_group.rs:288-298（查询词对名称与描述的小写包含匹配）
//   - isDuplicateName   create_group.rs:286（非空名且与现有卡组重名）
//   - canCreate         create_group.rs:287（非 busy、名非空、成员非空、不重名）
//   - allMarked         create_group.rs:299-300（筛选结果非空且全部已选）
//   - sortedPillMembers create_group.rs:386-387（已安装成员在前，orphan 垫后，稳定排序）
//   - commitMembers     create_group.rs:175-176（提交前字典序排序）
//   - toggleAll         create_group.rs:485-503（全选/取消全选只作用于当前筛选结果）

/** `EMOJI_OPTIONS`（create_group.rs:26-28）。 */
export const EMOJI_OPTIONS: readonly string[] = [
  "💻", "🚀", "🎨", "🔧", "📦", "🧪", "📊", "🔐", "🌐", "📝", "⚡", "🤖", "🛠️", "📱", "🎯", "🧩",
];

/** 成员列表超过这么多行才滚动（create_group.rs PICKER_VISIBLE）。 */
export const PICKER_VISIBLE = 5;
/** 滚动时成员列表视口的固定像素高度（create_group.rs PICKER_VIEW_H）。 */
export const PICKER_VIEW_H = 160;
/** pill 条超过这么多枚才滚动（create_group.rs PILLS_VISIBLE）。 */
export const PILLS_VISIBLE = 20;
/** 滚动时 pill 条视口的固定像素高度（create_group.rs PILLS_VIEW_H）。 */
export const PILLS_VIEW_H = 140;

/** 成员选择列表的一行：已安装技能的名字 + 一行描述。 */
export interface DeckSkillRow {
  name: string;
  description: string;
}

/** 编辑态的卡组（只有表单要预填的字段；对位 SkillGroup 的一个投影）。 */
export interface DeckDraft {
  id: string;
  name: string;
  description: string;
  icon: string;
  skills: string[];
}

/** 筛选：查询词为空给全部；否则名称或描述的小写包含匹配。 */
export function filterRows(all: readonly DeckSkillRow[], query: string): DeckSkillRow[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return all.slice();
  return all.filter(
    (row) =>
      row.name.toLowerCase().includes(needle) || row.description.toLowerCase().includes(needle)
  );
}

/** 重名判定：trim 后非空且命中现有卡组名。 */
export function isDuplicateName(name: string, existing: readonly string[]): boolean {
  const trimmed = name.trim();
  return trimmed !== "" && existing.includes(trimmed);
}

/** Create/Save 可用：非 busy、名非空、成员非空、不重名。 */
export function canCreate(
  name: string,
  members: readonly string[],
  duplicate: boolean,
  busy: boolean
): boolean {
  return !busy && name.trim() !== "" && members.length > 0 && !duplicate;
}

/** 当前筛选结果是否全部已选（全选 chip 的文案/行为开关）。 */
export function allMarked(filtered: readonly DeckSkillRow[], members: readonly string[]): boolean {
  return filtered.length > 0 && filtered.every((row) => members.includes(row.name));
}

/** 成员集合的数组表达：toggle 一个名字，保持其余成员的原相对顺序。 */
export function toggleMember(members: readonly string[], name: string): string[] {
  const at = members.indexOf(name);
  if (at >= 0) return members.filter((_, ix) => ix !== at);
  return [...members, name];
}

/** 全选/取消全选：只作用于当前筛选结果，未筛出的成员不动。 */
export function toggleAll(
  filtered: readonly DeckSkillRow[],
  members: readonly string[],
  marked: boolean
): string[] {
  const scope = filtered.map((row) => row.name);
  if (marked) {
    const drop = new Set(scope);
    return members.filter((name) => !drop.has(name));
  }
  const held = new Set(members);
  return [...members, ...scope.filter((name) => !held.has(name))];
}

/** pill 条排序：已安装成员在前，orphan（未安装）垫后；同类内保持成员序。 */
export function sortedPillMembers(
  members: readonly string[],
  installed: ReadonlySet<string>
): string[] {
  return members
    .map((name, ix) => ({ name, ix }))
    .sort((a, b) => {
      const orphanA = installed.has(a.name) ? 0 : 1;
      const orphanB = installed.has(b.name) ? 0 : 1;
      return orphanA - orphanB || a.ix - b.ix;
    })
    .map((entry) => entry.name);
}

/** 提交前排序：成员按字典序写入（create_group.rs create() 的 members.sort()）。 */
export function commitMembers(members: readonly string[]): string[] {
  return [...members].sort();
}
