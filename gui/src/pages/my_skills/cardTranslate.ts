import { batch, signal } from "nsh";
import { MarketplaceTranslation } from "skillstar-gui/native";
import { locale } from "../../services/locale";
import { publishToast, translationPrefsChanged } from "../../shell/events";
import { descriptionChoices, skills } from "./state";
import { rtf } from "./readerStrings";

/**
 * 卡片描述翻译队列与会话缓存（P1-S3）：
 * - 当全局开关开启且界面为中文时，对尚未回填 `localizedDescription` 的卡片
 *   按视口/批量排队调用 `MarketplaceTranslation.fillDescriptions`；
 * - 选择栏「批量翻译描述」调用 `fillSelectedDescriptions` 主动补齐选中项并
 *   同步回填到 `skills` 与缓存信号，使卡面即时重绘。
 */

export interface TranslationFillBridge {
  descriptionsEnabled(): boolean;
  needsTranslation(text: string): boolean;
  lookup(source: string): string | null;
  blocked(): boolean;
  fillDescriptions(sources: string[]): Promise<{
    done: { source: string; text: string }[];
    rejection: { model: string; detail: string } | null;
  }>;
}

const nativeFillBridge: TranslationFillBridge = {
  descriptionsEnabled: () => MarketplaceTranslation.descriptionsEnabled(),
  needsTranslation: (text) => MarketplaceTranslation.needsTranslation(text),
  lookup: (source) => MarketplaceTranslation.lookup(source),
  blocked: () => MarketplaceTranslation.descriptionsBlocked(),
  fillDescriptions: (sources) =>
    MarketplaceTranslation.fillDescriptions(sources) as Promise<{
      done: { source: string; text: string }[];
      rejection: { model: string; detail: string } | null;
    }>,
};

/** 源原文 -> 译文 响应式缓存，驱动卡片即时刷新。 */
export const cardTranslationCache = signal<Record<string, string>>({});

/** 翻译偏好变更时递增代数，清掉旧缓存与会话拒绝态，让仍在队列门外的卡片重新求值。 */
const cacheGeneration = signal(0);

const BATCH_SIZE = 20;
const BATCH_DELAY_MS = 24;

const pendingQueue = new Set<string>();
const inflightSources = new Set<string>();
const rejectedSources = new Set<string>();
let flushTimer: ReturnType<typeof setTimeout> | null = null;
let sessionRejected = false;
let prefsEpoch = 0;

translationPrefsChanged.subscribe(() => {
  prefsEpoch += 1;
  if (flushTimer !== null) {
    clearTimeout(flushTimer);
    flushTimer = null;
  }
  pendingQueue.clear();
  inflightSources.clear();
  rejectedSources.clear();
  sessionRejected = false;
  batch(() => {
    cardTranslationCache.set({});
    cacheGeneration.update((n) => n + 1);
  });
});

export function isChineseLocale(): boolean {
  return locale().startsWith("zh");
}

export function cardTranslatedText(source: string): string | null {
  void cacheGeneration();
  const trimmed = source.trim();
  if (trimmed.length === 0) return null;
  const cached = cardTranslationCache()[trimmed];
  if (typeof cached === "string" && cached.length > 0) return cached;
  return null;
}

export function recordCardTranslations(done: { source: string; text: string }[]): void {
  applyDoneToSignals(done);
}

function applyDoneToSignals(done: { source: string; text: string }[]): void {
  if (done.length === 0) return;
  const nextCache: Record<string, string> = { ...cardTranslationCache() };
  const mapBySource = new Map<string, string>();
  for (const item of done) {
    const src = item.source.trim();
    const text = item.text.trim();
    if (src.length === 0 || text.length === 0) continue;
    nextCache[src] = text;
    mapBySource.set(src, text);
  }
  if (mapBySource.size === 0) return;
  cardTranslationCache.set(nextCache);

  const current = skills();
  let changed = false;
  const patched = current.map((skill) => {
    const key = skill.description.trim();
    const hit = mapBySource.get(key);
    if (!hit || skill.localizedDescription === hit) return skill;
    changed = true;
    return { ...skill, localizedDescription: hit };
  });
  if (changed) {
    skills.set(patched);
  }
}

function scheduleFlush(bridge: TranslationFillBridge): void {
  if (flushTimer !== null || pendingQueue.size === 0) return;
  flushTimer = setTimeout(() => {
    flushTimer = null;
    void flushQueue(bridge);
  }, BATCH_DELAY_MS);
}

