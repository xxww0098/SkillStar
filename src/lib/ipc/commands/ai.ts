import type { AiConfig, AiPickResponse, AiTranslateSkillStreamResult } from "../../../types";

interface SkillMetaInput {
  name: string;
  description: string;
}

/** AI provider config, one-shot + streaming AI operations. */
export interface AiCommands {
  get_ai_config: { args: Record<string, never>; result: AiConfig };
  save_ai_config: { args: { config: AiConfig }; result: void };

  ai_summarize_skill: { args: { content: string }; result: string };
  ai_summarize_skill_stream: {
    args: { requestId: string; content: string; forceRefresh?: boolean };
    result: string;
  };
  ai_translate_skill: { args: { content: string }; result: string };
  ai_translate_skill_stream: {
    args: { requestId: string; content: string; forceRefresh?: boolean };
    result: AiTranslateSkillStreamResult;
  };

  ai_test_connection: { args: Record<string, never>; result: number };
  ai_pick_skills: {
    args: { prompt: string; skills: SkillMetaInput[] };
    result: AiPickResponse;
  };
}
