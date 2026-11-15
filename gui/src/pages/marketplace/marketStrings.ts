import { locale, type Locale } from "../../services/locale";
import { t as sharedT } from "../../services/translation";

// 市场页文案目录 — 值逐条取自 crates/ss-gpui/assets/locales/{zh-CN,en}.json
// 的 marketplace.*/toolbar.*/common.* 段（deckStrings.ts 同款约定：切片
// 自持键表，不编辑共享 services/translation.ts；zh/en 同步，术语表强制：
// Card=技能、Marketplace=市场、Uninstall=卸载、Install=安装、Stars→下载
// 量）。

const STRINGS: Record<Locale, Record<string, string>> = {
  "zh-CN": {
    "common.install": "安装",
    "common.uninstall": "卸载",
    "common.update": "更新",
    "common.refresh": "刷新",
    "common.retry": "重试",
    "toolbar.all": "全部",
    "toolbar.stars": "下载量 ↓",
    "toolbar.updated": "最近更新",
    "toolbar.viewGrid": "网格",
    "toolbar.viewList": "列表",
    "marketplace.searchPlaceholder": "搜索技能或发布者…",
    "marketplace.clearSearch": "清除搜索",
    "marketplace.trending": "趋势",
    "marketplace.hot": "热门",
    "marketplace.official": "官方",
    "marketplace.officialBadge": "官方",
    "marketplace.officialPublishersTitle": "官方发布者",
    "marketplace.publishersCount": "{{count}} 个发布者",
    "marketplace.repoCount": "{{count}} 个仓库",
    "marketplace.skillCount": "{{count}} 个技能",
    "marketplace.skillsCount": "{{count}} 个技能",
    "marketplace.subtitleAll": "浏览社区和生态里的全部技能",
    "marketplace.subtitleTrending": "各 AI 开发工具里增长最快的技能",
    "marketplace.subtitleHot": "按星标和使用量排行的热门技能",
    "marketplace.subtitleOfficial": "已验证的发布者和官方技能分发",
    "marketplace.refreshingSnapshot": "正在刷新本地快照...",
    "marketplace.seedingSnapshot": "正在初始化本地快照...",
    "marketplace.snapshotStale": "本地快照已过期",
    "marketplace.snapshotMiss": "本地快照尚未同步",
    "marketplace.snapshotErrorFallback": "本地快照不可用，当前展示在线数据",
    "marketplace.snapshotRemoteError": "更新失败",
    "marketplace.snapshotSynced": "快照已同步",
    "marketplace.snapshotWarning": "快照异常：{{err}}",
    "marketplace.retrying": "正在重试…",
    "marketplace.installedName": "已安装 {{name}}",
    "marketplace.uninstalledName": "已卸载 {{name}}",
    "marketplace.installFailedDetail": "安装失败：{{err}}",
    "marketplace.uninstallFailedDetail": "卸载失败：{{err}}",
    "marketplace.emptySearchTitle": "没有匹配「{{query}}」的技能",
    "marketplace.emptySearchDescription": "换个关键词，或清除搜索。",
    "marketplace.emptySectionTitle": "这一栏还没有技能",
    "marketplace.emptySectionDescription": "检查网络，或刷新快照。",
    "marketplace.backToTop": "回到顶部",
    "marketplace.removed": "已下架",
    "marketplace.spotlightTitle": "搜索技能",
    "marketplace.spotlightEmpty": "输入关键词筛选技能",
    "marketplace.detailInstalled": "已安装",
    "marketplace.detailLoading": "正在读取详情…",
    "marketplace.detailFailed": "无法读取详情。",
    "marketplace.detailSource": "来源",
    "marketplace.detailInstalls": "下载量",
    "marketplace.detailWeeklyInstalls": "周安装量",
    "marketplace.detailGithubStars": "GitHub 星标",
    "marketplace.detailFirstSeen": "首次出现",
    "marketplace.detailAudits": "安全审计",
    "detailPanel.description": "描述",
    "detailPanel.noDescription": "暂无描述。",
    "detailPanel.author": "作者",
    "detailPanel.lastUpdated": "最近更新",
    "detailPanel.gitSource": "Git 来源",
    "detailPanel.viewSkillMd": "查看 SKILL.md…",
    "detailPanel.viewOnSkillsSh": "在 skills.sh 查看",
    "detailPanel.showOriginal": "原文",
    "detailPanel.translate": "翻译",
    "detailPanel.translationRejected": "「{{model}}」拒绝了翻译请求。换一个模型后再试。",
    "detailPanel.translationRejectedDetail": "「{{model}}」无法翻译：{{detail}}。换一个模型后再试。",
    "skillCard.noDescription": "暂无描述",
    "publisherDetail.back": "返回",
    "publisherDetail.searchPlaceholder": "在 {{name}} 中搜索...",
    "publisherDetail.official": "官方",
    "publisherDetail.noneSelected": "还没有打开发布者。请从市场 → 官方进入。",
    "publisherDetail.noMatch": "没有匹配搜索的技能",
    "publisherDetail.noReposMatch": "没有匹配搜索的仓库",
    "publisherDetail.noSkills": "未找到技能",
    "publisherDetail.backToTop": "回到顶部",
    "marketplace.repositoriesCount": "{{count}} 个仓库",
    "marketplace.skillsTotal": "共 {{count}} 个技能",
    "marketplace.installsCount": "{{count}} 次安装",
    "marketplace.snapshotRefreshed": "快照已刷新",
    "marketplace.directSourceCommand": "识别到安装命令：{{source}}",
    "marketplace.directSourceRepo": "识别到仓库来源：{{source}}",
    "marketplace.directSourceHint": "可直接前往导入向导扫描并安装该来源的技能",
    "marketplace.directSourceAction": "前往导入安装",
    "page.marketDetailPlaceholder": "详情列随抽屉切片实装。",
    "page.publisherPlaceholder": "发布者页随发布者切片实装。",
  },
  en: {
    "common.install": "Install",
    "common.uninstall": "Uninstall",
    "common.update": "Update",
    "common.refresh": "Refresh",
    "common.retry": "Retry",
    "toolbar.all": "All",
    "toolbar.stars": "Downloads ↓",
    "toolbar.updated": "Updated",
    "toolbar.viewGrid": "Grid",
    "toolbar.viewList": "List",
    "marketplace.searchPlaceholder": "Search skills or publishers…",
    "marketplace.clearSearch": "Clear Search",
    "marketplace.trending": "Trending",
    "marketplace.hot": "Hot",
    "marketplace.official": "Official",
    "marketplace.officialBadge": "Official",
    "marketplace.officialPublishersTitle": "Official Publishers",
    "marketplace.publishersCount": "{{count}} publishers",
    "marketplace.repoCount": "{{count}} repos",
    "marketplace.skillCount": "{{count}} skills",
    "marketplace.skillsCount": "{{count}} cards",
    "marketplace.subtitleAll": "Browse all community and ecosystem agent skills",
    "marketplace.subtitleTrending": "Fastest growing skills across AI developer tools",
    "marketplace.subtitleHot": "Most popular skills ranked by stars and usage",
    "marketplace.subtitleOfficial": "Verified publishers and official skill distributions",
    "marketplace.refreshingSnapshot": "Refreshing snapshot...",
    "marketplace.seedingSnapshot": "Seeding local snapshot...",
    "marketplace.snapshotStale": "Snapshot is stale",
    "marketplace.snapshotMiss": "Snapshot not synced yet",
    "marketplace.snapshotErrorFallback": "Local snapshot unavailable — showing online data",
    "marketplace.snapshotRemoteError": "Update failed",
    "marketplace.snapshotSynced": "Snapshot synced",
    "marketplace.snapshotWarning": "Snapshot warning: {{err}}",
    "marketplace.retrying": "Retrying…",
    "marketplace.installedName": "Installed {{name}}",
    "marketplace.uninstalledName": "Uninstalled {{name}}",
    "marketplace.installFailedDetail": "Install failed: {{err}}",
    "marketplace.uninstallFailedDetail": "Uninstall failed: {{err}}",
    "marketplace.emptySearchTitle": "No cards matching \"{{query}}\"",
    "marketplace.emptySearchDescription": "Try a different keyword or clear the search.",
    "marketplace.emptySectionTitle": "Nothing in this section yet",
    "marketplace.emptySectionDescription": "Check the network connection, or refresh the snapshot.",
    "marketplace.backToTop": "Back to top",
    "marketplace.removed": "Removed",
    "marketplace.spotlightTitle": "Search skills",
    "marketplace.spotlightEmpty": "Type to filter skills",
    "marketplace.detailInstalled": "Installed",
    "marketplace.detailLoading": "Loading details…",
    "marketplace.detailFailed": "Couldn't load details.",
    "marketplace.detailSource": "Source",
    "marketplace.detailInstalls": "Installs",
    "marketplace.detailWeeklyInstalls": "Weekly installs",
    "marketplace.detailGithubStars": "GitHub stars",
    "marketplace.detailFirstSeen": "First seen",
    "marketplace.detailAudits": "Security audits",
    "detailPanel.description": "Description",
    "detailPanel.noDescription": "No description provided.",
    "detailPanel.author": "Author",
    "detailPanel.lastUpdated": "Last Updated",
    "detailPanel.gitSource": "Git Source",
    "detailPanel.viewSkillMd": "View SKILL.md…",
    "detailPanel.viewOnSkillsSh": "View on skills.sh",
    "detailPanel.showOriginal": "Original",
    "detailPanel.translate": "Translate",
    "detailPanel.translationRejected": "\"{{model}}\" rejected the translation. Pick another model and try again.",
    "detailPanel.translationRejectedDetail": "\"{{model}}\" can't translate this: {{detail}}. Pick another model and try again.",
    "skillCard.noDescription": "No description",
    "publisherDetail.back": "Back",
    "publisherDetail.searchPlaceholder": "Search in {{name}}...",
    "publisherDetail.official": "Official",
    "publisherDetail.noneSelected": "No publisher selected — open one from Marketplace → Official.",
    "publisherDetail.noMatch": "No cards match your search",
    "publisherDetail.noReposMatch": "No repos match your search",
    "publisherDetail.noSkills": "No cards found",
    "publisherDetail.backToTop": "Back to top",
    "marketplace.repositoriesCount": "{{count}} repositories",
    "marketplace.skillsTotal": "{{count}} skills total",
    "marketplace.installsCount": "{{count}} installs",
    "marketplace.snapshotRefreshed": "Snapshot refreshed",
    "marketplace.directSourceCommand": "Detected install command: {{source}}",
    "marketplace.directSourceRepo": "Detected repository source: {{source}}",
    "marketplace.directSourceHint": "Open the import wizard to scan and install skills from this source",
    "marketplace.directSourceAction": "Import Source",
    "page.marketDetailPlaceholder": "The detail column lands with the drawer slice.",
    "page.publisherPlaceholder": "The publisher page lands with the publisher slice.",
  },
};

