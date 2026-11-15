// 「翻译」分区的无头测试装置（settings.test.host.tsx 样板）：假桥注入
// TranslationWorkbench 的 state 工厂位；读数 signal 在假桥调用点直接 set
// （波 1 教训的装置版）。saveTranslation 复刻 facade 语义：盘上的
// description_choices / reader_theme 不被写丢；密钥走独立的一份内存
// 「文件」（loadLlmKey/saveLlmKey），页面拿不到内容只拿「有没有」。模型拉取
// 是手动扳机：装置动作经 host 事件通道决定迟到的 Promise 如何 resolve。
// 不触文件系统、不写 $HOME——真域路径由 native crate 的 cargo 测试在
// SKILLSTAR_DATA_DIR 沙箱覆盖（facades/settings_updates.rs）。

import { Text, View, host, mount, signal, type JSX } from "nsh";
import {
  TranslationWorkbench,
  createTranslationState,
  type ModelPullReport,
  type TranslationBridge,
  type TranslationConfigDto,
} from "./translation";

// ---- 假桥：内存态 + 调用记录 + 手动扳机的拉取 -----------------------------

function defaultDisk(): TranslationConfigDto {
  return {
    engine: "machine",
    translate_descriptions: false,
    translate_skill_md: false,
    description_choices: { "kept-skill": true },
    reader_theme: "wavy",
    target_lang: "zh-CN",
    llm_base_url: "",
    // 预置一个模型：未拉取时 combobox 只有这一行（sync 的首行补位语义）。
    llm_model: "monkey",
    llm_model_pinned: false,
    llm_account_id: "",
    llm_account_catalog: "",
  };
}

let disk: TranslationConfigDto = defaultDisk();
/** 手填密钥的内存版（独立 0600 文件的对位）。 */
let keyFile = "";

/** 最近一次写盘调用的 dump（假桥调用点直接写 signal）。 */
const readout = signal("none");
/** 密钥写盘的次数与最后一次内容（测试断言用）。 */
const keyReadout = signal("none");

/** 手动扳机：测试经 host 事件通道 resolve 正在进行的拉取。 */
let pendingPull: ((report: ModelPullReport) => void) | null = null;

const fakeBridge: TranslationBridge = {
  loadTranslation() {
    return {
      ...disk,
      description_choices: { ...disk.description_choices },
    };
  },
  saveTranslation(next) {
    // facade save_translation 语义：盘上的选择与阅读器样式不被写丢。
    disk = {
      ...next,
      description_choices: { ...disk.description_choices },
      reader_theme: disk.reader_theme,
    };
    readout.set(
      `tran:${next.engine}|${next.target_lang}|d=${next.translate_descriptions}` +
        `|base=${next.llm_base_url}|model=${next.llm_model}|pin=${next.llm_model_pinned}`,
    );
    return {
      config: { ...disk, description_choices: { ...disk.description_choices } },
      error: null,
    };
  },
  translationLanguages() {
    return [
      { code: "zh-CN", label: "简体中文" },
      { code: "zh-TW", label: "繁體中文" },
      { code: "en", label: "English" },
      { code: "ja", label: "日本語" },
      { code: "ko", label: "한국어" },
    ];
  },
  loadLlmKey() {
    return keyFile;
  },
  saveLlmKey(key) {
    keyFile = key.trim();
    keyReadout.set(`key:${keyFile.length} chars`);
  },
  pullModels() {
    return new Promise<ModelPullReport>((resolve) => {
      pendingPull = resolve;
    });
  },
};

const state = createTranslationState(fakeBridge);

// ---- 装置动作（nsh 0.5：press 有命中测试，装置动作走 host 事件通道） ------

const harnessActions: Record<string, () => void> = {
  /** 填接口地址（地址框是失焦提交，装置直接走 state 的同一步）。 */
  "set-base": () => state.setLlmBaseUrl("https://api.test/v1"),
  /** 提交密钥草稿（输入框的 onChange 已经把草稿放进 state）。 */
  "commit-key": () => state.commitLlmKey(),
  "pull-ok": () => {
    pendingPull?.({ ok: true, models: ["deepseek-v4.1-flash", "kimi-k2.6"] });
  },
  "pull-missing-key": () => {
    pendingPull?.({ ok: false, error: "missing_key" });
  },
};

host.on("harness", (data) => {
  const action = (data as { action?: string } | null)?.action;
  if (action !== undefined) harnessActions[action]?.();
});

function App(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <Text testID="host-readout" style={{ fontSize: 10 }}>
        {readout()}
      </Text>
      <Text testID="host-key-readout" style={{ fontSize: 10 }}>
        {keyReadout()}
      </Text>
      <View style={{ display: "flex", flexDirection: "row", flex: 1, minHeight: 0 }}>
        <TranslationWorkbench state={state} />
      </View>
    </View>
  );
}

export default App;

mount(App);
