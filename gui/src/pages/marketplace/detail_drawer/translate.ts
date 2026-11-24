import { signal, type Signal } from "nsh";
import { MarketplaceTranslation } from "skillstar-gui/native";
import { publishToast } from "../../../shell/events";
import { mt, mtf } from "../marketStrings";

// 详情列描述翻译 — detail_drawer/translate.rs + crates/ss-gpui/src/
// translation.rs 的会话半对位。
//
// 行为与技能详情列的语言按钮同一契约：
// - 读侧：存储的选择（description_choices[name]）优先，其余跟设置开关
//   （resolved_translate）；
// - 按钮：目标语言不需要翻译的文案（如中文之于 zh-CN）不画按钮；点击只
//   翻译当前打开并按技能名记住选择（set_description_choice），不写设置
//   开关；从原文翻过去时清掉会话内的模型拒绝状态（retry_rejected）；
// - 实时引擎：选了译文但磁盘缓存未命中时调 fill 补一行，成功写盘后 bump
//   generation 让 JSX 重读 lookup；模型 HTTP-400 拒绝 → 会话级钉住 +
//   通知带（replaceKey 互相顶替），下一次从原文翻过去时清除。
//
// 与 GPUI 调度器的差异（会话簿记搬进本模块）：网络失败不写缓存，也不
// 自动重排（nsh 的 effect 重跑会热循环）。选择写入失败滚回信号；传输
// 失败在段落下给 Retry（清失败再 fill）。不做打字机。模型拒绝仍钉住通知。
//
// 每个实例自持状态（createTranslateSession 工厂），抽屉模块级单例即页面
// 实体；测试各建各的并注入假桥。

/** 模型拒绝（FillReport.rejection 的桥形状）。 */
export interface ModelRejection {
  model: string;
  detail: string;
}

export interface TranslateBridge {
  /** 描述面设置开关（translate_descriptions）。 */
  descriptionsEnabled(): boolean;
  /** 存储的 per-skill 选择：true/false=已选，null=跟开关。 */
  descriptionChoice(name: string): boolean | null;
  /** 活动目标是否需要翻译这段文案（needs_translation）。 */
  needsTranslation(text: string): boolean;
  /** 活动目标的磁盘缓存译文，未命中为 null。 */
  lookup(source: string): string | null;
  /** LLM 引擎选了但跑不起来（无模型或无密钥）。 */
  blocked(): boolean;
  /** 记住一次按钮选择（set_description_choice）。 */
  setChoice(name: string, translated: boolean): unknown;
  /** 实时引擎调用（fill）：成功写盘后返回 done 列表与可能的拒绝。 */
  fill(sources: string[]): Promise<{ done: { source: string; text: string }[]; rejection: ModelRejection | null }>;
}

/** 真实桥：MarketplaceTranslation facade（本切片在 native facade 侧补的
 * 写选择 + fill 方法）。 */
export const facadeBridge: TranslateBridge = {
  descriptionsEnabled: () => MarketplaceTranslation.descriptionsEnabled(),
  descriptionChoice: (name) => MarketplaceTranslation.descriptionChoice(name),
  needsTranslation: (text) => MarketplaceTranslation.needsTranslation(text),
  lookup: (source) => MarketplaceTranslation.lookup(source),
  blocked: () => MarketplaceTranslation.descriptionsBlocked(),
  setChoice: (name, translated) => MarketplaceTranslation.setDescriptionChoice(name, translated),
  fill: (sources) => MarketplaceTranslation.fillDescriptions(sources) as Promise<{
    done: { source: string; text: string }[];
    rejection: ModelRejection | null;
  }>,
};

/** rejection_message（translation.rs:367-378）。 */
export function rejectionMessage(model: string, detail: string): string {
  const trimmedModel = model.trim();
  const trimmedDetail = detail.trim().replace(/[.。]+$/, "");
  if (trimmedDetail.length === 0) {
    return mtf("detailPanel.translationRejected", { model: trimmedModel });
  }
  return mtf("detailPanel.translationRejectedDetail", {
    model: trimmedModel,
    detail: trimmedDetail,
  });
}

