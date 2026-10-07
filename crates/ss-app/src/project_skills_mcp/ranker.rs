//! Reorders skill candidates. It does not add or remove ids, choose a
//! selection, or write a plan or approval.

#[derive(Debug, Clone, PartialEq)]
pub struct RankedCandidate {
    pub name: String,
    /// Frontmatter description only. The reranker must not read `SKILL.md` body.
    pub description: String,
    pub score: f32,
}

pub trait SkillReranker {
    fn name(&self) -> &'static str {
        "passthrough"
    }

    /// Name recorded on a plan for this task.
    fn name_for(&self, _task: &str) -> &'static str {
        self.name()
    }

    fn rerank(&self, task: &str, candidates: Vec<RankedCandidate>) -> Vec<RankedCandidate>;
}

/// Keeps BM25 order. The app does not load a local decision model.
#[derive(Debug, Default, Clone, Copy)]
pub struct PassthroughReranker;

impl SkillReranker for PassthroughReranker {
    fn rerank(&self, _task: &str, candidates: Vec<RankedCandidate>) -> Vec<RankedCandidate> {
        candidates
    }
}
