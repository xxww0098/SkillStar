//! CPU-only ONNX Runtime. A missing or refused Laya bundle keeps the BM25 order.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ndarray::{Array1, Array2};
use ort::ep::CPU;
use ort::session::Session;
use ort::value::TensorRef;
use tokenizers::Tokenizer;

use super::laya_pack::{
    self, Calibration, PackedNoul, SpecialIds, build_noul, bundle_is_complete,
    configured_bundle_dir, language_allows, noul_probability, order_by_score, read_calibration,
    skill_instruction,
};
use super::ranker::{RankedCandidate, SkillReranker};

pub struct OrtCpuReranker;

struct LoadedBundle {
    session: Session,
    tokenizer: Tokenizer,
    special: SpecialIds,
    calibration: Calibration,
}

struct Cache {
    dir: PathBuf,
    bundle: Option<LoadedBundle>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

impl SkillReranker for OrtCpuReranker {
    fn name_for(&self, task: &str) -> &'static str {
        let Some(dir) = configured_bundle_dir() else {
            return "passthrough";
        };
        if usable_calibration(task).is_none() {
            return "passthrough";
        }
        let cache = CACHE.lock().unwrap_or_else(|err| err.into_inner());
        match cache.as_ref() {
            Some(entry) if entry.dir == dir && entry.bundle.is_none() => "passthrough",
            _ => "ort-cpu",
        }
    }

    fn rerank(&self, task: &str, mut candidates: Vec<RankedCandidate>) -> Vec<RankedCandidate> {
        let Some(calibration) = usable_calibration(task) else {
            return candidates;
        };
        let Some(dir) = configured_bundle_dir() else {
            return candidates;
        };
        let mut cache = CACHE.lock().unwrap_or_else(|err| err.into_inner());
        let Some(bundle) = ensure_loaded(&mut cache, &dir, calibration) else {
            return candidates;
        };
        let Some(scores) = score_candidates(bundle, task, &candidates) else {
            return candidates;
        };
        for (candidate, score) in candidates.iter_mut().zip(scores) {
            candidate.score = score;
        }
        order_by_score(&mut candidates);
        candidates
    }
}

pub fn active_reranker() -> OrtCpuReranker {
    OrtCpuReranker
}

pub fn registered_execution_provider_names() -> &'static [&'static str] {
    &["CPUExecutionProvider"]
}

pub fn open_cpu_session(model: &[u8]) -> ort::Result<Session> {
    let mut builder = Session::builder()?.with_execution_providers([CPU::default().build()])?;
    builder.commit_from_memory(model)
}

pub fn open_cpu_session_file(path: &Path) -> ort::Result<Session> {
    let mut builder = Session::builder()?.with_execution_providers([CPU::default().build()])?;
    builder.commit_from_file(path)
}

fn usable_calibration(task: &str) -> Option<Calibration> {
    let dir = configured_bundle_dir()?;
    if !bundle_is_complete(&dir) {
        return None;
    }
    let calibration = read_calibration(&dir)?;
    language_allows(task, calibration.language).then_some(calibration)
}

fn ensure_loaded<'a>(
    cache: &'a mut Option<Cache>,
    dir: &Path,
    calibration: Calibration,
) -> Option<&'a mut LoadedBundle> {
    if cache.as_ref().is_some_and(|existing| existing.dir == dir) {
        return cache.as_mut().and_then(|existing| existing.bundle.as_mut());
    }
    let bundle = load_bundle(dir, calibration);
    *cache = Some(Cache {
        dir: dir.to_path_buf(),
        bundle,
    });
    cache.as_mut().and_then(|entry| entry.bundle.as_mut())
}

fn load_bundle(dir: &Path, calibration: Calibration) -> Option<LoadedBundle> {
    match try_load_bundle(dir, calibration) {
        Ok(bundle) => Some(bundle),
        Err(err) => {
            tracing::debug!("laya bundle did not load; keeping bm25 order: {err}");
            None
        }
    }
}

