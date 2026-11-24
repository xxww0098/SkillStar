import { batch, signal } from "nsh";
import { MarketplaceTranslation } from "skillstar-gui/native";
import { locale } from "../../services/locale";
import { translationPrefsChanged } from "../../shell/events";

/**
 * Marketplace 卡片描述翻译队列与缓存（P1-S3）：
 * 当全局开关开启且界面为中文但条目尚无 `localizedDescription` 时，
 * 按视口渲染到的卡片以批次 20 条排队调用 `MarketplaceTranslation.fillDescriptions`，
 * 并写回响应式缓存驱动卡面立即更新。
 */

export interface MarketTranslateBridge {
  descriptionsEnabled(): boolean;
  needsTranslation(text: string): boolean;
  lookup(source: string): string | null;
  blocked(): boolean;
  fillDescriptions(sources: string[]): Promise<{
    done: { source: string; text: string }[];
    rejection: { model: string; detail: string } | null;
  }>;
}

const nativeBridge: MarketTranslateBridge = {
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

export const marketTranslationCache = signal<Record<string, string>>({});
const prefsGeneration = signal(0);

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
    marketTranslationCache.set({});
    prefsGeneration.update((n) => n + 1);
  });
});

export function marketTranslatedText(source: string): string | null {
  void prefsGeneration();
  const trimmed = source.trim();
  if (trimmed.length === 0) return null;
  const cached = marketTranslationCache()[trimmed];
  return typeof cached === "string" && cached.length > 0 ? cached : null;
}

function recordTranslations(done: { source: string; text: string }[]): void {
  if (done.length === 0) return;
  const next = { ...marketTranslationCache() };
  let changed = false;
  for (const item of done) {
    const src = item.source.trim();
    const text = item.text.trim();
    if (src.length === 0 || text.length === 0 || next[src] === text) continue;
    next[src] = text;
    changed = true;
  }
  if (changed) {
    marketTranslationCache.set(next);
  }
}

function scheduleFlush(bridge: MarketTranslateBridge): void {
  if (flushTimer !== null || pendingQueue.size === 0) return;
  flushTimer = setTimeout(() => {
    flushTimer = null;
    void flushQueue(bridge);
  }, BATCH_DELAY_MS);
}

async function flushQueue(bridge: MarketTranslateBridge): Promise<void> {
  if (pendingQueue.size === 0 || sessionRejected) return;
  const runEpoch = prefsEpoch;
  const batch: string[] = [];
  for (const src of pendingQueue) {
    pendingQueue.delete(src);
    if (marketTranslationCache()[src] !== undefined || inflightSources.has(src)) continue;
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
    recordTranslations(report.done ?? []);
    if (report.rejection) {
      sessionRejected = true;
      for (const src of batch) rejectedSources.add(src);
    }
  } catch {
    // 后台排队失败保持静默，不中断用户浏览。
  } finally {
    if (runEpoch === prefsEpoch) {
      for (const src of batch) inflightSources.delete(src);
      if (pendingQueue.size > 0 && !sessionRejected) {
        scheduleFlush(bridge);
      }
    }
  }
}

export function queueMarketDescription(
  skill: { description: string; localizedDescription: string | null },
  bridge: MarketTranslateBridge = nativeBridge
): void {
  void prefsGeneration();
  if (typeof skill.localizedDescription === "string" && skill.localizedDescription.trim().length > 0) {
    return;
  }
  const source = skill.description.trim();
  if (source.length === 0) return;
  if (marketTranslationCache()[source] !== undefined) return;
  if (!locale().startsWith("zh") || !bridge.descriptionsEnabled()) return;

  const hit = bridge.lookup(source);
  if (hit && hit.trim().length > 0) {
    recordTranslations([{ source, text: hit }]);
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

export function resetMarketTranslateForTest(): void {
  prefsEpoch += 1;
  if (flushTimer !== null) {
    clearTimeout(flushTimer);
    flushTimer = null;
  }
  pendingQueue.clear();
  inflightSources.clear();
  rejectedSources.clear();
  sessionRejected = false;
  marketTranslationCache.set({});
}
