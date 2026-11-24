// 「GitHub 加速」桥接 DTO 与状态机（由 ghMirror.tsx re-export，供视图与测试共用）。

import { batch, signal, type Signal } from "nsh";
import { SettingsNetwork } from "skillstar-gui/native";
import { t, tf } from "../../../services/translation";

// ---- 桥（native/src/facades/settings_network.rs 的 TS 半边） ---------------

/** builtin_presets 的 wire（github_mirror::MirrorPreset serde 形状）。 */
export interface MirrorPresetDto {
  id: string;
  name: string;
  url: string;
  supports_clone: boolean;
}

/** github_mirror.json（GitHubMirrorConfig serde 形状；order=null 是缺文件
 * 的原始默认，UI 侧回落 preset id 序）。 */
export interface MirrorConfigDto {
  enabled: boolean;
  preset_id: string | null;
  custom_url: string | null;
  order: string[] | null;
}

/** gh_mirror.rs 里 CUSTOM_ENTRY_ID 常量的 TS 镜（域稳定值）。 */
export const CUSTOM_ENTRY_ID = "custom";

function asMirrorConfig(value: unknown): MirrorConfigDto {
  const raw = (value ?? {}) as Partial<MirrorConfigDto>;
  return {
    enabled: raw.enabled === true,
    preset_id: typeof raw.preset_id === "string" ? raw.preset_id : null,
    custom_url: typeof raw.custom_url === "string" ? raw.custom_url : null,
    order: Array.isArray(raw.order)
      ? raw.order.filter((id): id is string => typeof id === "string")
      : null,
  };
}

function asMirrorPresets(value: unknown): MirrorPresetDto[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((row) => {
    const raw = (row ?? {}) as Partial<MirrorPresetDto>;
    return typeof raw.id === "string" &&
      typeof raw.name === "string" &&
      typeof raw.url === "string"
      ? [
          {
            id: raw.id,
            name: raw.name,
            url: raw.url,
            supports_clone: raw.supports_clone === true,
          },
        ]
      : [];
  });
}

export interface GhMirrorBridge {
  /** github_mirror::load_config：缺文件 → 原始默认（order=null）。 */
  loadConfig(): MirrorConfigDto;
  /** github_mirror::builtin_presets（静态清单，工厂期读一次）。 */
  loadPresets(): MirrorPresetDto[];
  /** github_mirror::save_config：域归一化 order/preset_id。错误文本或 null。 */
  saveConfig(config: MirrorConfigDto): string | null;
  /** github_mirror::test_mirror：真网探活，resolve 毫秒延迟。 */
  testMirror(url: string): Promise<number>;
}

/** 真桥：native facade 直调（页面默认使用）。 */
export const realGhMirrorBridge: GhMirrorBridge = {
  loadConfig() {
    return asMirrorConfig(SettingsNetwork.loadMirrorConfig());
  },
  loadPresets() {
    return asMirrorPresets(SettingsNetwork.mirrorPresets());
  },
  saveConfig(config) {
    try {
      SettingsNetwork.saveMirrorConfig(config);
      return null;
    } catch (error) {
      return String(error);
    }
  },
  testMirror(url) {
    return SettingsNetwork.testMirror(url).then((ms) => Number(ms));
  },
};

// ---- 状态机（gh_mirror.rs 的 SettingsPage 字段 + save_gh_mirror 对位） ----

/** 一行镜像源（gh_mirror_form 的渲染行；selected 在 JSX 里现算）。 */
export interface GhMirrorEntry {
  id: string;
  name: string;
  url: string;
  custom: boolean;
}

export interface GhMirrorState {
  config: Signal<MirrorConfigDto>;
  /** custom 槽输入框的即时值（GPUI gh_mirror_custom_url InputState 对位）。 */
  customUrlForm: Signal<string>;
  /** gh_expanded。 */
  expanded: Signal<boolean>;
  /** gh_mirror_status：null 隐藏；ok 判定 = t("common.saved")。 */
  status: Signal<string | null>;
  /** gh_testing_id：当前探活行（单飞）。 */
  testingId: Signal<string | null>;
  /** gh_test_results：成功毫秒 / "fail"，按行 id 记忆。 */
  testResults: Signal<Record<string, number | "fail">>;
  /** 完整条目顺序；求值点必须在 JSX。 */
  order(): string[];
  /** 选中源 id（order[0]）；求值点必须在 JSX。 */
  selectedId(): string;
  /** 分区头 chip 的生效 URL（读内存 order，不读盘）。 */
  effectiveUrl(): string;
  /** 渲染行清单（gh_mirror_form 的循环体）；求值点必须在 JSX。 */
  entries(): GhMirrorEntry[];
  /** 挂载时读盘一次（构造函数 load 对位；不读 signal）。 */
  start(): void;
  /** save_gh_mirror：preset_id 从 order[0] 重推、custom_url 取表单 trim。 */
  save(): void;
  /** toggle：翻转 enabled 后保存。 */
  toggle(): void;
  /** 行点击：选中源挪到链首（retain + insert(0)）并保存。 */
  selectEntry(id: string): void;
  /** 排序箭头：与相邻位交换并保存（越界不动）。 */
  moveEntry(id: string, delta: -1 | 1): void;
  /** 折叠头：收起分支先保存再合（gh_mirror.rs closing 语义）。 */
  setExpanded(open: boolean): void;
  /** 表单编辑：只进表单 signal 并清状态行（InputEvent::Change 对位）。 */
  editCustomUrl(value: string): void;
  /** 探活：单飞 + 空地址不动，结果按行记忆（spawn_domain 对位）。 */
  runTest(id: string, url: string): void;
}

