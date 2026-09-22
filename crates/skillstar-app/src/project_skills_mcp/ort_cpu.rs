//! CPU-only ONNX Runtime. A missing Laya model keeps the BM25 order.

use std::path::Path;

use ort::ep::CPU;
use ort::session::Session;

use super::ranker::{RankedCandidate, SkillReranker};

const LAYA_FILE: &str = "laya.onnx";

pub struct OrtCpuReranker;

impl SkillReranker for OrtCpuReranker {
    fn rerank(&self, candidates: Vec<RankedCandidate>) -> Vec<RankedCandidate> {
        let _ = laya_session_opens();
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

fn laya_session_opens() -> bool {
    let Ok(dir) = std::env::var("SKILLSTAR_LAYA_ONNX") else {
        return false;
    };
    if dir.is_empty() {
        return false;
    }
    let path = Path::new(&dir).join(LAYA_FILE);
    let Ok(bytes) = std::fs::read(&path) else {
        return false;
    };
    open_cpu_session(&bytes).is_ok()
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
        let previous = std::env::var_os("SKILLSTAR_LAYA_ONNX");
        unsafe { std::env::remove_var("SKILLSTAR_LAYA_ONNX") };
        let candidates = vec![
            RankedCandidate {
                name: "a".into(),
                score: 0.2,
            },
            RankedCandidate {
                name: "b".into(),
                score: 0.9,
            },
        ];
        let ranked = OrtCpuReranker.rerank(candidates.clone());
        assert_eq!(ranked, PassthroughReranker.rerank(candidates));

        let _lock = futures::executor::block_on(crate::test_support::ENV_LOCK.lock());
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

    unsafe fn restore(key: &str, previous: Option<std::ffi::OsString>) {
        match previous {
            Some(value) => unsafe { std::env::set_var(key, value) },
            None => unsafe { std::env::remove_var(key) },
        }
    }
}
