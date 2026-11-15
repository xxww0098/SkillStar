import type { IconName } from "../../chrome/icons";

// 设置分区注册表 — 收敛为六大分类，顺序即次级导航（SettingsNav）与
// 窄窗口兜底堆叠顺序。standalone test workbenches 保留各自独立 section
// 元信息常量（SKILL_UPDATES_SECTION / GH_MIRROR_SECTION / NETWORK_DOCTOR_SECTION）。

export type SettingsSectionId =
  | "agents"
  | "general"
  | "network"
  | "translation"
  | "storage"
  | "about";

export interface SettingsSectionEntry {
  id: SettingsSectionId;
  /** 次级导航按钮 testID 与测试选择器。 */
  domId: string;
  /** label i18n key（settings.*）。 */
  labelKey: string;
  /** 分区导语副标题 i18n key（settings.sub.*）。 */
  subtitleKey: string;
  /** 导航与 SectionShell icon。 */
  icon: IconName;
}

export interface StandaloneSectionEntry {
  id: string;
  domId: string;
  labelKey: string;
  subtitleKey?: string;
  icon: IconName;
}

/** 设置六大主分区，顺序 = 左侧子菜单顺序。 */
export const SETTINGS_SECTIONS: SettingsSectionEntry[] = [
  {
    id: "agents",
    domId: "settings-agents",
    labelKey: "settings.agentConnections",
    subtitleKey: "settings.sub.agents",
    icon: "bot",
  },
  {
    id: "general",
    domId: "settings-general",
    labelKey: "settings.general",
    subtitleKey: "settings.sub.general",
    icon: "sliders",
  },
  {
    id: "network",
    domId: "settings-network",
    labelKey: "settings.network",
    subtitleKey: "settings.sub.network",
    icon: "globe",
  },
  {
    id: "translation",
    domId: "settings-translation",
    labelKey: "settings.translation",
    subtitleKey: "settings.sub.translation",
    icon: "languages",
  },
  {
    id: "storage",
    domId: "settings-storage",
    labelKey: "settings.storage",
    subtitleKey: "settings.sub.storage",
    icon: "hardDrive",
  },
  {
    id: "about",
    domId: "settings-about",
    labelKey: "settings.about",
    subtitleKey: "settings.sub.about",
    icon: "info",
  },
];

/** 供子组件独立工作台测试（*.test.tsx）使用的标题元信息。 */
export const SKILL_UPDATES_SECTION: StandaloneSectionEntry = {
  id: "updates",
  domId: "settings-skill-updates",
  labelKey: "settings.skillUpdates",
  icon: "refresh",
};

export const GH_MIRROR_SECTION: StandaloneSectionEntry = {
  id: "mirror",
  domId: "settings-mirror",
  labelKey: "settings.githubMirror",
  icon: "zap",
};

export const NETWORK_DOCTOR_SECTION: StandaloneSectionEntry = {
  id: "doctor",
  domId: "settings-network-doctor",
  labelKey: "settings.networkDoctor",
  icon: "activity",
};

export function sectionIndex(id: SettingsSectionId): number {
  return SETTINGS_SECTIONS.findIndex((section) => section.id === id);
}

// ---- 布局常量 ---------------------------------------------------------------

/** 滚动内容最大宽。 */
export const SETTINGS_BODY_W = 760;
/** 滚动列标准宽（保留导出供历史兼容与测试引用）。 */
export const SETTINGS_COL_W = 768;
/** 次级导航展开标签态宽。 */
export const SETTINGS_NAV_W = 188;
/** 次级导航收起图标态宽。 */
export const SETTINGS_NAV_COLLAPSED_W = 48;
/** 窄窗口退化为单列堆叠时分区槽底距。 */
export const SETTINGS_SECTION_PB = 28;
/** 滚动列标准内边距。 */
export const SETTINGS_SCROLL_PAD = 24;

/** 面板宽度响应式断点：>=720 展示次级导航，>=900 展示导航文字标签。 */
export const SETTINGS_NAV_MIN_PANE = 720;
export const SETTINGS_NAV_LABELS_MIN_PANE = 900;
