import type { DecisionEngineInfo, DecisionModelStatus, DecisionOutcome } from "../../../types";

/** Local decision model (AgentJev-0.6B): checkpoint state, download, inference. */
export interface DecisionCommands {
  decision_model_status: { args: Record<string, never>; result: DecisionModelStatus };
  decision_verify_model: { args: Record<string, never>; result: void };
  decision_download_model: { args: Record<string, never>; result: void };
  decision_cancel_download: { args: Record<string, never>; result: void };
  decision_engine_info: { args: Record<string, never>; result: DecisionEngineInfo | null };
  decision_load_engine: { args: Record<string, never>; result: DecisionEngineInfo };
  decision_unload_engine: { args: Record<string, never>; result: void };
  decision_evaluate: { args: { payload: unknown }; result: DecisionOutcome };
}
