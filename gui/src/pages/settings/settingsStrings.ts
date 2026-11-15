import { locale, type Locale } from "../../services/locale";
import { t as sharedT } from "../../services/translation";

// Settings page localized copy (section subtitles, group headers, visual aids).
// Keeps zh-CN and en strictly in sync without inflating shared services/translation.ts.

const STRINGS: Record<Locale, Record<string, string>> = {
  "zh-CN": {
    "settings.sub.agents": "管理本机 Agent 技能目录连接、实时挂载状态与托管技能暂停策略",
    "settings.sub.general": "定制界面主题风格、显示语言、后台常驻行为与技能自动更新节奏",
    "settings.sub.network": "诊断远端连通性，配置 HTTP / SOCKS 代理与 GitHub 镜像加速链路",
    "settings.sub.translation": "设置技能描述与 SKILL.md 文档的自动翻译目标语言及模型接口",
    "settings.sub.storage": "查看 Hub 与缓存磁盘占用分布，维护目录健康度或纳管本机已有技能",
    "settings.sub.about": "查看当前客户端版本、检查更新，并核验本机 Git 与 GitHub CLI 工具链",
    "settings.group.appearance": "外观与语言",
    "settings.group.runtimeUpdates": "运行与自动更新",
    "settings.group.translationScope": "翻译范围与目标语言",
    "settings.group.translationEngine": "翻译引擎与模型凭证",
    "settings.group.storageOverview": "空间总览",
    "settings.group.storageDirectories": "目录明细与维护",
    "settings.group.toolchain": "环境与工具链诊断",
    "settings.mirrorPriorityPrimary": "首选",
    "settings.storageOther": "配置与历史",
    "settings.storageNoData": "暂无磁盘占用数据",
  },
  en: {
    "settings.sub.agents": "Manage local Agent skill directory connections, active links, and pause states",
    "settings.sub.general": "Customize appearance theme, display language, background behavior, and skill update cadence",
    "settings.sub.network": "Diagnose remote connectivity and configure HTTP / SOCKS proxy and GitHub mirror acceleration",
    "settings.sub.translation": "Configure automatic translation scope, target language, and LLM endpoint for skills",
    "settings.sub.storage": "Inspect Hub and cache disk usage breakdown, repair broken links, and adopt local skills",
    "settings.sub.about": "Check current build version, release updates, and local Git / GitHub CLI toolchain status",
    "settings.group.appearance": "Appearance & Language",
    "settings.group.runtimeUpdates": "Runtime & Skill Updates",
    "settings.group.translationScope": "Scope & Target Language",
    "settings.group.translationEngine": "Translation Engine & Credentials",
    "settings.group.storageOverview": "Storage Overview",
    "settings.group.storageDirectories": "Directories & Maintenance",
    "settings.group.toolchain": "Environment & Toolchain",
    "settings.mirrorPriorityPrimary": "Primary",
    "settings.storageOther": "Config & History",
    "settings.storageNoData": "No disk usage recorded",
  },
};

Object.assign(STRINGS["zh-CN"], {
  "settings.defaultDeployMode": "默认部署方式",
  "settings.defaultDeployModeHint": "新建关联或开关同步时默认使用软链或副本；单个智能体可单独覆盖。",
  "settings.deployModeSymlink": "软链",
  "settings.deployModeCopy": "副本",
  "settings.deployModeInherit": "跟随默认",
  "settings.agentDeployMode": "部署方式",
  "settings.deployModeUpdated": "部署方式已切换（更新 {{updated}} 处）",
  "settings.deployModeUpdatedWithSkipped": "部署方式已切换（更新 {{updated}} 处，跳过 {{skipped}} 处本地修改或外部目录）",
  "settings.deployModeSaveFailed": "切换部署方式失败",
  "settings.disallowJunctionHint": "该工具不支持 Windows Junction，将回退为普通符号链接或副本。",
  "settings.backupsTitle": "技能快照备份",
  "settings.backupsSubtitle": "卸载、更新覆盖或副本冲突保护前自动保留的历史快照（最多 25 份）。",
  "settings.backupsEmpty": "暂无技能备份快照",
  "settings.backupsRestore": "恢复",
  "settings.backupsClearAll": "清空全部备份",
  "settings.backupsClearConfirmTitle": "清空全部备份？",
  "settings.backupsClearConfirmDesc": "将永久删除 {{count}} 份备份快照，释放 {{size}} 磁盘空间。",
  "settings.backupsClearConfirmBtn": "清空",
  "settings.backupsRestored": "已恢复备份：{{name}}",
  "settings.backupsRestoreFailed": "恢复备份失败",
  "settings.backupsCleared": "已清空 {{count}} 份备份（释放 {{size}}）",
  "settings.backupsClearFailed": "清空备份失败",
  "settings.backupReasonUninstall": "卸载保留",
  "settings.backupReasonUpdate": "更新前保留",
  "settings.backupReasonCopyConflict": "副本冲突保护",
  "settings.backupFromCalled": "原名 {{called}}",
  "settings.backupsOpenFolder": "打开目录",
});

Object.assign(STRINGS.en, {
  "settings.defaultDeployMode": "Default Deploy Mode",
  "settings.defaultDeployModeHint": "Prefer symlinks or copies when syncing skills; individual agents can override below.",
  "settings.deployModeSymlink": "Symlink",
  "settings.deployModeCopy": "Copy",
  "settings.deployModeInherit": "Inherit",
  "settings.agentDeployMode": "Deploy mode",
  "settings.deployModeUpdated": "Deploy mode updated ({{updated}} reconciled)",
  "settings.deployModeUpdatedWithSkipped": "Deploy mode updated ({{updated}} reconciled, {{skipped}} modified/foreign skipped)",
  "settings.deployModeSaveFailed": "Failed to switch deploy mode",
  "settings.disallowJunctionHint": "This tool disables Windows junctions and falls back to symlink or copy.",
  "settings.backupsTitle": "Skill Backups",
  "settings.backupsSubtitle": "Rolling snapshots captured before uninstall, update overwrite, or copy conflict (max 25).",
  "settings.backupsEmpty": "No skill backup snapshots yet",
  "settings.backupsRestore": "Restore",
  "settings.backupsClearAll": "Clear All",
  "settings.backupsClearConfirmTitle": "Clear all skill backups?",
  "settings.backupsClearConfirmDesc": "This permanently deletes {{count}} backup snapshot(s) and frees {{size}}.",
  "settings.backupsClearConfirmBtn": "Clear All",
  "settings.backupsRestored": "Restored backup: {{name}}",
  "settings.backupsRestoreFailed": "Failed to restore backup",
  "settings.backupsCleared": "Cleared {{count}} backup(s), freed {{size}}",
  "settings.backupsClearFailed": "Failed to clear backups",
  "settings.backupReasonUninstall": "Uninstall",
  "settings.backupReasonUpdate": "Pre-update",
  "settings.backupReasonCopyConflict": "Copy Conflict",
  "settings.backupFromCalled": "from {{called}}",
  "settings.backupsOpenFolder": "Open Folder",
});

export function st(key: string): string {
  const current = locale();
  const other: Locale = current === "zh-CN" ? "en" : "zh-CN";
  return STRINGS[current][key] ?? STRINGS[other][key] ?? sharedT(key);
}

export function stf(key: string, params: Record<string, string | number>): string {
  let text = st(key);
  for (const [k, v] of Object.entries(params)) {
    text = text.replaceAll(`{{${k}}}`, String(v));
  }
  return text;
}