// i18n.rs t()/tf() parity over this page's keys: current catalog first,
// then the other catalog, then the shared table, then the key itself.
export function mt(key: string): string {
  const current = locale();
  const other: Locale = current === "zh-CN" ? "en" : "zh-CN";
  const hit = STRINGS[current][key] ?? STRINGS[other][key] ?? sharedT(key);
  return hit;
}

export function mtf(key: string, vars: Record<string, string | number>): string {
  let text = mt(key);
  for (const [name, value] of Object.entries(vars)) {
    text = text.split(`{{${name}}}`).join(String(value));
  }
  return text;
}

/** 快照状态 → 工具栏弱化文案（mod.rs snapshot_label：Fresh 不显示，
 * 刷新中优先）。 */
export function snapshotLabel(
  status: string,
  refreshing: boolean
): string | null {
  if (refreshing) return mt("marketplace.refreshingSnapshot");
  switch (status) {
    case "fresh":
      return null;
    case "stale":
      return mt("marketplace.snapshotStale");
    case "seeding":
      return mt("marketplace.seedingSnapshot");
    case "miss":
      return mt("marketplace.snapshotMiss");
    case "error_fallback":
      return mt("marketplace.snapshotErrorFallback");
    case "remote_error":
      return mt("marketplace.snapshotRemoteError");
    default:
      return null;
  }
}
