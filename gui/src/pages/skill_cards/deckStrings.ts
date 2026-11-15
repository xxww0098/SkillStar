import { locale, type Locale } from "../../services/locale";
import { t as sharedT } from "../../services/translation";

// Deck-page catalog — the `skillCards.*` (+ the few `common.*`/`toolbar.*`)
// entries this slice paints, lifted verbatim from
// crates/ss-gpui/assets/locales/{zh-CN,en}.json. The shared services/
// translation.ts table only ships the shell sections so far; rather than
// edit that shared file from a slice, the deck page keeps its own copy of
// exactly the keys it reads (zh/en in sync, glossary per
// docs/features/frontend/README.md 术语表) and falls back to the shared
// table for anything not listed here. When the shared catalog grows the
// full generated table, this module shrinks to a re-export.

const STRINGS: Record<Locale, Record<string, string>> = {
  "zh-CN": {
    "common.import": "导入",
    "common.moreActions": "更多操作",
    "common.moreCount": "还有 {{count}} 个",
    "common.delete": "删除",
    "toolbar.viewGrid": "网格",
    "toolbar.viewList": "列表",
    "skillCards.searchPlaceholder": "搜索卡组...",
    "skillCards.newGroup": "新建卡组…",
    "skillCards.duplicateDeck": "复制卡组",
    "skillCards.deleteDeck": "删除卡组…",
    "skillCards.processing": "正在处理卡组…",
    "skillCards.emptyTitle": "还没有卡组",
    "skillCards.emptyDesc": "把技能收进卡组，方便一起部署或分享。",
    "skillCards.createFirst": "创建第一个卡组…",
    "skillCards.noDescription": "暂无描述",
    "skillCards.skillsCount": "{{count}}",
    "skillCards.missingCount": "缺 {{count}} 个",
    "skillCards.linkAllTo": "将整组链接到 {{agent}}",
    "skillCards.unlinkAllFrom": "取消整组与 {{agent}} 的链接",
    "skillCards.shareDeck": "分享卡组",
    "skillCards.deleteDeckTitle": "删除「{{name}}」？",
    "skillCards.deleteDeckHint": "删除这个卡组。Hub 里的技能会保留。",
    "skillCards.noMatching": "没有匹配的卡组",
    "skillCards.tryDifferent": "请尝试其他搜索词。",
    "skillCards.batchTogglePartialFailed": "有 {{failed}}/{{total}} 个链接未能更新",
    "skillCards.batchTogglePartialSkipped":
      "已跳过 {{skipped}}/{{total}} 个链接：目标名称已被占用，未覆盖原目录",
  },
  en: {
    "common.import": "Import",
    "common.moreActions": "More actions",
    "common.moreCount": "+{{count}} more",
    "common.delete": "Delete",
    "toolbar.viewGrid": "Grid",
    "toolbar.viewList": "List",
    "skillCards.searchPlaceholder": "Search decks...",
    "skillCards.newGroup": "New deck…",
    "skillCards.duplicateDeck": "Duplicate deck",
    "skillCards.deleteDeck": "Delete deck…",
    "skillCards.processing": "Processing deck…",
    "skillCards.emptyTitle": "No decks yet",
    "skillCards.emptyDesc": "Group cards into a deck to deploy or share them together.",
    "skillCards.createFirst": "Create your first deck…",
    "skillCards.noDescription": "No description",
    "skillCards.skillsCount": "{{count}}",
    "skillCards.missingCount": "{{count}} missing",
    "skillCards.linkAllTo": "Link the whole deck to {{agent}}",
    "skillCards.unlinkAllFrom": "Unlink the whole deck from {{agent}}",
    "skillCards.shareDeck": "Share Deck",
    "skillCards.deleteDeckTitle": "Delete \"{{name}}\"?",
    "skillCards.deleteDeckHint": "Removes this deck bundle. Hub skills stay intact.",
    "skillCards.noMatching": "No matching decks",
    "skillCards.tryDifferent": "Try a different search term.",
    "skillCards.batchTogglePartialFailed": "Couldn't update {{failed}}/{{total}} links",
    "skillCards.batchTogglePartialSkipped":
      "Skipped {{skipped}}/{{total}} links — name already occupied on that Agent (left in place)",
  },
};

// i18n.rs t()/tf() parity over this slice's keys: current catalog first,
// then the other catalog, then the shared table, then the key itself.
export function dt(key: string): string {
  const current = locale();
  const other: Locale = current === "zh-CN" ? "en" : "zh-CN";
  const hit = STRINGS[current][key] ?? STRINGS[other][key] ?? sharedT(key);
  return hit;
}

export function dtf(key: string, vars: Record<string, string | number>): string {
  let text = dt(key);
  for (const [name, value] of Object.entries(vars)) {
    text = text.split(`{{${name}}}`).join(String(value));
  }
  return text;
}