export interface TranslateSession {
  /** 会话代数：选择写入 / 译文落地 / 拒绝变化后 bump，JSX 借它重读
   * lookup 与 choice。 */
  generation: Signal<number>;
  /** resolved_translate：存储选择优先，其余跟设置开关。同步桥读，调用
   * 点必须落在 JSX 表达式（配 generation）或 effect 里。 */
  resolveTranslate(name: string): boolean;
  /** display_when（translation.rs:161-167）：开着译文显示缓存行，未命中
   * 或关闭显示原文。 */
  descriptionLine(text: string, showing: boolean): string;
  /** 语言按钮门：visible=needs_translation；pending=正显示译文、引擎可
   * 用且这行还没落地（icon_sweep 的扫描态）。 */
  buttonGate(text: string, showing: boolean): { visible: boolean; pending: boolean };
  /** 描述下方的一行弱化提示：blocked 配置提示 / 模型拒绝 / 传输失败文案。 */
  failureLine(text?: string): string | null;
  /** 传输失败的原文。段落下的 Retry 读它；模型拒绝仍走 failureLine。 */
  transportFailed: Signal<string | null>;
  /** 一次按钮按下（apply_description_choice）：showing=当前是否译文。
   * 从原文翻过去时清拒绝状态（retry_rejected），并记住选择。写入失败滚回。 */
  applyChoice(name: string, showing: boolean): void;
  /** 为当前显示行排一次引擎调用（schedule_when 的单片版）：开着译文、
   * 需要翻译、无缓存、非在途、未拒绝、引擎可用才发。 */
  ensureTranslation(name: string, text: string, showing: boolean): void;
  /** 清掉传输失败后再 fill 一次。不造第二条 promise 以外的取消。 */
  retryTransport(name: string, text: string): void;
  /** 设置页保存后清掉失败态/拒绝态，并递增代数让打开的抽屉重问引擎。 */
  onPrefsChanged(): void;
  /** 会话内是否已有模型拒绝（按钮 pending 与 failureLine 用）。 */
  rejected(): boolean;
}

