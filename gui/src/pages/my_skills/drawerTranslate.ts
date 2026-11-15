// 详情抽屉描述翻译会话 — detail_drawer.rs 的语言按钮半（crate translation
// 的会话簿记）+ marketplace/detail_drawer/translate.ts 同一契约的 my_skills
// 版（页面不互相 import，会话各自持有）。
//
// 行为契约（与市场详情列一致，差异只在写入口）：
// - 读侧：存储的 per-skill 选择优先（description_choices[name]），其余跟
//   设置的描述开关（resolved_translate）；
// - 按钮：目标语言不需要翻译的文案（如中文之于 zh-CN）不画；点击只翻当
//   前打开并按技能名记住选择，不写设置开关；从原文翻过去时清掉会话内的
//   模型拒绝状态（retry_rejected）；
// - 写入口走本页约定：MySkills.setDescriptionChoices 整表写入 + 页面
//   state.descriptionChoices 信号同步（卡面随信号重绘）；
// - 实时引擎：选了译文但磁盘缓存未命中时补一次 fill，成功写盘后 bump
//   generation 让 JSX 重读 lookup；模型拒绝 → 会话钉住 + 通知带（同
//   replaceKey 顶替）。
//
// 会话簿记（GPUI 进程级 statics 的对应物）：inflight 去重、传输失败静默
// （nsh effect 重跑会热循环，改由下一次按钮按下或换卡再问）。

import { signal, type Signal } from "nsh";
import { MarketplaceTranslation, MySkills } from "skillstar-gui/native";
import { publishToast } from "../../shell/events";
import { tf } from "../../services/translation";
import { descriptionChoices } from "./state";

/** 模型拒绝（fill 报告的桥形状）。 */
export interface ModelRejection {
  model: string;
  detail: string;
}

export interface DrawerTranslateBridge {
  /** 描述面设置开关（translate_descriptions）。 */
  descriptionsEnabled(): boolean;
  /** 活动目标是否需要翻译这段文案（needs_translation）。 */
  needsTranslation(text: string): boolean;
  /** 活动目标的磁盘缓存译文，未命中为 null。 */
  lookup(source: string): string | null;
  /** LLM 引擎选了但跑不起来（无模型或无密钥）。 */
  blocked(): boolean;
  /** 语言按钮写入（本页：整表存储 + 信号同步）。 */
  setChoices(choices: Record<string, boolean>): Promise<void> | void;
  /** 实时引擎调用（fill）：成功写盘后返回 done 列表与可能的拒绝。 */
  fill(
    sources: string[]
  ): Promise<{ done: { source: string; text: string }[]; rejection: ModelRejection | null }>;
}

/** 真实桥：翻译读数走共享翻译缝（MarketplaceTranslation，登记名带市场
 * 前缀但它是翻译域的统一出口）；写入走本页的 MySkills 存储约定。 */
export const drawerBridge: DrawerTranslateBridge = {
  descriptionsEnabled: () => MarketplaceTranslation.descriptionsEnabled(),
  needsTranslation: (text) => MarketplaceTranslation.needsTranslation(text),
  lookup: (source) => MarketplaceTranslation.lookup(source),
  blocked: () => MarketplaceTranslation.descriptionsBlocked(),
  setChoices: (choices) => MySkills.setDescriptionChoices(choices),
  fill: (sources) =>
    MarketplaceTranslation.fillDescriptions(sources) as Promise<{
      done: { source: string; text: string }[];
      rejection: ModelRejection | null;
    }>,
};

/** translation.rs rejection_message。 */
export function rejectionMessage(model: string, detail: string): string {
  const trimmedModel = model.trim();
  const trimmedDetail = detail.trim().replace(/[.。]+$/, "");
  if (trimmedDetail.length === 0) {
    return tf("detailPanel.translationRejected", { model: trimmedModel });
  }
  return tf("detailPanel.translationRejectedDetail", {
    model: trimmedModel,
    detail: trimmedDetail,
  });
}