async function flushQueue(bridge: TranslationFillBridge): Promise<void> {
  if (pendingQueue.size === 0 || sessionRejected) return;
  const runEpoch = prefsEpoch;
  const batch: string[] = [];
  for (const src of pendingQueue) {
    pendingQueue.delete(src);
    if (cardTranslationCache()[src] !== undefined || inflightSources.has(src)) continue;
    inflightSources.add(src);
    batch.push(src);
    if (batch.length >= BATCH_SIZE) break;
  }
  if (pendingQueue.size > 0) {
    scheduleFlush(bridge);
  }
  if (batch.length === 0) return;
  try {
    const report = await bridge.fillDescriptions(batch);
    if (runEpoch !== prefsEpoch) return;
    applyDoneToSignals(report.done ?? []);
    if (report.rejection) {
      sessionRejected = true;
      for (const src of batch) rejectedSources.add(src);
    }
  } catch {
    // 后台排队失败保持静默，由用户下次切回或点详情重试。
  } finally {
    if (runEpoch === prefsEpoch) {
      for (const src of batch) inflightSources.delete(src);
      if (pendingQueue.size > 0 && !sessionRejected) {
        scheduleFlush(bridge);
      }
    }
  }
}

/**
 * 视口卡片渲染时调用：当条目尚无 `localizedDescription` 且满足：
 * - 单卡选择为 true，或（未显式 false 且全局开关开启且界面为中文）
 * 先查同步磁盘缓存；未命中则按批排队调 `fillDescriptions`。
 */
export function ensureCardDescriptionTranslation(
  skillName: string,
  rawDescription: string,
  localizedFromSnapshot?: string,
  bridge: TranslationFillBridge = nativeFillBridge
): void {
  void cacheGeneration();
  if (typeof localizedFromSnapshot === "string" && localizedFromSnapshot.trim().length > 0) {
    return;
  }
  const source = rawDescription.trim();
  if (source.length === 0) return;
  if (cardTranslationCache()[source] !== undefined) return;

  const choice = descriptionChoices()[skillName];
  if (choice === false) return;
  const shouldAuto = choice === true || (bridge.descriptionsEnabled() && isChineseLocale());
  if (!shouldAuto) return;

  const hit = bridge.lookup(source);
  if (hit && hit.trim().length > 0) {
    applyDoneToSignals([{ source, text: hit }]);
    return;
  }

  if (sessionRejected || bridge.blocked()) return;
  if (inflightSources.has(source) || pendingQueue.has(source) || rejectedSources.has(source)) {
    return;
  }
  if (!bridge.needsTranslation(source)) return;
  pendingQueue.add(source);
  scheduleFlush(bridge);
}

/** 选择栏「批量翻译描述」主动补齐选中技能的译文并写回展示状态。 */
export async function fillSelectedDescriptions(
  sources: string[],
  bridge: TranslationFillBridge = nativeFillBridge
): Promise<void> {
  const unique = Array.from(
    new Set(
      sources
        .map((s) => s.trim())
        .filter((s) => s.length > 0 && cardTranslationCache()[s] === undefined)
    )
  );
  if (unique.length === 0) return;

  const warmHits: { source: string; text: string }[] = [];
  const missing: string[] = [];
  for (const src of unique) {
    const hit = bridge.lookup(src);
    if (hit && hit.trim().length > 0) {
      warmHits.push({ source: src, text: hit });
    } else if (!inflightSources.has(src) && bridge.needsTranslation(src)) {
      missing.push(src);
    }
  }
  applyDoneToSignals(warmHits);
  if (missing.length === 0 || bridge.blocked()) return;

  const runEpoch = prefsEpoch;
  sessionRejected = false;
  for (let i = 0; i < missing.length; i += BATCH_SIZE) {
    if (runEpoch !== prefsEpoch) break;
    const chunk = missing.slice(i, i + BATCH_SIZE);
    for (const src of chunk) inflightSources.add(src);
    try {
      const report = await bridge.fillDescriptions(chunk);
      if (runEpoch !== prefsEpoch) break;
      applyDoneToSignals(report.done ?? []);
      if (report.rejection) {
        sessionRejected = true;
        const model = report.rejection.model.trim();
        const detail = report.rejection.detail.trim().replace(/[.。]+$/, "");
        publishToast({
          tone: "warning",
          replaceKey: "translation-rejected",
          message:
            detail.length > 0
              ? rtf("detailPanel.translationRejectedDetail", { model, detail })
              : rtf("detailPanel.translationRejected", { model }),
        });
        break;
      }
    } catch {
      // 批翻译网络失败保留内存选择，允许下次重按补齐。
      break;
    } finally {
      if (runEpoch === prefsEpoch) {
        for (const src of chunk) inflightSources.delete(src);
      }
    }
  }
}

export function resetCardTranslateForTest(): void {
  prefsEpoch += 1;
  if (flushTimer !== null) {
    clearTimeout(flushTimer);
    flushTimer = null;
  }
  pendingQueue.clear();
  inflightSources.clear();
  rejectedSources.clear();
  sessionRejected = false;
  cardTranslationCache.set({});
}
