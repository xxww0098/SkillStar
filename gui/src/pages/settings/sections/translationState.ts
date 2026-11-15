// 「翻译」状态机、桥接与 DTO — 从 ./translation.tsx 拆出（保持单文件 < 800 行）。

import { signal, type Signal } from "nsh";
import { SettingsUpdates } from "skillstar-gui/native";
import { t, tf } from "../../../services/translation";
import { translationPrefsChanged } from "../../../shell/events";

// ---- 桥类型（facades/settings_updates.rs 的 TS 半边） ----------------------

/** translation.json 全量（TranslationConfig 的 serde 形状）。 */
export interface TranslationConfigDto {
  engine: "machine" | "llm";
  translate_descriptions: boolean;
  translate_skill_md: boolean;
  description_choices: Record<string, boolean>;
  reader_theme: string;
  target_lang: string;
  llm_base_url: string;
  llm_model: string;
  llm_model_pinned: boolean;
  llm_account_id: string;
  llm_account_catalog: string;
}

/** TRANSLATION_LANGUAGES 行（code/endonym label）。 */
export interface TranslationLanguageRow {
  code: string;
  label: string;
}

/** 模型拉取的 wire 信封（pull_llm_models）。 */
export type ModelPullReport =
  | { ok: true; models: string[] }
  | {
      ok: false;
      error: "missing_base" | "missing_key" | "empty" | "status" | "request";
      status?: number;
    };

export interface TranslationBridge {
  /** translation::load_config：缺文件 → 默认（machine / 双开关关 / zh-CN）。 */
  loadTranslation(): TranslationConfigDto;
  /** save_translation 语义：盘上的 description_choices/reader_theme 回填，
   * 返回重载后的归一配置；error 非空表示写盘失败（配置仍是重载值）。 */
  saveTranslation(config: TranslationConfigDto): { config: TranslationConfigDto; error: string | null };
  translationLanguages(): TranslationLanguageRow[];
  /** translation::load_api_key：手填密钥（独立 0600 文件）。纯、同步。 */
  loadLlmKey(): string;
  /** translation::save_api_key：写盘（原子 + 0600）。失败 reject。 */
  saveLlmKey(key: string): void;
  /** pull_llm_models：唯一网络调用。 */
  pullModels(): Promise<ModelPullReport>;
}

// ---- 收窄助手（bridge.ts as* 同款纪律） -----------------------------------

function asConfig(value: unknown): TranslationConfigDto {
  const raw = (value ?? {}) as Partial<TranslationConfigDto>;
  const choices: Record<string, boolean> = {};
  if (raw.description_choices !== null && typeof raw.description_choices === "object") {
    for (const [name, choice] of Object.entries(raw.description_choices)) {
      choices[name] = choice === true;
    }
  }
  return {
    engine: raw.engine === "llm" ? "llm" : "machine",
    translate_descriptions: raw.translate_descriptions === true,
    translate_skill_md: raw.translate_skill_md === true,
    description_choices: choices,
    reader_theme: typeof raw.reader_theme === "string" ? raw.reader_theme : "quote",
    target_lang: typeof raw.target_lang === "string" ? raw.target_lang : "zh-CN",
    llm_base_url: typeof raw.llm_base_url === "string" ? raw.llm_base_url : "",
    llm_model: typeof raw.llm_model === "string" ? raw.llm_model : "",
    llm_model_pinned: raw.llm_model_pinned === true,
    llm_account_id: typeof raw.llm_account_id === "string" ? raw.llm_account_id : "",
    llm_account_catalog: typeof raw.llm_account_catalog === "string" ? raw.llm_account_catalog : "",
  };
}

function asLanguages(value: unknown): TranslationLanguageRow[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((row) => {
    const raw = (row ?? {}) as Partial<TranslationLanguageRow>;
    return typeof raw.code === "string" && typeof raw.label === "string"
      ? [{ code: raw.code, label: raw.label }]
      : [];
  });
}

