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

export interface AiProviderRef {
  /**
   * An id from the agent registry (`claude-code`, `codex`). v3 called this
   * `app_id` and used its own two-value id space (`claude` / `codex`); the
   * backend still accepts the old spelling and maps it forward.
   */
  agent_id: string;
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