export function createTranslateSession(bridge: TranslateBridge): TranslateSession {
  const generation = signal(0);
  const transportFailed = signal<string | null>(null);
  // 会话级簿记（GPUI 的进程级 statics 对应物；页面 keep-alive 单实体）。
  let rejection: ModelRejection | null = null;
  let prefsEpoch = 0;
  const inflight = new Set<string>();
  // 传输层失败（非模型拒绝）的行：不再自动重排（effect 重跑会热循环）。
  // 段落下的 Retry 清标记后再 fill；按钮从原文再翻也会清。
  const failed = new Set<string>();

  function isThenable(out: unknown): out is Promise<unknown> {
    return out !== null && typeof out === "object" && "catch" in out;
  }

  /** 选择写入失败：把存储的选择拨回按下之前，并 bump 让界面重读。 */
  function restoreChoice(name: string, previous: boolean): void {
    try {
      const out = bridge.setChoice(name, previous);
      if (isThenable(out)) {
        void out.catch((err: unknown) => warnChoice(err));
      }
    } catch (err) {
      warnChoice(err);
    }
    bump();
  }

  function bump(): void {
    generation.set(generation() + 1);
  }

  function markTransportFailed(name: string, trimmed: string): void {
    failed.add(trimmed);
    transportFailed.set(trimmed);
    try {
      const out = bridge.setChoice(name, false);
      if (isThenable(out)) void out.catch((err: unknown) => warnChoice(err));
    } catch (err) {
      warnChoice(err);
    }
  }

  function ensureTranslation(name: string, text: string, showing: boolean): void {
    if (!showing || rejection !== null || bridge.blocked()) return;
    const trimmed = text.trim();
    if (trimmed.length === 0 || !bridge.needsTranslation(trimmed)) return;
    if (bridge.lookup(trimmed) !== null || inflight.has(trimmed) || failed.has(trimmed)) return;
    const runEpoch = prefsEpoch;
    inflight.add(trimmed);
    bridge
      .fill([trimmed])
      .then((report) => {
        if (runEpoch !== prefsEpoch) return;
        inflight.delete(trimmed);
        if (report.rejection !== null) {
          // 每会话只播报一次；同 replaceKey 的旧通知被顶替。钉住通知保留。
          rejection = report.rejection;
          publishToast({
            tone: "error",
            message: rejectionMessage(report.rejection.model, report.rejection.detail),
            pinned: true,
            replaceKey: "translation-model-rejected",
          });
        } else if (
          report.done.some((item) => item.source === trimmed) ||
          bridge.lookup(trimmed) !== null
        ) {
          failed.delete(trimmed);
          if (transportFailed() === trimmed) transportFailed.set(null);
        } else {
          markTransportFailed(name, trimmed);
        }
        bump();
      })
      .catch(() => {
        if (runEpoch !== prefsEpoch) return;
        // 传输失败：不自动重排。滚回「显示原文」，段落下的 Retry 再 fill。
        inflight.delete(trimmed);
        markTransportFailed(name, trimmed);
        bump();
      });
  }

  function retryTransport(name: string, text: string): void {
    const trimmed = text.trim();
    failed.delete(trimmed);
    transportFailed.set(null);
    try {
      const out = bridge.setChoice(name, true);
      if (isThenable(out)) void out.catch((err: unknown) => warnChoice(err));
    } catch (err) {
      warnChoice(err);
    }
    bump();
    ensureTranslation(name, trimmed, true);
  }

  const session: TranslateSession = {
    generation,
    resolveTranslate(name: string): boolean {
      const choice = bridge.descriptionChoice(name);
      return choice === null ? bridge.descriptionsEnabled() : choice;
    },
    descriptionLine(text: string, showing: boolean): string {
      if (!showing) return text;
      return bridge.lookup(text) ?? text;
    },
    buttonGate(text: string, showing: boolean): { visible: boolean; pending: boolean } {
      const visible = bridge.needsTranslation(text);
      const pending =
        showing &&
        !bridge.blocked() &&
        bridge.needsTranslation(text) &&
        bridge.lookup(text) === null &&
        rejection === null &&
        !failed.has(text.trim());
      return { visible, pending };
    },
    failureLine(text?: string): string | null {
      if (text !== undefined && bridge.lookup(text) !== null) return null;
      if (bridge.blocked() && (text === undefined || bridge.needsTranslation(text))) {
        return mt("detailPanel.translationNeedsConfig");
      }
      if (rejection !== null) {
        return rejectionMessage(rejection.model, rejection.detail);
      }
      if (text !== undefined && transportFailed() === text.trim()) {
        return mt("detailPanel.translationFailed");
      }
      return null;
    },
    transportFailed,
    rejected(): boolean {
      return rejection !== null;
    },
    applyChoice(name: string, showing: boolean): void {
      const next = !showing;
      // 从原文翻过去：先清拒绝与失败标记，让引擎可以再问（retry_rejected）。
      if (!showing) {
        rejection = null;
        failed.clear();
      }
      transportFailed.set(null);
      try {
        const out = bridge.setChoice(name, next);
        bump();
        if (isThenable(out)) {
          void out.catch((err: unknown) => {
            restoreChoice(name, showing);
            warnChoice(err);
          });
        }
      } catch (err) {
        restoreChoice(name, showing);
        warnChoice(err);
      }
    },
    ensureTranslation,
    retryTransport,
    onPrefsChanged(): void {
      prefsEpoch += 1;
      inflight.clear();
      failed.clear();
      rejection = null;
      transportFailed.set(null);
      bump();
    },
  };
  return session;
}

function warnChoice(err: unknown): void {
  console.warn("failed to save the description choice:", String(err));
}