/** 真桥：SettingsUpdates facade 直调（页面默认使用）。 */
export const realTranslationBridge: TranslationBridge = {
  loadTranslation() {
    return asConfig(SettingsUpdates.loadTranslation());
  },
  saveTranslation(config) {
    try {
      return { config: asConfig(SettingsUpdates.saveTranslation(config)), error: null };
    } catch (error) {
      // GPUI save_translation：写盘失败 warn 后仍从盘重载（页面继续可用）。
      console.warn(`failed to save translation settings: ${error}`);
      return { config: asConfig(SettingsUpdates.loadTranslation()), error: String(error) };
    }
  },
  translationLanguages() {
    return asLanguages(SettingsUpdates.translationLanguages());
  },
  loadLlmKey() {
    return SettingsUpdates.loadLlmKey();
  },
  saveLlmKey(key) {
    SettingsUpdates.saveLlmKey(key);
  },
  pullModels() {
    return SettingsUpdates.pullLlmModels() as Promise<ModelPullReport>;
  },
};

// ---- 目标语言与拉取文案 ----------------------------------------------------

/** 目标语言选中档：精确码优先，旧方言码 starts_with 落到基础语言。 */
export function targetSelectedIndex(
  target: string,
  languages: TranslationLanguageRow[],
): number {
  const ix = languages.findIndex(
    (language) => target === language.code || target.startsWith(language.code),
  );
  return ix < 0 ? 0 : ix;
}

/** model_list_message 对位：错误类 → i18n 文案；no_account 静默（页面侧已早退）。 */
export function pullErrorMessage(report: { ok: false; error: string; status?: number }): string | null {
  switch (report.error) {
    case "missing_base":
      return t("settings.tranLlmBaseUrlHint");
    case "missing_key":
      return t("settings.tranLlmModelNoKey");
    case "empty":
      return t("settings.tranLlmModelNone");
    case "status":
      return tf("settings.tranLlmModelStatus", { status: report.status ?? 0 });
    case "request":
      return t("settings.tranLlmModelFailed");
    default:
      return null;
  }
}

// ---- 状态机 ----------------------------------------------------------------

export interface TranslationState {
  config: Signal<TranslationConfigDto>;
  languages: Signal<TranslationLanguageRow[]>;
  /** 密钥输入框的草稿（提交前不回写盘，页面也不回显真密钥）。 */
  llmKeyDraft: Signal<string>;
  /** 盘上是否已有密钥（只给「已保存」标记，不给内容）。 */
  llmKeySaved: Signal<boolean>;
  /** 上次成功拉取的模型目录。 */
  models: Signal<string[]>;
  /** models 归属的接口地址（llm_models_for）。 */
  modelsFor: Signal<string>;
  modelsLoading: Signal<boolean>;
  modelsError: Signal<string | null>;
  /** 拉取代际（GPUI llm_fetch_seq）：同址同清单的重拉也要推进。 */
  fetchSeq: Signal<number>;
  modelMenuOpen: Signal<boolean>;
  modelQuery: Signal<string>;
  start(): void;
  toggleDescriptions(): void;
  toggleSkillMd(): void;
  setTargetLang(code: string): void;
  setEngine(engine: "machine" | "llm"): void;
  setLlmBaseUrl(url: string): void;
  /** 提交密钥（回车/失焦/点保存）：空串即清除盘上的密钥。 */
  commitLlmKey(): void;
  setLlmKeyDraft(text: string): void;
  selectModel(model: string): void;
  pullModels(): void;
  setModelMenuOpen(open: boolean): void;
  setModelQuery(query: string): void;
}

