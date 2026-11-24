import { locale, type Locale } from "../../services/locale";

// SKILL.md 阅读器文案目录 — 值逐条取自 crates/ss-gpui/assets/locales/
// {zh-CN,en}.json 的 detailPanel.*/settings.tran* 段（marketStrings.ts 同款
// 约定：切片自持键表，不编辑共享 services/translation.ts；zh/en 同步）。

const STRINGS: Record<Locale, Record<string, string>> = {
  "zh-CN": {
    "common.retry": "重试",
    "mySkills.handleProjectCopies": "处理项目副本",
    "detailPanel.emptySkillMd": "这份 SKILL.md 是空的。",
    "detailPanel.readFailed": "读不到 SKILL.md。",
    "detailPanel.translate": "翻译",
    "detailPanel.showOriginal": "原文",
    "detailPanel.translationNeedsConfig":
      "请先在「设置 → 翻译」填写接口地址、API Key 并选择模型。",
    "detailPanel.translationRejected": "「{{model}}」拒绝了翻译请求。换一个模型后再试。",
    "detailPanel.translationRejectedDetail":
      "「{{model}}」无法翻译：{{detail}}。换一个模型后再试。",
    "settings.tranReaderTheme": "译文样式",
    "settings.tranStyleSample": "你好，世界",
    "settings.tranThemeNone": "无",
    "settings.tranThemeLine": "下划直线",
    "settings.tranThemeDot": "下划点状线",
    "settings.tranThemeDash": "下划虚线",
    "settings.tranThemeDbold": "下划虚线加粗",
    "settings.tranThemeWavy": "下划波浪线",
    "settings.tranThemeWbold": "下划波浪线加粗",
    "settings.tranThemeBox": "虚线框",
    "settings.tranThemeXbold": "虚线框加粗",
    "settings.tranThemeMark": "马克笔",
    "settings.tranThemeGmark": "渐变马克笔",
    "settings.tranThemeFuzzy": "模糊",
    "settings.tranThemeHi": "高亮",
    "settings.tranThemeQuote": "引用",
    "settings.tranThemeGrad": "渐变",
    "settings.tranThemeBlink": "闪现",
    "settings.tranThemeGlow": "发光",
    "settings.tranThemeColor": "多彩",
  },
  en: {
    "common.retry": "Retry",
    "mySkills.handleProjectCopies": "Manage project copies",
    "detailPanel.emptySkillMd": "This SKILL.md is empty.",
    "detailPanel.readFailed": "Couldn't read SKILL.md.",
    "detailPanel.translate": "Translate",
    "detailPanel.showOriginal": "Original",
    "detailPanel.translationNeedsConfig":
      "Configure the base URL, API key, and model in Settings → Translation first.",
    "detailPanel.translationRejected": '"{{model}}" rejected the translation. Pick another model and try again.',
    "detailPanel.translationRejectedDetail":
      '"{{model}}" can\'t translate this: {{detail}}. Pick another model and try again.',
    "settings.tranReaderTheme": "Translation style",
    "settings.tranStyleSample": "Hello, world",
    "settings.tranThemeNone": "None",
    "settings.tranThemeLine": "Underline",
    "settings.tranThemeDot": "Dotted Underline",
    "settings.tranThemeDash": "Dashed Underline",
    "settings.tranThemeDbold": "Dashed Underline Bold",
    "settings.tranThemeWavy": "Wavy Underline",
    "settings.tranThemeWbold": "Wavy Underline Bold",
    "settings.tranThemeBox": "Dashed Box",
    "settings.tranThemeXbold": "Dashed Box Bold",
    "settings.tranThemeMark": "Marker",
    "settings.tranThemeGmark": "Gradient Marker",
    "settings.tranThemeFuzzy": "Fuzzy",
    "settings.tranThemeHi": "Highlight",
    "settings.tranThemeQuote": "Quote",
    "settings.tranThemeGrad": "Gradient",
    "settings.tranThemeBlink": "Blink",
    "settings.tranThemeGlow": "Glow",
    "settings.tranThemeColor": "Colorful",
  },
};

/** 点键查表（i18n.rs t() 的目录内对位：当前目录 → 另一目录 → 键本身）。 */
export function rt(key: string): string {
  const lang = locale();
  const hit = STRINGS[lang][key] ?? STRINGS[lang === "zh-CN" ? "en" : "zh-CN"][key];
  return typeof hit === "string" ? hit : key;
}

/** `{{name}}` 插值（i18n.rs tf() 对位）。 */
export function rtf(key: string, vars: Record<string, string>): string {
  let text = rt(key);
  for (const [name, value] of Object.entries(vars)) {
    text = text.split(`{{${name}}}`).join(value);
  }
  return text;
}

/** rejection_message（translation.rs:363-378 对位）。 */
export function readerRejectionMessage(model: string, detail: string): string {
  const trimmedModel = model.trim();
  const trimmedDetail = detail.trim().replace(/[.。]+$/, "");
  if (trimmedDetail.length === 0) {
    return rtf("detailPanel.translationRejected", { model: trimmedModel });
  }
  return rtf("detailPanel.translationRejectedDetail", {
    model: trimmedModel,
    detail: trimmedDetail,
  });
}
