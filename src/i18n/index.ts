import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./locales/en.json";
import zhCN from "./locales/zh-CN.json";

const savedLang = localStorage.getItem("skillstar:lang") || "zh-CN";

i18n.use(initReactI18next).init({
  resources: {
    en: { translation: en },
    "zh-CN": { translation: zhCN },
  },
  lng: savedLang,
  fallbackLng: "zh-CN",
  interpolation: {
    escapeValue: false,
  },
});

export function setLanguage(lang: string) {
  i18n.changeLanguage(lang);
  localStorage.setItem("skillstar:lang", lang);
  document.documentElement.lang = lang;
}

export function getLanguage(): string {
  return i18n.language || "zh-CN";
}

export const supportedLanguages = [
  { code: "zh-CN", label: "简体中文", flag: "🇨🇳" },
  { code: "en", label: "English", flag: "🇺🇸" },
] as const;

export default i18n;
