//! Reorders skill candidates. It does not add or remove ids, choose a
//! selection, or write a plan or approval.

#[derive(Debug, Clone, PartialEq)]
pub struct RankedCandidate {
    pub name: String,
    pub score: f32,
}

pub trait SkillReranker {
    fn name(&self) -> &'static str {
        "passthrough"
    }

    fn rerank(&self, candidates: Vec<RankedCandidate>) -> Vec<RankedCandidate>;
}

/// Permanent fallback. Later rankers that cannot load behave like this.
#[derive(Debug, Default, Clone, Copy)]
pub struct PassthroughReranker;

impl SkillReranker for PassthroughReranker {
    fn rerank(&self, candidates: Vec<RankedCandidate>) -> Vec<RankedCandidate> {
        candidates
    }
}
