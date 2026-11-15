import { locale, type Locale } from "../../services/locale";

const STRINGS: Record<Locale, Record<string, string>> = {
  "zh-CN": {
    "mySkills.fromCalled": "原名 {{called}}",
    "mySkills.modifiedCopiesTitle": "智能体副本本地修改",
    "mySkills.modifiedCopiesHint":
      "检测到副本目录有本地修改：可将修改吸收回 Hub（并重新部署），或先备份再以 Hub 覆盖。",
    "mySkills.adoptCopyEdits": "吸收回 Hub",
    "mySkills.overwriteCopyWithBackup": "备份并覆盖",
    "mySkills.adoptCopyDone": "已将 {{agent}} 的修改吸收回 Hub",
    "mySkills.adoptCopyFailed": "吸收副本修改失败",
    "mySkills.overwriteCopyDone": "已生成备份（{{id}}）并还原 {{agent}}",
    "mySkills.overwriteCopyFailed": "备份并覆盖失败",
    "mySkills.besideSkillsTitle": "同源仓库未安装技能",
    "mySkills.besideSkillsHint": "在同一来源仓库中发现以下相邻技能：",
    "mySkills.besideInstall": "安装",
  },
  en: {
    "mySkills.fromCalled": "from {{called}}",
    "mySkills.modifiedCopiesTitle": "Modified Agent Copies",
    "mySkills.modifiedCopiesHint":
      "Detected edits inside copy-mode agent directories. Adopt edits into the Hub or overwrite after saving a backup.",
    "mySkills.adoptCopyEdits": "Adopt Edits",
    "mySkills.overwriteCopyWithBackup": "Backup & Overwrite",
    "mySkills.adoptCopyDone": "Adopted edits from {{agent}} into Hub",
    "mySkills.adoptCopyFailed": "Failed to adopt agent edits",
    "mySkills.overwriteCopyDone": "Saved backup ({{id}}) and reset {{agent}}",
    "mySkills.overwriteCopyFailed": "Failed to backup and overwrite",
    "mySkills.besideSkillsTitle": "Sibling Skills in Same Repo",
    "mySkills.besideSkillsHint": "Other uninstalled skills found in this source repository:",
    "mySkills.besideInstall": "Install",
  },
};

export function et(key: string): string {
  const current = locale();
  const other: Locale = current === "zh-CN" ? "en" : "zh-CN";
  return STRINGS[current][key] ?? STRINGS[other][key] ?? key;
}

export function etf(key: string, params: Record<string, string | number>): string {
  let text = et(key);
  for (const [k, v] of Object.entries(params)) {
    text = text.replaceAll(`{{${k}}}`, String(v));
  }
  return text;
}