fn try_load_bundle(dir: &Path, calibration: Calibration) -> Result<LoadedBundle, String> {
    let mut tokenizer = Tokenizer::from_file(dir.join("tokenizer").join("tokenizer.json"))
        .map_err(|err| err.to_string())?;
    tokenizer
        .with_truncation(None)
        .map_err(|err| err.to_string())?;
    tokenizer.with_padding(None);
    let special = SpecialIds {
        cls: tokenizer
            .token_to_id(&calibration.cls_token)
            .ok_or_else(|| format!("missing {}", calibration.cls_token))?,
        sep: tokenizer
            .token_to_id(&calibration.sep_token)
            .ok_or_else(|| format!("missing {}", calibration.sep_token))?,
        mask: tokenizer
            .token_to_id(&calibration.mask_token)
            .ok_or_else(|| format!("missing {}", calibration.mask_token))?,
        pad: tokenizer
            .token_to_id(&calibration.pad_token)
            .ok_or_else(|| format!("missing {}", calibration.pad_token))?,
        mask_token: calibration.mask_token.clone(),
    };
    let session = open_cpu_session_file(&dir.join("laya.onnx")).map_err(|err| err.to_string())?;
    Ok(LoadedBundle {
        session,
        tokenizer,
        special,
        calibration,
    })
}

fn score_candidates(
    bundle: &mut LoadedBundle,
    task: &str,
    candidates: &[RankedCandidate],
) -> Option<Vec<f32>> {
    let n = candidates.len().min(laya_pack::MAX_SCORED);
    if n == 0 {
        return Some(Vec::new());
    }
    let mut packed = Vec::with_capacity(n);
    for candidate in &candidates[..n] {
        packed.push(build_noul(
            |text| encode_ids(&bundle.tokenizer, text),
            &bundle.special,
            &skill_instruction(&candidate.name, &candidate.description),
            task,
            bundle.calibration.max_len,
            bundle.calibration.head_max_len,
        )?);
    }
    let logits = raw_noul_logits(&mut bundle.session, &packed, bundle.special.pad)?;
    let mut scores = Vec::with_capacity(n);
    for row in 0..n {
        scores.push(noul_probability(
            logits[row * 2],
            logits[row * 2 + 1],
            bundle.calibration.noul_temperature,
        )?);
    }
    Some(scores)
}

fn logits_for_instruction(instruction: &str, task: &str) -> Option<(Vec<i64>, [i64; 2], Vec<f32>)> {
    let dir = configured_bundle_dir()?;
    let calibration = read_calibration(&dir)?;
    let mut cache = CACHE.lock().unwrap_or_else(|err| err.into_inner());
    let bundle = ensure_loaded(&mut cache, &dir, calibration)?;
    let packed = build_noul(
        |text| encode_ids(&bundle.tokenizer, text),
        &bundle.special,
        instruction,
        task,
        bundle.calibration.max_len,
        bundle.calibration.head_max_len,
    )?;
    let logits = raw_noul_logits(&mut bundle.session, &[packed.clone()], bundle.special.pad)?;
    Some((packed.ids, packed.markers, logits))
}

fn encode_ids(tokenizer: &Tokenizer, text: &str) -> Option<Vec<u32>> {
    tokenizer
        .encode(text, false)
        .ok()
        .map(|encoding| encoding.get_ids().to_vec())
}