export interface DrawerTranslateSession {
  /** 会话代数：选择写入 / 译文落地 / 拒绝变化后 bump，JSX 借它重读
   * lookup 与 choice。 */
  generation: Signal<number>;
  /** resolved_translate：传入的 per-skill 选择表（抽屉 props，页面 state
   * 的同一存储）优先，缺省跟设置开关。 */
  resolveTranslate(name: string, choices: Record<string, boolean>): boolean;
  /** display_when：开着译文显示缓存行，未命中或关闭显示原文。 */
  descriptionLine(text: string, showing: boolean): string;
  /** 语言按钮门：visible=needs_translation；pending=正显示译文、引擎可用
   * 且这行还没落地（icon_sweep 的扫描态）。 */
  buttonGate(text: string, showing: boolean): { visible: boolean; pending: boolean };
  /** 描述下方的一行弱化提示：会话内的模型拒绝文案（failure_line）。 */
  failureLine(): string | null;
  /** 一次按钮按下：showing=当前是否译文；从原文翻过去时清拒绝状态
   * （retry_rejected），并整表记住选择 + 同步页面信号。 */
  applyChoice(name: string, showing: boolean): void;
  /** 为当前显示行排一次引擎调用（schedule_when 的单片版）。 */
  ensureTranslation(text: string, showing: boolean): void;
  /** 会话内是否已有模型拒绝。 */
  rejected(): boolean;
}

export function createDrawerTranslateSession(
  bridge: DrawerTranslateBridge
): DrawerTranslateSession {
  const generation = signal(0);
  // 会话级簿记（页面 keep-alive 单实体）。
  let rejection: ModelRejection | null = null;
  const inflight = new Set<string>();
  const failed = new Set<string>();

  function bump(): void {
    generation.set(generation() + 1);
  }

  function persistChoice(name: string, translated: boolean): void {
    // 本页约定：整表写入（MySkills.setDescriptionChoices）+ 信号同步，卡
    // 面与抽屉一起换行。存盘失败不影响本次显示，只留痕。
    const next = { ...descriptionChoices(), [name]: translated };
    descriptionChoices.set(next);
    try {
      const out = bridge.setChoices(next);
      if (out !== null && typeof out === "object" && typeof (out as Promise<void>).catch === "function") {
        (out as Promise<void>).catch((err: unknown) => {
          console.warn("failed to save the description choice:", String(err));
        });
      }
    } catch (err) {
      console.warn("failed to save the description choice:", String(err));
    }
  }

  return {
    generation,
    resolveTranslate(name: string, choices: Record<string, boolean>): boolean {
      const choice = choices[name];
      return choice === undefined ? bridge.descriptionsEnabled() : choice;
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
        visible &&
        bridge.lookup(text) === null &&
        rejection === null &&
        !failed.has(text.trim());
      return { visible, pending };
    },
    failureLine(): string | null {
      if (rejection === null) return null;
      return rejectionMessage(rejection.model, rejection.detail);
    },
    rejected(): boolean {
      return rejection !== null;
    },
    applyChoice(name: string, showing: boolean): void {
      // 从原文翻过去：清拒绝与失败标记，让引擎可以再问（retry_rejected）。
      if (!showing) {
        rejection = null;
        failed.clear();
      }
      persistChoice(name, !showing);
      bump();
    },
    ensureTranslation(text: string, showing: boolean): void {
      if (!showing || rejection !== null || bridge.blocked()) return;
      const trimmed = text.trim();
      if (trimmed.length === 0 || !bridge.needsTranslation(trimmed)) return;
      if (bridge.lookup(trimmed) !== null || inflight.has(trimmed) || failed.has(trimmed)) {
        return;
      }
      inflight.add(trimmed);
      bridge
        .fill([trimmed])
        .then((report) => {
          inflight.delete(trimmed);
          if (report.rejection !== null) {
            // 每会话只播报一次；同 replaceKey 的旧通知被顶替。
            rejection = report.rejection;
            publishToast({
              tone: "error",
              message: rejectionMessage(report.rejection.model, report.rejection.detail),
              pinned: true,
              replaceKey: "translation-model-rejected",
            });
          } else if (report.done.some((item) => item.source === trimmed)) {
            failed.delete(trimmed);
          }
          bump();
        })
        .catch(() => {
          // 传输失败：本会话静默这一行，不写缓存。
          inflight.delete(trimmed);
          failed.add(trimmed);
          bump();
        });
    },
  };
}

/** 页面实体（keep-alive 单挂载）。 */
export const drawerTranslateSession = createDrawerTranslateSession(drawerBridge);
