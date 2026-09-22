//! Golden test — the Rust runtime against the reference implementation.
//!
//! `agentjev_golden.json` was produced by running the official
//! `jev_service` engine (torch, CPU, f32) over six fixed payloads: it carries
//! the exact token ids of every candidate path, the per-question logits and
//! calibrated probabilities, and the reference's own answer objects. This test
//! re-runs the same payloads through the candle runtime and fails if the
//! tokenizer, the trunk, the candidate head or the temperatures drift.
//!
//! Ignored by default: it needs the 1.2 GB checkpoint. Run it with
//! `SKILLSTAR_DECISION_MODEL_DIR=<checkpoint dir> cargo test -p skillstar-decision \
//!  --profile release-fast --test golden -- --ignored --nocapture`

use std::collections::BTreeMap;

use serde_json::Value;
use skillstar_decision::{DecisionEngine, EngineOptions, ModelPaths};

/// Largest tolerated absolute probability difference.
///
/// The reference scores candidates in batches of 16 and the port scores them
/// one at a time, so f32 accumulation order differs by design. Anything above
/// this is a real numerical bug, not batching noise.
const PROBABILITY_TOLERANCE: f32 = 5e-3;

#[test]
#[ignore = "requires the 1.2 GB AgentJev checkpoint (set SKILLSTAR_DECISION_MODEL_DIR)"]
fn rust_runtime_matches_the_reference_implementation() {
    let dir = std::env::var("SKILLSTAR_DECISION_MODEL_DIR").unwrap_or_else(|_| {
        panic!(
            "set SKILLSTAR_DECISION_MODEL_DIR to a downloaded checkpoint directory \
             (the four files listed in MODEL_FILES)"
        )
    });
    let golden: Value =
        serde_json::from_str(include_str!("fixtures/agentjev_golden.json")).unwrap();
    let engine = DecisionEngine::load(&ModelPaths::at(&dir), EngineOptions::reference())
        .expect("load the checkpoint");
    println!("engine: {:?}", engine.info());

    let mut worst_probability = 0f32;
    let mut worst_tokens = String::new();
    let mut checked_paths = 0usize;

    for case in golden["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let payload = &case["payload"];

        // 1. Tokenizer parity, path by path, by (request, question, key).
        let paths = engine.encode_paths(payload).unwrap();
        let ours: BTreeMap<(String, String, String), Vec<u32>> = paths
            .iter()
            .map(|path| {
                (
                    (
                        path.request_id.clone(),
                        path.question_id.clone(),
                        path.key.clone(),
                    ),
                    path.tokens.clone(),
                )
            })
            .collect();

        let mut reference_index = 0usize;
        for question in case["questions"].as_array().unwrap() {
            let request_id = question["request_id"].as_str().unwrap();
            let question_id = question["id"].as_str().unwrap();
            let keys: Vec<&str> = question["keys"]
                .as_array()
                .unwrap()
                .iter()
                .map(|key| key.as_str().unwrap())
                .collect();
            for key in &keys {
                let expected: Vec<u32> = case["path_token_ids"][reference_index]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|token| token.as_u64().unwrap() as u32)
                    .collect();
                let actual = ours
                    .get(&(
                        request_id.to_string(),
                        question_id.to_string(),
                        (*key).to_string(),
                    ))
                    .unwrap_or_else(|| {
                        panic!("{name}: no tokenized path for {request_id}/{question_id}/{key}")
                    });
                if *actual != expected {
                    worst_tokens = format!(
                        "{name}:{question_id}:{key} — {} tokens vs {} expected",
                        actual.len(),
                        expected.len()
                    );
                }
                assert_eq!(
                    actual, &expected,
                    "{name}: token ids differ for {request_id}/{question_id}/{key}"
                );
                checked_paths += 1;
                reference_index += 1;
            }
        }

        // 2. Probability parity and the same winner, through the public API.
        let started = std::time::Instant::now();
        let outcome = engine.evaluate(payload).unwrap();
        println!(
            "{name}: {} ms (reference config)",
            started.elapsed().as_millis()
        );
        assert_eq!(
            outcome.usage.candidate_paths, reference_index,
            "{name}: candidate path count"
        );
        assert_eq!(outcome.usage.generated_tokens, 0);

        for question in case["questions"].as_array().unwrap() {
            let expected_keys = question["keys"].as_array().unwrap();
            let expected_probabilities = question["probabilities"].as_array().unwrap();

            // Align by key: object-form options are stored sorted, so index
            // order is not the reference's order.
            let ours_by_key: BTreeMap<&str, f32> = find_answer(&outcome, question)
                .distribution
                .iter()
                .map(|entry| (entry.key.as_str(), entry.probability))
                .collect();
            for (key, expected) in expected_keys.iter().zip(expected_probabilities) {
                let key = key.as_str().unwrap();
                let expected = expected.as_f64().unwrap() as f32;
                let actual = *ours_by_key
                    .get(key)
                    .unwrap_or_else(|| panic!("{name}: no probability for key {key}"));
                let delta = (actual - expected).abs();
                if delta > worst_probability {
                    worst_probability = delta;
                    println!("{name}:{key} ours={actual} reference={expected} delta={delta}");
                }
                assert!(
                    delta <= PROBABILITY_TOLERANCE,
                    "{name}: key {key} differs by {delta} (ours {actual}, reference {expected})"
                );
            }

            // The winner must match the reference's published answer.
            let expected_answer = &question["answer"];
            let expected_value = match question["type"].as_str().unwrap() {
                "boolean" => expected_answer["value"].as_bool().unwrap().to_string(),
                "choice" => expected_answer["value"].as_str().unwrap().to_string(),
                "score" => expected_answer["level"].as_u64().unwrap().to_string(),
                other => panic!("unexpected question type {other}"),
            };
            assert_eq!(
                find_answer(&outcome, question).selected_key,
                expected_value,
                "{name}: winner for {}",
                question["id"]
            );
        }
    }

    println!(
        "checked {checked_paths} token paths; worst probability delta {worst_probability}{}",
        if worst_tokens.is_empty() {
            String::new()
        } else {
            format!("; token mismatch: {worst_tokens}")
        }
    );

    // 3. Over-length input is refused with the reference's message, never cropped.
    let over = serde_json::json!({
        "state": "token ".repeat(4000),
        "questions": [{"id": "big", "type": "boolean", "question": "Is this long?"}],
    });
    let error = engine.evaluate(&over).unwrap_err().to_string();
    let expected = golden["over_length_error"]["message"].as_str().unwrap();
    assert_eq!(error, expected);
}

