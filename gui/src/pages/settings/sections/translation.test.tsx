// 「翻译」分区测试 — settings/translation.rs 的语义对位：面开关/语言/引擎
// 即点即写盘并广播 translationPrefsChanged；LLM 路径改成手填接口地址 +
// 密钥（账号域已从翻译拆开）；模型拉取的门槛（地址/密钥）、迟到丢弃与错误
// 类 i18n 分派（model_list_message）；combobox 清单的派生
// （sync_llm_model_state：目录 ∪ 当前模型）。不写真实 $HOME。
import { expect, open, test } from "nsh/test";
import { t, tf } from "../../../services/translation";
import { translationPrefsChanged } from "../../../shell/events";
import {
  createTranslationState,
  modelRows,
  pullErrorMessage,
  targetSelectedIndex,
  type ModelPullReport,
  type TranslationBridge,
  type TranslationConfigDto,
} from "./translation";

const HOST = "./translation.test.host.tsx";

// ---- 共用假桥素材 -----------------------------------------------------------

function baseConfig(overrides: Partial<TranslationConfigDto> = {}): TranslationConfigDto {
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
    ...overrides,
  };
}

function fakeBridge(options?: { saveError?: string }): {
  bridge: TranslationBridge;
  saves: TranslationConfigDto[];
  resolvePull: (report: ModelPullReport) => void;
  keyFile: () => string;
} {
  const saves: TranslationConfigDto[] = [];
  let disk = baseConfig({ description_choices: { "kept-skill": true }, reader_theme: "wavy" });
  let resolvePull: ((report: ModelPullReport) => void) | null = null;
  let key = "";
  const bridge: TranslationBridge = {
    loadTranslation: () => ({ ...disk, description_choices: { ...disk.description_choices } }),
    saveTranslation(next) {
      // facade 语义：盘上的选择与阅读器样式不被写丢；写盘失败时盘不动，
      // 返回值是盘上的重载配置。
      if (options?.saveError === undefined) {
        disk = { ...next, description_choices: { ...disk.description_choices }, reader_theme: disk.reader_theme };
      }
      saves.push({ ...next });
      return {
        config: { ...disk, description_choices: { ...disk.description_choices } },
        error: options?.saveError ?? null,
      };
    },
    translationLanguages: () => [],
    loadLlmKey: () => key,
    saveLlmKey: (next) => {
      key = next.trim();
    },
    pullModels: () =>
      new Promise<ModelPullReport>((resolve) => {
        resolvePull = resolve;
      }),
  };
  return { bridge, saves, resolvePull: (report) => resolvePull?.(report), keyFile: () => key };
}