pub(crate) fn raw_noul_logits(
    session: &mut Session,
    packed: &[PackedNoul],
    pad: u32,
) -> Option<Vec<f32>> {
    let n = packed.len();
    let width = packed.iter().map(|item| item.ids.len()).max()?;
    let mut input_ids = Array2::<i64>::from_elem((n, width), i64::from(pad));
    let mut attention = Array2::<i64>::zeros((n, width));
    let mut marker_pos = Array2::<i64>::zeros((n, 2));
    let mut marker_mask = Array2::<bool>::from_elem((n, 2), false);
    let qtype = Array1::<i64>::from_elem(n, 2_i64);
    for (row, item) in packed.iter().enumerate() {
        for (col, id) in item.ids.iter().enumerate() {
            input_ids[[row, col]] = *id;
            attention[[row, col]] = 1;
        }
        for (col, marker) in item.markers.iter().enumerate() {
            marker_pos[[row, col]] = *marker;
            marker_mask[[row, col]] = true;
        }
    }
    let outputs = session
        .run(ort::inputs![
            "input_ids" => TensorRef::from_array_view(&input_ids).ok()?,
            "attention_mask" => TensorRef::from_array_view(&attention).ok()?,
            "marker_pos" => TensorRef::from_array_view(&marker_pos).ok()?,
            "marker_mask" => TensorRef::from_array_view(&marker_mask).ok()?,
            "qtype" => TensorRef::from_array_view(&qtype).ok()?,
        ])
        .ok()?;
    let (_shape, values) = outputs["logits"].try_extract_tensor::<f32>().ok()?;
    if values.len() < n * 2 {
        return None;
    }
    Some(values.iter().copied().take(n * 2).collect())
}

#[cfg(test)]
mod ort_cpu_tests {
    use super::{OrtCpuReranker, open_cpu_session, registered_execution_provider_names};
    use crate::project_skills_mcp::plan::{PlanAction, PlanDraft, PlanSkill, create_plan};
    use crate::project_skills_mcp::ranker::{PassthroughReranker, RankedCandidate, SkillReranker};
    use crate::project_skills_mcp::recommend::{RecommendRequest, recommend_project_skills};
    use chrono::{TimeZone, Utc};
    use ort::ep::{CPU, ExecutionProvider};
    use ort::value::TensorRef;
    use std::path::PathBuf;

