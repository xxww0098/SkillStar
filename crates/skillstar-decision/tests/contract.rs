//! Contract tests — no checkpoint needed.
//!
//! These lock the request/answer surface (validation order, message text,
//! answer shaping) and the model-directory bookkeeping, so a regression there
//! fails in CI instead of at a user's first question.

use std::path::Path;

use serde_json::json;
use skillstar_decision::{
    answer, prepare, DecisionAnswer, ModelPaths, ModelState, QuestionKind, MODEL_FILES,
};
use tempfile::TempDir;

fn prepare_error(payload: &serde_json::Value) -> String {
    prepare(payload).expect_err("payload should be rejected").to_string()
}

#[test]
fn rejects_non_object_payloads() {
    assert_eq!(
        prepare_error(&json!([1, 2, 3])),
        "request must be an object"
    );
}

#[test]
fn rejects_missing_state() {
    assert_eq!(
        prepare_error(&json!({"questions": [{"type": "boolean", "question": "q"}]})),
        "state must be nonempty text, an object or an array"
    );
}

#[test]
fn rejects_blank_state_and_empty_questions() {
    assert_eq!(
        prepare_error(&json!({"state": "   ", "questions": []})),
        "state must be nonempty text, an object or an array"
    );
    assert_eq!(
        prepare_error(&json!({"state": "a ticket", "questions": []})),
        "questions must be a nonempty array"
    );
}

#[test]
fn rejects_duplicate_question_ids() {
    let message = prepare_error(&json!({
        "state": "ticket",
        "questions": [
            {"id": "same", "type": "boolean", "question": "a"},
            {"id": "same", "type": "boolean", "question": "b"},
        ],
    }));
    assert_eq!(
        message,
        "question IDs must be unique nonempty strings within a state"
    );
}

#[test]
fn ids_default_to_the_question_index() {
    let prepared = prepare(&json!({
        "state": "ticket",
        "questions": [
            {"type": "boolean", "question": "a"},
            {"type": "boolean", "question": "b"},
        ],
    }))
    .unwrap();
    assert_eq!(prepared[0].id, "0");
    assert_eq!(prepared[0].questions[0].id, "0");
    assert_eq!(prepared[0].questions[1].id, "1");
}

#[test]
fn rejects_unknown_type_and_bad_option_counts() {
    assert_eq!(
        prepare_error(&json!({
            "state": "ticket",
            "questions": [{"type": "noul", "question": "a"}],
        })),
        "type must be boolean, choice or score"
    );
    assert_eq!(
        prepare_error(&json!({
            "state": "ticket",
            "questions": [{"type": "choice", "question": "a", "options": ["only"]}],
        })),
        "choice requires 2..255 candidates"
    );
    assert_eq!(
        prepare_error(&json!({
            "state": "ticket",
            "questions": [{"type": "score", "question": "a", "levels": ["one"]}],
        })),
        "score.levels must contain 2..10 ordered descriptions"
    );
}

#[test]
fn rejects_duplicate_candidate_text() {
    assert_eq!(
        prepare_error(&json!({
            "state": "ticket",
            "questions": [{
                "type": "choice",
                "question": "a",
                "options": {"x": "same", "y": "same"},
            }],
        })),
        "candidate descriptions must be distinct"
    );
}

#[test]
fn boolean_candidates_default_to_upper_case_and_accept_criteria() {
    let prepared = prepare(&json!({
        "state": "ticket",
        "questions": [
            {"id": "plain", "type": "boolean", "question": "a"},
            {
                "id": "described",
                "type": "boolean",
                "question": "b",
                "criteria": {"true": "the message asks for money back"},
            },
        ],
    }))
    .unwrap();
    assert_eq!(
        prepared[0].questions[0].candidates,
        vec!["TRUE".to_string(), "FALSE".to_string()]
    );
    assert_eq!(prepared[0].questions[0].keys, vec!["true", "false"]);
    assert_eq!(
        prepared[0].questions[1].candidates,
        vec!["the message asks for money back".to_string(), "FALSE".to_string()]
    );
}

