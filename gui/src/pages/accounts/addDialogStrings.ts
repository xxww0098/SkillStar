// 账号添加弹窗私有双语文案（Kiro Portal/IdC、ZCode Z.ai/BigModel 等能力选项），
// 不修改共享 services/translation.ts，保持 zh-CN 与 en 严格对齐。

import { locale, type Locale } from "../../services/locale";

const STRINGS: Record<Locale, Record<string, string>> = {
  "zh-CN": {
    "addDialog.oauthModeLabel": "登录方式",
    "addDialog.idcRegionLabel": "AWS Region",
    "addDialog.idcRegionPlaceholder": "us-east-1",
    "addDialog.idcStartUrlLabel": "Start URL（可选）",
    "addDialog.idcStartUrlPlaceholder": "留空使用 AWS Builder ID 默认地址",
    "addDialog.idcStartUrlHint": "企业 IAM Identity Center 填写 https://*.awsapps.com/start",
  },
  en: {
    "addDialog.oauthModeLabel": "Login Method",
    "addDialog.idcRegionLabel": "AWS Region",
    "addDialog.idcRegionPlaceholder": "us-east-1",
    "addDialog.idcStartUrlLabel": "Start URL (Optional)",
    "addDialog.idcStartUrlPlaceholder": "Leave blank for AWS Builder ID default",
    "addDialog.idcStartUrlHint": "For enterprise IAM Identity Center, enter https://*.awsapps.com/start",
  },
};

export function dt(key: string): string {
  const cur = locale();
  const alt: Locale = cur === "zh-CN" ? "en" : "zh-CN";
  return STRINGS[cur][key] ?? STRINGS[alt][key] ?? key;
}
