//! ai domain types. Split out of the old monolithic index for
//! navigability; all re-exported by `index.ts`.

import type { Skill } from "./skill";

export type AiStreamEvent = "start" | "delta" | "complete" | "error";

export interface AiKeywordSearchResult {
  skills: Skill[];
  total_count: number;
  /** Maps each keyword to the skill names it found */
  keyword_skill_map: Record<string, string[]>;
}

export interface AiConfigStatus {
  enabled: boolean;
  api_key: string;
}

export interface AiPickRecommendation {
  name: string;
  score: number;
  reason: string;
}

export interface AiPickResponse {
  recommendations: AiPickRecommendation[];
  fallbackUsed: boolean;
  roundsSucceeded: number;
}

export interface AiStreamPayload {
  requestId: string;
  event: AiStreamEvent;
  delta?: string | null;
  message?: string | null;
  providerId?: string | null;
}

/** Phases of the AST-based translation pipeline reported by the backend. */

export type AiTranslatePipelinePhase = "prepare" | "translate" | "finalize" | "guard";

/** Per-event bundle progress reported on `ai://translate-stream`. */

export interface AiTranslatePipelineProgress {
  phase: AiTranslatePipelinePhase;
  current: number;
  total: number;
}

/** Translation model speed and usage reported when SKILL.md translation completes. */

export interface AiTranslateMetrics {
  model: string;
  targetLanguage: string;
  elapsedMs: number;
  inputChars: number;
  outputChars: number;
  promptTokens?: number | null;
  completionTokens?: number | null;
  totalTokens?: number | null;
  tps?: number | null;
  cacheHit: boolean;
  modelCalls: number;
}

export interface AiTranslateSkillStreamResult {
  content: string;
  metrics: AiTranslateMetrics;
}

/** Payload emitted on the `ai://translate-stream` Tauri event. */

export interface AiTranslateStreamPayload {
  requestId: string;
  event: "start" | "progress" | "complete" | "error";
  pipelineProgress?: AiTranslatePipelineProgress | null;
  metrics?: AiTranslateMetrics | null;
  message?: string | null;
}

export interface AiProviderRef {
  app_id: string;
  provider_id: string;
}

export interface FormatPreset {
  base_url: string;
  api_key: string;
  model: string;
}

export interface AiConfig {
  enabled: boolean;
  api_format: "openai" | "anthropic" | "local";
  provider_ref: AiProviderRef | null;
  base_url: string;
  api_key: string;
  model: string;
  target_language: string;
  /** Model context window in K tokens (e.g. 128 = 128K tokens) */
  context_window_k: number;
  max_concurrent_requests: number;
  /** Per-format saved presets */
  openai_preset: FormatPreset;
  anthropic_preset: FormatPreset;
  local_preset: FormatPreset;
}

// ── GitHub Repo Scanner ─────────────────────────────────────────────