const DEFAULT_MIRROR_CONFIG: MirrorConfigDto = {
  enabled: false,
  preset_id: null,
  custom_url: null,
  order: null,
};

export function createGhMirrorState(
  bridge: GhMirrorBridge,
  opts?: { deferLoad?: boolean },
): GhMirrorState {
  // builtin presets 是静态清单：默认工厂期读一次；deferLoad 时延后到 start()。
  let presets = opts?.deferLoad ? [] : bridge.loadPresets();
  const config = signal<MirrorConfigDto>(
    opts?.deferLoad ? { ...DEFAULT_MIRROR_CONFIG } : bridge.loadConfig(),
  );
  const customUrlForm = signal(config().custom_url ?? "");
  const expanded = signal(false);
  const status = signal<string | null>(null);
  const testingId = signal<string | null>(null);
  const testResults = signal<Record<string, number | "fail">>({});

  /** gh_mirror_order 对位：order 为 null（缺文件原始默认）回落 preset id 序并附带 custom 槽。 */
  function order(): string[] {
    const stored = config().order;
    if (stored !== null && stored.length > 0) {
      return stored.includes(CUSTOM_ENTRY_ID) ? stored : [...stored, CUSTOM_ENTRY_ID];
    }
    return [...presets.map((preset) => preset.id), CUSTOM_ENTRY_ID];
  }

  const state: GhMirrorState = {
    config,
    customUrlForm,
    expanded,
    status,
    testingId,
    testResults,
    order,
    selectedId() {
      return order()[0] ?? "";
    },
    effectiveUrl() {
      const id = order()[0];
      if (id === undefined) return "";
      if (id === CUSTOM_ENTRY_ID) return config().custom_url ?? "";
      return presets.find((preset) => preset.id === id)?.url ?? "";
    },
    entries() {
      return order()
        .filter(
          (id) => id === CUSTOM_ENTRY_ID || presets.some((preset) => preset.id === id)
        )
        .map((id) => {
          const preset = presets.find((item) => item.id === id);
          return {
            id,
            custom: id === CUSTOM_ENTRY_ID,
            name:
              id === CUSTOM_ENTRY_ID
                ? t("settings.mirrorCustom")
                : (preset?.name ?? ""),
            url:
              id === CUSTOM_ENTRY_ID
                ? (config().custom_url ?? "")
                : (preset?.url ?? ""),
          };
        });
    },
    start() {
      // 读盘值先进局部变量再落 signal：effect 的追踪面必须为空。
      presets = bridge.loadPresets();
      const loaded = bridge.loadConfig();
      batch(() => {
        config.set(loaded);
        customUrlForm.set(loaded.custom_url ?? "");
      });
    },
    save() {
      const orderNow = order();
      const first = orderNow[0];
      const url = customUrlForm().trim();
      const next: MirrorConfigDto = {
        ...config(),
        preset_id: first !== undefined && first !== CUSTOM_ENTRY_ID ? first : null,
        custom_url: url.length > 0 ? url : null,
        order: orderNow,
      };
      config.set(next);
      const error = bridge.saveConfig(next);
      status.set(
        error === null ? t("common.saved") : tf("settings.connectionFailed", { error })
      );
    },
    toggle() {
      config.set({ ...config(), enabled: !config().enabled });
      state.save();
    },
    selectEntry(id) {
      const orderNow = order().filter((item) => item !== id);
      orderNow.unshift(id);
      config.set({ ...config(), order: orderNow });
      state.save();
    },
    moveEntry(id, delta) {
      const orderNow = order().slice();
      const i = orderNow.indexOf(id);
      const j = i + delta;
      if (i < 0 || j < 0 || j >= orderNow.length) return;
      const swapped = orderNow[j];
      orderNow[j] = orderNow[i];
      orderNow[i] = swapped;
      config.set({ ...config(), order: orderNow });
      state.save();
    },
    setExpanded(open) {
      if (!open) {
        // 收起分支先保存（gh_mirror.rs closing 语义：URL 输入随折叠提交）。
        state.save();
      }
      expanded.set(open);
    },
    editCustomUrl(value) {
      customUrlForm.set(value);
      status.set(null);
    },
    runTest(id, url) {
      if (testingId() !== null || url.length === 0) return;
      testingId.set(id);
      const settle = (result: number | "fail"): void => {
        testResults.set({ ...testResults(), [id]: result });
        testingId.set(null);
      };
      bridge.testMirror(url).then(
        (ms) => settle(ms),
        () => settle("fail")
      );
    },
  };
  return state;
}