#[test]
fn structured_state_is_serialized_compactly_with_sorted_keys() {
    let prepared = prepare(&json!({
        "state": {"b": 1, "a": [1, 2]},
        "questions": [{"type": "boolean", "question": "a"}],
    }))
    .unwrap();
    assert_eq!(prepared[0].state, r#"{"a":[1,2],"b":1}"#);
}

#[test]
fn structured_candidates_are_serialized_too() {
    let prepared = prepare(&json!({
        "state": "ticket",
        "questions": [{
            "type": "choice",
            "question": "which?",
            "options": [{"kind": "refund"}, {"kind": "cancel"}],
        }],
    }))
    .unwrap();
    assert_eq!(
        prepared[0].questions[0].candidates,
        vec![r#"{"kind":"refund"}"#.to_string(), r#"{"kind":"cancel"}"#.to_string()]
    );
}

#[test]
fn question_text_accepts_the_instructions_alias() {
    let prepared = prepare(&json!({
        "state": "ticket",
        "questions": [{"type": "boolean", "instructions": "Is it urgent?"}],
    }))
    .unwrap();
    assert_eq!(prepared[0].questions[0].text, "Is it urgent?");
}

#[test]
fn batch_is_capped_at_32_states() {
    let requests: Vec<serde_json::Value> = (0..33)
        .map(|index| {
            json!({
                "id": index.to_string(),
                "state": "ticket",
                "questions": [{"type": "boolean", "question": "a"}],
            })
        })
        .collect();
    assert_eq!(
        prepare_error(&json!({"requests": requests})),
        "requests must contain 1..32 states"
    );
}

#[test]
fn batch_is_capped_at_128_questions_and_1024_paths() {
    let questions: Vec<serde_json::Value> = (0..129)
        .map(|index| json!({"id": index.to_string(), "type": "boolean", "question": "a"}))
        .collect();
    assert_eq!(
        prepare_error(&json!({"state": "ticket", "questions": questions})),
        "batch exceeds 128 questions or 1024 candidate paths"
    );

    let options: Vec<String> = (0..255).map(|index| format!("option {index}")).collect();
    let questions: Vec<serde_json::Value> = (0..5)
        .map(|index| {
            json!({
                "id": index.to_string(),
                "type": "choice",
                "question": "pick",
                "options": options,
            })
        })
        .collect();
    assert_eq!(
        prepare_error(&json!({"state": "ticket", "questions": questions})),
        "batch exceeds 128 questions or 1024 candidate paths"
    );
}

#[test]
fn answers_shape_each_primitive() {
    let prepared = prepare(&json!({
        "state": "ticket",
        "questions": [
            {"id": "b", "type": "boolean", "question": "a"},
            {
                "id": "c",
                "type": "choice",
                "question": "b",
                "options": ["first", "second", "third"],
            },
            {"id": "s", "type": "score", "question": "c", "levels": ["low", "mid", "high"]},
        ],
    }))
    .unwrap();
    let questions = &prepared[0].questions;

    let boolean = answer(&questions[0], &[0.7, 0.3]).unwrap();
    match boolean {
        DecisionAnswer::Boolean { probability, value, .. } => {
            assert!((probability - 0.7).abs() < 1e-6);
            assert!(value);
        }
        other => panic!("expected a boolean answer, got {other:?}"),
    }

    let choice = answer(&questions[1], &[0.2, 0.5, 0.3]).unwrap();
    match choice {
        DecisionAnswer::Choice {
            value,
            description,
            top_probability,
            margin,
            ..
        } => {
            assert_eq!(value, "1");
            assert_eq!(description, "second");
            assert!((top_probability - 0.5).abs() < 1e-6);
            assert!((margin - 0.2).abs() < 1e-6);
        }
        other => panic!("expected a choice answer, got {other:?}"),
    }

    let score = answer(&questions[2], &[0.1, 0.2, 0.7]).unwrap();
    match score {
        DecisionAnswer::Score { score, level, legend, .. } => {
            assert!((score - 1.6).abs() < 1e-6);
            assert_eq!(level, 2);
            assert_eq!(legend, vec!["low", "mid", "high"]);
        }
        other => panic!("expected a score answer, got {other:?}"),
    }
}

#[test]
fn answers_reject_a_broken_distribution() {
    let prepared = prepare(&json!({
        "state": "ticket",
        "questions": [{"id": "b", "type": "boolean", "question": "a"}],
    }))
    .unwrap();
    let question = &prepared[0].questions[0];

    assert_eq!(
        answer(question, &[0.7, 0.4]).unwrap_err().to_string(),
        "decision model inference failed: model distribution does not sum to one"
    );
    assert_eq!(
        answer(question, &[f32::NAN, 1.0]).unwrap_err().to_string(),
        "decision model inference failed: invalid model distribution"
    );
    assert_eq!(
        answer(question, &[-0.1, 1.1]).unwrap_err().to_string(),
        "decision model inference failed: invalid model distribution"
    );
}

#[test]
fn kinds_round_trip_through_their_wire_names() {
    for (kind, name) in [
        (QuestionKind::Boolean, "boolean"),
        (QuestionKind::Choice, "choice"),
        (QuestionKind::Score, "score"),
    ] {
        assert_eq!(kind.as_str(), name);
        assert_eq!(serde_json::to_value(kind).unwrap(), json!(name));
        assert_eq!(serde_json::from_value::<QuestionKind>(json!(name)).unwrap(), kind);
    }
}

/// The Tauri adapter keeps one engine in shared state, so the engine must be
/// usable from any thread. This is a compile-time property; asserting it here
/// keeps a future non-`Sync` tensor backend from surfacing as an adapter error.
#[test]
fn the_engine_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<skillstar_decision::DecisionEngine>();
}

#[test]
fn model_dir_status_walks_missing_partial_ready() {
    let temp = TempDir::new().unwrap();
    let paths = ModelPaths::at(temp.path());
    assert_eq!(paths.status().state, ModelState::Missing);
    assert_eq!(paths.status().present_bytes, 0);

    // A file at the wrong size does not count as present.
    std::fs::write(paths.weights(), b"not the checkpoint").unwrap();
    assert_eq!(paths.status().state, ModelState::Missing);

    // Sparse files of the exact expected size are enough for the cheap status
    // walk; digests are checked separately (and would fail here).
    for spec in MODEL_FILES {
        let file = std::fs::File::create(paths.file(spec.name)).unwrap();
        file.set_len(spec.bytes).unwrap();
    }
    let status = paths.status();
    assert_eq!(status.state, ModelState::Ready);
    assert_eq!(status.present_bytes, status.total_bytes);
    assert!(status.files.iter().all(|file| file.present));

    let verified = paths.verify().expect_err("sparse files cannot pass digests");
    assert!(verified.to_string().contains("failed its SHA-256 check"));
}

#[test]
fn model_dir_respects_the_data_dir_override() {
    let temp = TempDir::new().unwrap();
    // SAFETY: test-local env mutation; the workspace runs these serially enough
    // because nothing else in this binary reads SKILLSTAR_* concurrently.
    unsafe {
        std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        std::env::remove_var("SKILLSTAR_DECISION_MODEL_DIR");
    }
    assert_eq!(
        ModelPaths::resolve().dir(),
        temp.path().join("models").join("agentjev-0.6b")
    );
    unsafe {
        std::env::set_var("SKILLSTAR_DECISION_MODEL_DIR", temp.path().join("explicit"));
    }
    assert_eq!(ModelPaths::resolve().dir(), temp.path().join("explicit"));
    unsafe {
        std::env::remove_var("SKILLSTAR_DECISION_MODEL_DIR");
        std::env::remove_var("SKILLSTAR_DATA_DIR");
    }
}

#[test]
fn temperatures_reject_an_out_of_range_scalar() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("temperatures.json");
    std::fs::write(&path, r#"{"choice": {"temperature": 99.0}}"#).unwrap();
    assert_eq!(
        skillstar_decision::Temperatures::load(&path)
            .unwrap_err()
            .to_string(),
        "Invalid calibration temperature"
    );

    std::fs::write(
        &path,
        r#"{"boolean": {"temperature": 1.2}, "choice": {"temperature": 1.0}}"#,
    )
    .unwrap();
    let loaded = skillstar_decision::Temperatures::load(&path).unwrap();
    assert_eq!(loaded.boolean(), 1.2);
    assert_eq!(loaded.choice(), 1.0);
    assert_eq!(loaded.score(), 1.0, "absent kinds default to 1.0");
}

#[test]
fn checkpoint_paths_are_derived_from_the_directory() {
    let paths = ModelPaths::at(Path::new("/tmp/agentjev"));
    assert_eq!(paths.weights(), Path::new("/tmp/agentjev/model.safetensors"));
    assert_eq!(paths.tokenizer(), Path::new("/tmp/agentjev/tokenizer.json"));
    assert_eq!(paths.config(), Path::new("/tmp/agentjev/config.json"));
    assert_eq!(
        paths.temperatures(),
        Path::new("/tmp/agentjev/temperatures.json")
    );
    assert!(skillstar_decision::file_url("config.json").ends_with(
        "aimeigaoshou/agent-jev/resolve/b3bf6b6dd443d6e724943b9194da31a4f055428e/config.json"
    ));
}
