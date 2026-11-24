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
    "mySkills.unmanagedEntriesTitle": "未受管同名目录冲突",
    "mySkills.unmanagedEntriesHint":
      "智能体目录中已存在外部写入的同名目录或链接。可先自动备份后由 SkillStar 接管，或将该目录内容吸收回 Hub。",
    "mySkills.takeOverUnmanaged": "备份并接管",
    "mySkills.takeOverForeignLink": "接管链接",
    "mySkills.adoptUnmanagedToHub": "吸收回 Hub",
    "mySkills.openConflictFolder": "打开目录",
    "mySkills.takeOverDoneWithBackup": "已备份原目录（{{id}}）并接管 {{agent}} 部署",
    "mySkills.takeOverDone": "已接管 {{agent}} 的同名链接",
    "mySkills.takeOverFailed": "接管未受管目录失败",
    "mySkills.adoptUnmanagedDone": "已将 {{agent}} 目录内容吸收回 Hub 并建立受管连接",
    "mySkills.adoptUnmanagedFailed": "吸收未受管目录失败",
    "mySkills.besideSkillsTitle": "同源仓库未安装技能",
    "mySkills.besideSkillsHint": "在同一来源仓库中发现以下相邻技能：",
    "mySkills.besideInstall": "安装",
    "githubSignIn.cancelAuth": "取消授权",
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
    "mySkills.unmanagedEntriesTitle": "Unmanaged Directory Conflicts",
    "mySkills.unmanagedEntriesHint":
      "An external folder or link already occupies this agent slot. Back it up and let SkillStar take over, or adopt its files into the Hub.",
    "mySkills.takeOverUnmanaged": "Backup & Take Over",
    "mySkills.takeOverForeignLink": "Take Over Link",
    "mySkills.adoptUnmanagedToHub": "Adopt into Hub",
    "mySkills.openConflictFolder": "Open Folder",
    "mySkills.takeOverDoneWithBackup": "Backed up folder ({{id}}) and linked {{agent}}",
    "mySkills.takeOverDone": "Replaced link and connected {{agent}}",
    "mySkills.takeOverFailed": "Failed to take over unmanaged entry",
    "mySkills.adoptUnmanagedDone": "Adopted {{agent}} folder into Hub and linked",
    "mySkills.adoptUnmanagedFailed": "Failed to adopt unmanaged folder",
    "mySkills.besideSkillsTitle": "Sibling Skills in Same Repo",
    "mySkills.besideSkillsHint": "Other uninstalled skills found in this source repository:",
    "mySkills.besideInstall": "Install",
    "githubSignIn.cancelAuth": "Cancel authorization",
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