/** 微任务冲刷：让 pullModels 的 .then 链走完。 */
async function flush(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

// ---- 纯策略（translation.rs 的 UI 半边逐分支对位） --------------------------

test("the target slider falls back to the base language slot", () => {
  const languages = [
    { code: "zh-CN", label: "简体中文" },
    { code: "zh-TW", label: "繁體中文" },
    { code: "en", label: "English" },
    { code: "ja", label: "日本語" },
    { code: "ko", label: "한국어" },
  ];
  expect(targetSelectedIndex("ja", languages)).toBe(3);
  expect(targetSelectedIndex("zh-TW", languages)).toBe(1);
  expect(targetSelectedIndex("klingon", languages)).toBe(0);
});

test("model_list_message maps every pull failure kind", () => {
  expect(pullErrorMessage({ ok: false, error: "missing_base" })).toBe(
    t("settings.tranLlmBaseUrlHint"),
  );
  expect(pullErrorMessage({ ok: false, error: "missing_key" })).toBe(
    t("settings.tranLlmModelNoKey"),
  );
  expect(pullErrorMessage({ ok: false, error: "empty" })).toBe(t("settings.tranLlmModelNone"));
  expect(pullErrorMessage({ ok: false, error: "status", status: 429 })).toBe(
    tf("settings.tranLlmModelStatus", { status: 429 }),
  );
  expect(pullErrorMessage({ ok: false, error: "request" })).toBe(
    t("settings.tranLlmModelFailed"),
  );
});

// ---- 状态机（假桥；save_translation / 手填密钥 / pull 的读写语义） -----------

test("the fallback config takes the domain defaults (both surfaces on)", () => {
  const state = createTranslationState(fakeBridge().bridge);
  // ss_core::translation::TranslationConfig::default：机翻、双开关开、简体中文。
  expect(state.config().engine).toBe("machine");
  expect(state.config().translate_descriptions).toBe(true);
  expect(state.config().translate_skill_md).toBe(true);
  expect(state.config().target_lang).toBe("zh-CN");
});

test("surface switches and language saves publish translationPrefsChanged", () => {
  const { bridge } = fakeBridge();
  const state = createTranslationState(bridge);
  let events = 0;
  const unsubscribe = translationPrefsChanged.subscribe(() => {
    events += 1;
  });
  state.start();
  state.toggleDescriptions();
  state.toggleSkillMd();
  state.setTargetLang("ja");
  expect(events).toBe(3);
  expect(state.config().translate_descriptions).toBe(true);
  expect(state.config().translate_skill_md).toBe(true);
  expect(state.config().target_lang).toBe("ja");
  // 保存回读带上盘上的选择（facade 的 description_choices 回填语义）。
  expect(state.config().description_choices["kept-skill"]).toBe(true);
  expect(state.config().reader_theme).toBe("wavy");
  unsubscribe();
});

test("the typed key is written on commit and only its presence is echoed", () => {
  const { bridge, keyFile } = fakeBridge();
  const state = createTranslationState(bridge);
  let events = 0;
  const unsubscribe = translationPrefsChanged.subscribe(() => {
    events += 1;
  });
  state.start();
  expect(state.llmKeySaved()).toBe(false);
  expect(state.llmKeyDraft()).toBe("");
  state.setLlmKeyDraft("  sk-typed-9  ");
  expect(keyFile()).toBe("");
  state.commitLlmKey();
  expect(keyFile()).toBe("sk-typed-9");
  expect(state.llmKeySaved()).toBe(true);
  expect(events).toBe(1);
  // 提交后草稿清空：页面不回显密钥本身。
  expect(state.llmKeyDraft()).toBe("");
  // 空串提交禁止覆盖已存密钥，也不发布 prefs 变更；必须走显式 clearLlmKey() 才清除。
  state.setLlmKeyDraft("   ");
  state.commitLlmKey();
  expect(keyFile()).toBe("sk-typed-9");
  expect(state.llmKeySaved()).toBe(true);
  expect(state.llmKeyDraft()).toBe("");
  expect(events).toBe(1);

  state.clearLlmKey();
  expect(keyFile()).toBe("");
  expect(state.llmKeySaved()).toBe(false);
  expect(events).toBe(2);
  unsubscribe();
});

test("selectModel ignores empty picks and pinned duplicates", () => {
  const { bridge, saves } = fakeBridge();
  const state = createTranslationState(bridge);
  state.start();
  state.setEngine("llm");
  const savesAfterEngine = saves.length;

  state.selectModel("   ");
  state.selectModel("deepseek-v4.1-flash"); // 未钉住：允许（补钉）。
  expect(saves).toHaveLength(savesAfterEngine + 1);
  state.selectModel("deepseek-v4.1-flash"); // 已钉住且相同：忽略。
  expect(saves).toHaveLength(savesAfterEngine + 1);
  expect(state.config().llm_model_pinned).toBe(true);
});

test("a failed save falls back to the reloaded disk config", () => {
  const { bridge } = fakeBridge({ saveError: "disk detached" });
  const state = createTranslationState(bridge);
  state.start();
  state.setTargetLang("ko");
  // 写盘失败：配置回到盘上（重载语义），页面不抛。
  expect(state.config().target_lang).toBe("zh-CN");
});

test("pulls gate on the typed endpoint and drop stale results", async () => {
  const { bridge, resolvePull } = fakeBridge();
  const state = createTranslationState(bridge);
  state.start();

  // 没填地址：展示校验错误，不发起拉取。
  state.pullModels();
  expect(state.modelsLoading()).toBe(false);
  expect(state.modelsError()).toBe(t("settings.tranLlmBaseUrlHint"));

  state.setLlmBaseUrl("https://api.test/v1");
  expect(state.config().llm_base_url).toBe("https://api.test/v1");
  state.pullModels();
  expect(state.modelsLoading()).toBe(true);
  resolvePull({ ok: true, models: ["deepseek-v4.1-flash", "kimi-k2.6"] });
  await flush();
  expect(state.modelsLoading()).toBe(false);
  expect(state.models()).toEqual(["deepseek-v4.1-flash", "kimi-k2.6"]);
  expect(state.modelsFor()).toBe("https://api.test/v1");
  expect(state.fetchSeq()).toBe(1);

  // 迟到的结果按「发起时的地址」丢弃：拉取中换址，结果不再落地。
  state.pullModels();
  state.setLlmBaseUrl("https://api.other/v1");
  resolvePull({ ok: true, models: ["gemma4:31b"] });
  await flush();
  expect(state.models()).toEqual([]);
  expect(state.modelsFor()).toBe("");
  expect(state.fetchSeq()).toBe(1);

  // 错误类对位 model_list_message。
  state.pullModels();
  resolvePull({ ok: false, error: "missing_key" });
  await flush();
  expect(state.modelsError()).toBe(t("settings.tranLlmModelNoKey"));

  state.pullModels();
  resolvePull({ ok: false, error: "status", status: 429 });
  await flush();
  expect(state.modelsError()).toBe(tf("settings.tranLlmModelStatus", { status: 429 }));
});

// ---- combobox 派生（sync_llm_model_state：目录 ∪ 当前模型） ------------------

test("model rows prepend the configured model when the catalog misses it", () => {
  const { bridge } = fakeBridge();
  const state = createTranslationState(bridge);
  // 未拉取：只有当前模型一行（触发器永远显示已配置值）。
  state.config.set(baseConfig({ llm_model: "custom-id" }));
  expect(modelRows(state)).toEqual(["custom-id"]);

  // 目录归属当前地址：目录行 + 缺失的当前模型补首行。
  state.models.set(["alpha", "beta"]);
  state.modelsFor.set("https://api.test/v1");
  state.config.set(baseConfig({ llm_model: "custom-id", llm_base_url: "https://api.test/v1" }));
  expect(modelRows(state)).toEqual(["custom-id", "alpha", "beta"]);

  // 当前模型已在目录：不重复。
  state.config.set(baseConfig({ llm_model: "alpha", llm_base_url: "https://api.test/v1" }));
  expect(modelRows(state)).toEqual(["alpha", "beta"]);

  // 目录归属别的地址：视同未拉取。
  state.modelsFor.set("https://api.other/v1");
  state.config.set(baseConfig({ llm_model: "alpha", llm_base_url: "https://api.test/v1" }));
  expect(modelRows(state)).toEqual(["alpha"]);
});

// ---- host UI（假桥装置；渲染期同步与 JSX 绑定） ------------------------------

async function openSection() {
  return open(HOST);
}

test("the engine slider reveals the typed-key block instead of accounts", async () => {
  const app = await openSection();
  await app.waitUntil(() => app.shows("id:tran-engine-machine"), { timeout: 2000 });
  // machine 默认：LLM 区不挂。
  expect(app.shows("id:tran-llm-base-url")).toBe(false);

  await app.press("id:tran-engine-llm");
  await app.waitUntil(() => app.shows("id:tran-llm-base-url"));
  expect(app.shows("id:tran-llm-key")).toBe(true);
  expect(app.shows("id:tran-llm-model-button")).toBe(true);
  // 没有账号胶囊了（账号域已从翻译拆开）。
  expect(app.shows("id:tran-credential-sub-oc")).toBe(false);

  // 面开关与语言行即点即写。
  await app.press("id:tran-descriptions");
  await app.waitUntil(() => app.texts().some((s) => s.includes("d=true")));
  await app.press("id:ja");
  await app.waitUntil(() => app.texts().some((s) => s.includes("tran:llm|ja|")));
});

test("the model combobox syncs in render: fetch fills it and picks pin", async () => {
  const app = await openSection();
  await app.waitUntil(() => app.shows("id:tran-engine-machine"), { timeout: 2000 });
  await app.press("id:tran-engine-llm");
  await app.waitUntil(() => app.shows("id:tran-llm-model-button"));

  // 未拉取：菜单只有当前模型一行；选中它即钉住。
  await app.press("id:tran-llm-model-button");
  await app.waitUntil(() => app.shows("id:tran-llm-model-item-monkey"));
  await app.press("id:tran-llm-model-item-monkey");
  await app.waitUntil(() => app.texts().some((s) => s.includes("model=monkey|pin=true")));
  expect(app.shows("id:tran-llm-model-menu")).toBe(false);

  // 拉取：先有地址，装置经 host 事件通道 resolve 迟到结果。
  await app.host.send("harness", { action: "set-base" });
  await app.waitUntil(() => app.texts().some((s) => s.includes("base=https://api.test/v1")));
  await app.press("id:tran-llm-pull");
  await app.host.send("harness", { action: "pull-ok" });
  await app.waitUntil(() =>
    app.texts().some((s) => s.includes(tf("settings.tranLlmModelCount", { count: 2 }))),
  );
  await app.press("id:tran-llm-model-button");
  await app.waitUntil(() => app.shows("id:tran-llm-model-item-kimi-k2.6"));
  await app.waitUntil(() => app.shows("id:tran-llm-model-total"));
  await app.press("id:tran-llm-model-item-kimi-k2.6");
  await app.waitUntil(() => app.texts().some((s) => s.includes("model=kimi-k2.6|pin=true")));
});

test("the key field writes on commit, keeps saved key on empty blur, and clears on explicit clear button", async () => {
  const app = await openSection();
  await app.waitUntil(() => app.shows("id:tran-engine-machine"), { timeout: 2000 });
  await app.press("id:tran-engine-llm");
  await app.waitUntil(() => app.shows("id:tran-llm-key"));
  // 「已保存」只在密钥状态行单独成句（提示文案里也提到这四个字，所以按整
  // 串比，不做子串匹配）。
  expect(app.texts()).not.toContain(t("settings.tranLlmKeySaved"));
  await app.type("id:tran-llm-key", "sk-ui-1");
  await app.host.send("harness", { action: "commit-key" });
  await app.waitUntil(() => app.texts().some((s) => s.includes("key:7 chars")));
  await app.waitUntil(() => app.texts().includes(t("settings.tranLlmKeySaved")));
  expect(app.shows("id:tran-llm-key-clear")).toBe(true);

  // 空草稿或纯空白失焦/回车不得清空已保存密钥。
  await app.submit("id:tran-llm-key");
  await app.type("id:tran-llm-key", "   ");
  await app.submit("id:tran-llm-key");
  await app.host.send("harness", { action: "commit-key" });
  await app.wait(40);
  expect(app.texts()).toContain(t("settings.tranLlmKeySaved"));
  expect(app.texts()).toContain("key:7 chars");

  // 显式点击清除按钮才会清空密钥。
  await app.press("id:tran-llm-key-clear");
  await app.waitUntil(() => app.texts().includes("key:0 chars"));
  expect(app.shows("id:tran-llm-key-clear")).toBe(false);
  expect(app.texts()).not.toContain(t("settings.tranLlmKeySaved"));
});

test("pull with empty base url shows validation error and custom model id saves before and after fetch", async () => {
  const app = await openSection();
  await app.waitUntil(() => app.shows("id:tran-engine-machine"), { timeout: 2000 });
  await app.press("id:tran-engine-llm");
  await app.waitUntil(() => app.shows("id:tran-llm-pull"));

  // 未填接口地址点拉取：展示校验错误提示，不静默返回。
  await app.press("id:tran-llm-pull");
  await app.waitUntil(() => app.shows("id:tran-llm-model-error"));
  expect(app.texts()).toContain(t("settings.tranLlmBaseUrlHint"));

  // 未拉取模型列表时直接输入自定义模型 id 并点击确认按钮保存。
  await app.clear("id:tran-llm-model-input");
  await app.type("id:tran-llm-model-input", "custom-before-fetch");
  await app.waitUntil(() => app.shows("id:tran-llm-model-confirm"));
  await app.press("id:tran-llm-model-confirm");
  await app.waitUntil(() =>
    app.texts().some((s) => s.includes("model=custom-before-fetch|pin=true")),
  );

  // 填写接口地址并拉取模型列表后，仍可经下拉搜索输入自定义模型 id 并点击确认保存。
  await app.host.send("harness", { action: "set-base" });
  await app.press("id:tran-llm-pull");
  await app.host.send("harness", { action: "pull-ok" });
  await app.waitUntil(() =>
    app.texts().some((s) => s.includes(tf("settings.tranLlmModelCount", { count: 2 }))),
  );
  await app.press("id:tran-llm-model-button");
  await app.waitUntil(() => app.shows("id:tran-llm-model-search"));
  await app.type("id:tran-llm-model-search", "custom-after-fetch");
  await app.waitUntil(() => app.shows("id:tran-llm-model-custom-option"));
  await app.press("id:tran-llm-model-menu-confirm");
  await app.waitUntil(() =>
    app.texts().some((s) => s.includes("model=custom-after-fetch|pin=true")),
  );
  expect(app.shows("id:tran-llm-model-menu")).toBe(false);

  // 直接在输入框输入后失焦提交自定义模型 id。
  await app.clear("id:tran-llm-model-input");
  await app.type("id:tran-llm-model-input", "custom-on-blur");
  await app.host.send("harness", { action: "commit-model" });
  await app.waitUntil(() =>
    app.texts().some((s) => s.includes("model=custom-on-blur|pin=true")),
  );
});

test("switching back to machine unmounts the llm block", async () => {
  const app = await openSection();
  await app.waitUntil(() => app.shows("id:tran-engine-machine"), { timeout: 2000 });
  await app.press("id:tran-engine-llm");
  await app.waitUntil(() => app.shows("id:tran-llm-base-url"));
  await app.press("id:tran-engine-machine");
  await app.waitUntil(() => app.texts().some((s) => s.includes("tran:machine|")));
  expect(app.shows("id:tran-llm-base-url")).toBe(false);
  expect(app.shows("id:tran-llm-model-field")).toBe(false);
});