    #[test]
    fn ort_cpu_session_runs_the_checked_in_identity_graph() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ort-cpu-identity.onnx");
        let bytes = std::fs::read(&path).unwrap();
        let mut session = open_cpu_session(&bytes).unwrap();
        let input = ndarray::array![1.5_f32, -2.0];
        let outputs = session
            .run(ort::inputs![TensorRef::from_array_view(&input).unwrap()])
            .unwrap();
        let (_shape, values) = outputs[0].try_extract_tensor::<f32>().unwrap();
        assert_eq!(values, &[1.5, -2.0]);
    }

    #[test]
    fn missing_laya_onnx_keeps_bm25_order_and_recommend_succeeds() {
        let _lock = futures::executor::block_on(crate::test_support::ENV_LOCK.lock());
        let previous = std::env::var_os("SKILLSTAR_LAYA_ONNX");
        unsafe { std::env::remove_var("SKILLSTAR_LAYA_ONNX") };
        let candidates = vec![
            RankedCandidate {
                name: "a".into(),
                description: "alpha".into(),
                score: 0.2,
            },
            RankedCandidate {
                name: "b".into(),
                description: "beta".into(),
                score: 0.9,
            },
        ];
        let ranked = OrtCpuReranker.rerank("demo", candidates.clone());
        assert_eq!(ranked, PassthroughReranker.rerank("demo", candidates));

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("skillstar-ort-recommend-{nanos}"));
        std::fs::create_dir_all(root.join("home")).unwrap();
        std::fs::create_dir_all(root.join("data")).unwrap();
        let project = root.join("project");
        std::fs::create_dir_all(&project).unwrap();
        let previous_home = std::env::var_os("HOME");
        let previous_data = std::env::var_os("SKILLSTAR_DATA_DIR");
        unsafe {
            std::env::set_var("HOME", root.join("home"));
            std::env::set_var("SKILLSTAR_DATA_DIR", root.join("data"));
        }
        let result = recommend_project_skills(
            RecommendRequest {
                project_path: std::fs::canonicalize(&project)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                query: "demo".into(),
                constraints: Vec::new(),
                focus_paths: Vec::new(),
                selection: None,
                catalog_scope: "installed".into(),
            },
            &OrtCpuReranker,
            Utc.with_ymd_and_hms(2026, 9, 22, 10, 0, 0).unwrap(),
        );
        unsafe {
            restore("HOME", previous_home);
            restore("SKILLSTAR_DATA_DIR", previous_data);
            restore("SKILLSTAR_LAYA_ONNX", previous);
        }
        let _ = std::fs::remove_dir_all(&root);
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn ort_ranker_does_not_change_plan_hash_inputs() {
        let now = Utc.with_ymd_and_hms(2026, 9, 22, 10, 0, 0).unwrap();
        let mut draft = PlanDraft {
            root: PathBuf::from("/tmp/demo"),
            will_register: true,
            agent_ids: vec!["codex".into()],
            skills: vec![PlanSkill {
                name: "demo".into(),
                content_hash: "hash-demo".into(),
                action: PlanAction::Create,
            }],
            physical_rel: ".agents/skills".into(),
            owner_id: "codex".into(),
            affected_agents: vec!["codex".into()],
            scores: vec![0.4],
            reranker: "passthrough".into(),
        };
        // create_plan writes a file. Keep it out of the real home.
        let _lock = futures::executor::block_on(crate::test_support::ENV_LOCK.lock());
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("skillstar-ort-hash-{nanos}"));
        std::fs::create_dir_all(root.join("home")).unwrap();
        std::fs::create_dir_all(root.join("data")).unwrap();
        let previous_home = std::env::var_os("HOME");
        let previous_data = std::env::var_os("SKILLSTAR_DATA_DIR");
        unsafe {
            std::env::set_var("HOME", root.join("home"));
            std::env::set_var("SKILLSTAR_DATA_DIR", root.join("data"));
        }
        let left = create_plan(draft.clone(), now).unwrap().unwrap();
        draft.reranker = "ort-cpu".into();
        draft.scores = vec![0.1];
        let right = create_plan(draft, now).unwrap().unwrap();
        unsafe {
            restore("HOME", previous_home);
            restore("SKILLSTAR_DATA_DIR", previous_data);
        }
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(left.plan_hash, right.plan_hash);
        assert_ne!(left.reranker, right.reranker);
    }

    #[test]
    fn session_builder_registers_only_the_cpu_execution_provider() {
        assert_eq!(CPU::default().name(), "CPUExecutionProvider");
        assert_eq!(
            registered_execution_provider_names(),
            [CPU::default().name()]
        );
        for forbidden in [
            "CUDAExecutionProvider",
            "CoreMLExecutionProvider",
            "DmlExecutionProvider",
            "TensorrtExecutionProvider",
        ] {
            assert!(!registered_execution_provider_names().contains(&forbidden));
        }
    }

    #[test]
    fn english_bundle_keeps_bm25_order_for_a_chinese_task() {
        let _lock = futures::executor::block_on(crate::test_support::ENV_LOCK.lock());
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("skillstar-laya-gate-{nanos}"));
        std::fs::create_dir_all(root.join("tokenizer")).unwrap();
        std::fs::write(root.join("laya.onnx"), b"not a model").unwrap();
        std::fs::write(root.join("laya.onnx.data"), b"not weights").unwrap();
        std::fs::write(
            root.join("laya_config.json"),
            r#"{"max_len":512,"head_max_len":192,"temperature":[1.0,1.0,1.9]}"#,
        )
        .unwrap();
        std::fs::write(root.join("tokenizer").join("tokenizer.json"), b"{}").unwrap();
        std::fs::write(
            root.join("tokenizer").join("tokenizer_config.json"),
            r#"{"cls_token":"[CLS]","sep_token":"[SEP]","mask_token":"[MASK]","pad_token":"[PAD]"}"#,
        )
        .unwrap();
        let previous = std::env::var_os("SKILLSTAR_LAYA_ONNX");
        unsafe { std::env::set_var("SKILLSTAR_LAYA_ONNX", &root) };
        let candidates = vec![
            RankedCandidate {
                name: "deploy-k8s".into(),
                description: "Deploy applications to Kubernetes.".into(),
                score: 0.9,
            },
            RankedCandidate {
                name: "refund-helper".into(),
                description: "Handle payment refunds and invoices.".into(),
                score: 0.1,
            },
        ];
        let chinese = OrtCpuReranker.rerank("客户要退重复扣款", candidates.clone());
        let chinese_name = OrtCpuReranker.name_for("客户要退重复扣款");
        let english_name = OrtCpuReranker.name_for("Refund the duplicate invoice.");
        let english = OrtCpuReranker.rerank("Refund the duplicate invoice.", candidates.clone());
        let english_name_after = OrtCpuReranker.name_for("Refund the duplicate invoice.");
        unsafe { restore("SKILLSTAR_LAYA_ONNX", previous) };
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(chinese_name, "passthrough");
        assert_eq!(chinese, candidates);
        assert_eq!(english_name, "ort-cpu");
        assert_eq!(english_name_after, "passthrough");
        assert_eq!(english, candidates);
    }

    #[test]
    #[ignore = "requires SKILLSTAR_LAYA_ONNX"]
    fn laya_graph_reranks_without_changing_the_candidate_set() {
        let candidates = vec![
            RankedCandidate {
                name: "deploy-k8s".into(),
                description: "Deploy applications to Kubernetes clusters.".into(),
                score: 0.9,
            },
            RankedCandidate {
                name: "refund-helper".into(),
                description: "Handle payment refunds, invoices, and duplicate charges.".into(),
                score: 0.1,
            },
        ];
        let task = "The customer wants a refund for a duplicate invoice.";
        assert_eq!(OrtCpuReranker.name_for(task), "ort-cpu");
        let ranked = OrtCpuReranker.rerank(task, candidates.clone());
        let mut names: Vec<_> = ranked.iter().map(|item| item.name.clone()).collect();
        names.sort();
        assert_eq!(
            names,
            ["deploy-k8s".to_string(), "refund-helper".to_string()]
        );
        assert_eq!(ranked.len(), candidates.len());
        assert_eq!(ranked[0].name, "refund-helper");
        assert!(ranked[0].score >= ranked[1].score);
        assert!(ranked.iter().all(|item| (0.0..=1.0).contains(&item.score)));
    }

    #[test]
    #[ignore = "requires SKILLSTAR_LAYA_ONNX"]
    fn laya_graph_refuses_english_bundle_for_chinese_query() {
        let candidates = vec![
            RankedCandidate {
                name: "deploy-k8s".into(),
                description: "Deploy applications to Kubernetes clusters.".into(),
                score: 0.9,
            },
            RankedCandidate {
                name: "refund-helper".into(),
                description: "Handle payment refunds, invoices, and duplicate charges.".into(),
                score: 0.1,
            },
        ];
        let task = "客户要退掉重复扣款的发票";
        assert_eq!(OrtCpuReranker.name_for(task), "passthrough");
        assert_eq!(OrtCpuReranker.rerank(task, candidates.clone()), candidates);
    }

    #[test]
    #[ignore = "requires SKILLSTAR_LAYA_ONNX"]
    fn laya_logits_match_the_onnx_runtime_reference_within_1e_4() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/laya-ort-reference.json");
        let fixture: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let task = fixture["task"].as_str().unwrap();
        let instruction = fixture["instruction"].as_str().unwrap();
        let (ids, markers, logits) =
            super::logits_for_instruction(instruction, task).expect("laya graph");
        let expected_ids: Vec<i64> = fixture["input_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_i64().unwrap())
            .collect();
        let expected_markers: Vec<i64> = fixture["marker_pos"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_i64().unwrap())
            .collect();
        let expected_logits: Vec<f32> = fixture["logits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_f64().unwrap() as f32)
            .collect();
        assert_eq!(ids, expected_ids);
        assert_eq!(markers.as_slice(), expected_markers.as_slice());
        assert_eq!(logits.len(), expected_logits.len());
        for (got, want) in logits.iter().zip(expected_logits) {
            assert!((got - want).abs() <= 1e-4, "{got} vs {want}");
        }
    }

    unsafe fn restore(key: &str, previous: Option<std::ffi::OsString>) {
        match previous {
            Some(value) => unsafe { std::env::set_var(key, value) },
            None => unsafe { std::env::remove_var(key) },
        }
    }
}
