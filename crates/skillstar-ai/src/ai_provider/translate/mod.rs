//! AST-based Markdown translation pipeline.
//!
//! Parses Markdown with `comrak`, extracts container-level translatable text,
//! packs it into XML `<seg id="N">…</seg>` batches, fans those batches out to
//! the configured LLM in parallel, then walks the AST again to substitute the
//! translations and re-emits CommonMark.
//!
//! Frontmatter (`---\n…\n---\n`) is split off and re-attached verbatim — the
//! AST never sees it, so it round-trips untouched.

mod ast;
mod batch;
mod cache;
mod pipeline;

pub use pipeline::{PipelinePhase, PipelineProgress, translate_skill};
