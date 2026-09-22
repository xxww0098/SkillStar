/**
 * Dev-mock fragment: the local decision model.
 *
 * Browser dev has no Rust side and certainly no 1.2 GB checkpoint, so the
 * status mock reports `missing` and inference answers with a fixed
 * distribution. That keeps the Settings panel renderable end to end without
 * pretending a model is installed.
 */

import type { DecisionEngineInfo, DecisionModelStatus, DecisionOutcome } from "../../../types";
import type { DevMockHandlers } from "./shared";

const STATUS: DecisionModelStatus = {
  dir: "~/.skillstar/models/agentjev-0.6b",
  endpoint: "https://huggingface.co",
  revision: "b3bf6b6dd443d6e724943b9194da31a4f055428e",
  state: "missing",
  presentBytes: 0,
  totalBytes: 1203924030,
  files: [
    { name: "model.safetensors", bytes: 1196881242, present: false },
    { name: "tokenizer.json", bytes: 7031645, present: false },
    { name: "config.json", bytes: 754, present: false },
    { name: "temperatures.json", bytes: 389, present: false },
  ],
};

const ENGINE: DecisionEngineInfo = {
  apiVersion: "agentjev.decision.v1",
  model: "AgentJev-0.6B",
  device: "metal",
  dtype: "f32",
  maxPathTokens: 2048,
  maxChoiceCandidates: 255,
  outputTokenDecoding: false,
  sharedPrefixCompute: true,
  temperatures: { boolean: 1.07, choice: 1.04, score: 1.07 },
};

const OUTCOME: DecisionOutcome = {
  apiVersion: "agentjev.decision.v1",
  model: "AgentJev-0.6B",
  results: [
    {
      id: "0",
      answers: [
        {
          id: "needs_review",
          kind: "boolean",
          distribution: [
            { key: "true", probability: 0.72 },
            { key: "false", probability: 0.28 },
          ],
          selectedKey: "true",
          selectedDescription: "TRUE",
          topProbability: 0.72,
          margin: 0.44,
          probabilityTrue: 0.72,
          score: null,
          level: null,
          levelDescriptions: [],
        },
      ],
    },
  ],
  usage: {
    questions: 1,
    candidatePaths: 2,
    inputPathTokens: 120,
    backboneInputTokens: 118,
    sharedPrefixQuestions: 1,
    generatedTokens: 0,
    truncatedInputs: 0,
    wallMs: 180,
  },
};

export const DECISION_HANDLERS: DevMockHandlers = {
  decision_model_status: () => STATUS,
  decision_verify_model: () => undefined,
  decision_download_model: () => undefined,
  decision_cancel_download: () => undefined,
  decision_engine_info: () => ENGINE,
  decision_load_engine: () => ENGINE,
  decision_unload_engine: () => undefined,
  decision_evaluate: () => OUTCOME,
};
