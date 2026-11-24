import { locale, type Locale } from "../../services/locale";

const STRINGS: Record<Locale, Record<string, string>> = {
  "zh-CN": {
    "publish.action": "发布到 GitHub",
    "publish.title": "发布本地技能到 GitHub",
    "publish.skillLabel": "本地技能：{{name}}",
    "publish.checking": "正在检测 Git 环境与 GitHub 登录状态…",
    "publish.gitMissingTitle": "需要先安装 Git",
    "publish.gitMissingBody":
      "发布流程会在本地初始化并推送 Git 仓库，需要系统命令行中可用 `git`。",
    "publish.gitDownload": "打开 Git 下载页面",
    "publish.authRequiredTitle": "需要 GitHub 授权",
    "publish.authSignedOutBody":
      "请先通过 SkillStar GitHub App 完成设备授权。发布需要仓库写入与创建权限（Contents: write / Administration: write）。",
    "publish.authExpiredBody":
      "当前 GitHub 登录会话已过期，请重新完成授权后继续发布。",
    "publish.permissionNotice":
      "仅使用 SkillStar GitHub App 身份创建或推送仓库，不改动本地 `.skill-lock.json` 来源记录。",
    "publish.signInBtn": "立即登录 GitHub",
    "publish.recheck": "重新检测",
    "publish.readyBadge": "已连接 @{{username}}",
    "publish.repoNameLabel": "目标仓库名",
    "publish.repoNamePlaceholder": "例如：my-skills",
    "publish.repoNameError": "仓库名须为 1–100 个字符，仅支持字母、数字、-、_、. 且不能以 . 开头",
    "publish.visibilityLabel": "仓库可见性",
    "publish.privateRepo": "私有仓库",
    "publish.publicRepo": "公开仓库",
    "publish.descriptionLabel": "仓库简介",
    "publish.descriptionPlaceholder": "简述该技能或仓库用途（可选）",
    "publish.submit": "发布",
    "publish.submitting": "正在创建并推送…",
    "publish.cancel": "取消",
    "publish.doneTitle": "已发布到 GitHub",
    "publish.doneFolder": "仓库内路径：{{folder}}",
    "publish.openRepo": "在 GitHub 中打开",
    "publish.copyUrl": "复制仓库链接",
    "publish.copied": "已复制",
    "publish.close": "完成",
    "publish.retry": "重试",
    "publish.preflightFailed": "预检失败：{{error}}",
  },
  en: {
    "publish.action": "Publish to GitHub",
    "publish.title": "Publish Local Skill to GitHub",
    "publish.skillLabel": "Local skill: {{name}}",
    "publish.checking": "Checking Git installation and GitHub session…",
    "publish.gitMissingTitle": "Git Required",
    "publish.gitMissingBody":
      "Publishing prepares a local Git commit and pushes it upstream, so `git` must be available on PATH.",
    "publish.gitDownload": "Download Git",
    "publish.authRequiredTitle": "GitHub Sign-In Required",
    "publish.authSignedOutBody":
      "Sign in with the SkillStar GitHub App first. Publishing needs repository creation and push permissions (Contents: write / Administration: write).",
    "publish.authExpiredBody":
      "Your SkillStar GitHub session has expired. Sign in again to publish.",
    "publish.permissionNotice":
      "Uses the SkillStar GitHub App identity to push into skills/{{name}} without altering `.skill-lock.json`.",
    "publish.signInBtn": "Sign in to GitHub",
    "publish.recheck": "Re-check",
    "publish.readyBadge": "Signed in as @{{username}}",
    "publish.repoNameLabel": "Repository name",
    "publish.repoNamePlaceholder": "e.g. my-skills",
    "publish.repoNameError":
      "Repository name must be 1–100 chars (letters, digits, '-', '_', '.') and cannot start with '.'",
    "publish.visibilityLabel": "Visibility",
    "publish.privateRepo": "Private",
    "publish.publicRepo": "Public",
    "publish.descriptionLabel": "Description",
    "publish.descriptionPlaceholder": "Short repository description (optional)",
    "publish.submit": "Publish",
    "publish.submitting": "Publishing…",
    "publish.cancel": "Cancel",
    "publish.doneTitle": "Published to GitHub",
    "publish.doneFolder": "Repository folder: {{folder}}",
    "publish.openRepo": "Open on GitHub",
    "publish.copyUrl": "Copy repository URL",
    "publish.copied": "Copied",
    "publish.close": "Done",
    "publish.retry": "Retry",
    "publish.preflightFailed": "Preflight failed: {{error}}",
  },
};

export function pt(key: string): string {
  const current = locale();
  const fallback: Locale = current === "zh-CN" ? "en" : "zh-CN";
  return STRINGS[current][key] ?? STRINGS[fallback][key] ?? key;
}

export function ptf(key: string, params: Record<string, string | number>): string {
  let text = pt(key);
  for (const [k, v] of Object.entries(params)) {
    text = text.replaceAll(`{{${k}}}`, String(v));
  }
  return text;
}