export function createTranslationState(bridge: TranslationBridge): TranslationState {
  const config = signal<TranslationConfigDto>(defaultConfig());
  const languages = signal<TranslationLanguageRow[]>([]);
  const llmKeyDraft = signal("");
  const llmKeySaved = signal(false);
  const models = signal<string[]>([]);
  const modelsFor = signal("");
  const modelsLoading = signal(false);
  const modelsError = signal<string | null>(null);
  const fetchSeq = signal(0);
  const modelMenuOpen = signal(false);
  const modelQuery = signal("");

  function defaultConfig(): TranslationConfigDto {
    return {
      engine: "machine",
      translate_descriptions: false,
      translate_skill_md: false,
      description_choices: {},
      reader_theme: "quote",
      target_lang: "zh-CN",
      llm_base_url: "",
      llm_model: "",
      llm_model_pinned: false,
      llm_account_id: "",
      llm_account_catalog: "",
    };
  }

  /** save_translation 对位：写盘（facade 回填盘上的选择/样式）→ 重载 → 广播。 */
  function saveFrom(next: TranslationConfigDto, publish: boolean): void {
    const result = bridge.saveTranslation(next);
    config.set(result.config);
    if (result.error !== null) {
      console.warn(`failed to save translation settings: ${result.error}`);
    }
    if (publish) {
      translationPrefsChanged.publish();
    }
  }

  return {
    config,
    languages,
    llmKeyDraft,
    llmKeySaved,
    models,
    modelsFor,
    modelsLoading,
    modelsError,
    fetchSeq,
    modelMenuOpen,
    modelQuery,
    start() {
      // 读盘值先进局部变量再落 signal（effect 追踪面纪律同基座 start）。
      const languagesLoaded = bridge.translationLanguages();
      const loaded = bridge.loadTranslation();
      const hasKey = bridge.loadLlmKey().trim().length > 0;
      languages.set(languagesLoaded);
      llmKeySaved.set(hasKey);
      llmKeyDraft.set("");
      config.set(loaded);
    },
    toggleDescriptions() {
      saveFrom({ ...config(), translate_descriptions: !config().translate_descriptions }, true);
    },
    toggleSkillMd() {
      saveFrom({ ...config(), translate_skill_md: !config().translate_skill_md }, true);
    },
    setTargetLang(code) {
      saveFrom({ ...config(), target_lang: code }, true);
    },
    setEngine(engine) {
      saveFrom({ ...config(), engine }, true);
    },
    setLlmBaseUrl(url) {
      const current = config();
      const trimmed = url.trim();
      if (current.llm_base_url === trimmed) return;
      // 换址作废已拉取的清单（清单归属地址）。
      models.set([]);
      modelsFor.set("");
      modelsError.set(null);
      modelsLoading.set(false);
      saveFrom({ ...current, llm_base_url: trimmed }, true);
    },
    setLlmKeyDraft(text) {
      llmKeyDraft.set(text);
    },
    commitLlmKey() {
      const key = llmKeyDraft().trim();
      try {
        // 空串即清除：save_api_key 写成空文件，load_api_key 读回空。
        bridge.saveLlmKey(key);
        llmKeySaved.set(key.length > 0);
        llmKeyDraft.set("");
      } catch (error) {
        console.warn(`failed to save the translation key: ${error}`);
      }
    },
    selectModel(model) {
      const current = config();
      const trimmed = model.trim();
      if (
        trimmed.length === 0 ||
        (current.llm_model_pinned && current.llm_model === trimmed)
      ) {
        return;
      }
      saveFrom({ ...current, llm_model: trimmed, llm_model_pinned: true }, true);
    },
    pullModels() {
      if (modelsLoading()) return;
      const current = config();
      const base = current.llm_base_url.trim();
      if (base.length === 0) {
        // 没填地址：按钮静默（提示文案就摆在地址行下面）。
        return;
      }
      modelsLoading.set(true);
      modelsError.set(null);
      bridge
        .pullModels()
        .then((report) => {
          // 迟到的结果按「发起时的地址」丢弃。
          if (config().llm_base_url.trim() !== base) return;
          modelsLoading.set(false);
          if (report.ok) {
            models.set(report.models);
            modelsFor.set(base);
            modelsError.set(null);
            fetchSeq.set(fetchSeq() + 1);
          } else {
            const message = pullErrorMessage(report);
            if (message !== null) {
              modelsError.set(message);
            }
          }
        })
        .catch(() => {
          if (config().llm_base_url.trim() !== base) return;
          modelsLoading.set(false);
          modelsError.set(t("settings.tranLlmModelFailed"));
        });
    },
    setModelMenuOpen(open) {
      modelMenuOpen.set(open);
      if (!open) {
        modelQuery.set("");
      }
    },
    setModelQuery(query) {
      modelQuery.set(query);
    },
  };
}
