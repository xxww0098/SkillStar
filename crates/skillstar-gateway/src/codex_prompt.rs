//! Codex compaction text, copied from the reference gateway.
//!
//! The prompt strings are Codex's own wording. [`COMPACTION_MARKER`] is this
//! product's prefix. Inserting them into a turn is a later slice.

/// Prefix on a compaction item this gateway wrote.
pub const COMPACTION_MARKER: &str = "skillstar1:";

/// Handoff instructions Codex uses for a context checkpoint.
pub const CODEX_COMPACT_PROMPT: &str = "You are performing a CONTEXT CHECKPOINT COMPACTION. Create a handoff summary for another LLM that will resume the task.\n\nInclude:\n- Current progress and key decisions made\n- Important context, constraints, or user preferences\n- What remains to be done (clear next steps)\n- Any critical data, examples, or references needed to continue\n\nBe concise, structured, and focused on helping the next LLM seamlessly continue the work.";

/// Preface Codex puts in front of another model's summary.
pub const CODEX_SUMMARY_PREFIX: &str = "Another language model started to solve this problem and produced a summary of its thinking process. You also have access to the state of the tools that were used by that language model. Use this to build on the work that has already been done and avoid duplicating work. Here is the summary produced by the other language model, use the information in this summary to assist with your own analysis:";

#[cfg(test)]
mod tests {
    use super::{CODEX_COMPACT_PROMPT, CODEX_SUMMARY_PREFIX, COMPACTION_MARKER};

    #[test]
    fn compact_prompt_bytes_match_magpie() {
        assert_eq!(COMPACTION_MARKER, "skillstar1:");
        assert_eq!(
            CODEX_COMPACT_PROMPT,
            "You are performing a CONTEXT CHECKPOINT COMPACTION. Create a handoff summary for another LLM that will resume the task.\n\nInclude:\n- Current progress and key decisions made\n- Important context, constraints, or user preferences\n- What remains to be done (clear next steps)\n- Any critical data, examples, or references needed to continue\n\nBe concise, structured, and focused on helping the next LLM seamlessly continue the work."
        );
        assert_eq!(
            CODEX_SUMMARY_PREFIX,
            "Another language model started to solve this problem and produced a summary of its thinking process. You also have access to the state of the tools that were used by that language model. Use this to build on the work that has already been done and avoid duplicating work. Here is the summary produced by the other language model, use the information in this summary to assist with your own analysis:"
        );
        assert!(!COMPACTION_MARKER.contains("magpie"));
        assert!(!CODEX_COMPACT_PROMPT.to_ascii_lowercase().contains("magpie"));
        assert!(!CODEX_SUMMARY_PREFIX.to_ascii_lowercase().contains("magpie"));
    }
}