/// Find our answer for a golden question.
fn find_answer<'a>(
    outcome: &'a skillstar_decision::DecisionOutcome,
    question: &Value,
) -> &'a skillstar_decision::AnswerDto {
    let request_id = question["request_id"].as_str().unwrap();
    let question_id = question["id"].as_str().unwrap();
    let request = outcome
        .results
        .iter()
        .find(|request| request.id == request_id)
        .unwrap_or_else(|| panic!("no result for request {request_id}"));
    request
        .answers
        .iter()
        .find(|answer| answer.id == question_id)
        .unwrap_or_else(|| panic!("no answer for question {question_id}"))
}

/// The shipped configuration (Metal + f16 on macOS, CPU + f32 elsewhere) must
/// agree with the reference too — a fast path that silently changes answers is
/// worse than a slow one.
///
/// Tolerance is looser than the f32 test on purpose: f16 accumulation is real.
#[test]
#[ignore = "requires the 1.2 GB AgentJev checkpoint (set SKILLSTAR_DECISION_MODEL_DIR)"]
fn default_device_agrees_with_the_reference() {
    let dir = std::env::var("SKILLSTAR_DECISION_MODEL_DIR")
        .expect("set SKILLSTAR_DECISION_MODEL_DIR to a downloaded checkpoint directory");
    let golden: Value =
        serde_json::from_str(include_str!("fixtures/agentjev_golden.json")).unwrap();
    let engine = DecisionEngine::load(&ModelPaths::at(&dir), EngineOptions::default())
        .expect("load the checkpoint");
    println!("engine: {:?}", engine.info());

    let mut worst = 0f32;
    for case in golden["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let payload = &case["payload"];
        let started = std::time::Instant::now();
        let outcome = engine.evaluate(payload).unwrap();
        println!("{name}: {} ms", started.elapsed().as_millis());

        for question in case["questions"].as_array().unwrap() {
            let ours = find_answer(&outcome, question);
            let by_key: BTreeMap<&str, f32> = ours
                .distribution
                .iter()
                .map(|entry| (entry.key.as_str(), entry.probability))
                .collect();
            for (key, expected) in question["keys"]
                .as_array()
                .unwrap()
                .iter()
                .zip(question["probabilities"].as_array().unwrap())
            {
                let key = key.as_str().unwrap();
                let delta = (by_key[key] - expected.as_f64().unwrap() as f32).abs();
                worst = worst.max(delta);
                assert!(
                    delta <= 2e-2,
                    "{name}: key {key} differs by {delta} on the default device"
                );
            }
            let expected_value = match question["type"].as_str().unwrap() {
                "boolean" => question["answer"]["value"].as_bool().unwrap().to_string(),
                "choice" => question["answer"]["value"].as_str().unwrap().to_string(),
                _ => question["answer"]["level"].as_u64().unwrap().to_string(),
            };
            assert_eq!(
                ours.selected_key, expected_value,
                "{name}: winner for {} on the default device",
                question["id"]
            );
        }
    }
    println!("worst probability delta on the default device: {worst}");
}
