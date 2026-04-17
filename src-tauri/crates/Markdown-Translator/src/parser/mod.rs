pub mod extractor;
pub mod frontmatter;
pub mod mapper;
pub mod protector;

use std::path::Path;

use crate::types::{FrontMatter, ParsedDocument};

/// Parse a Markdown file into a `ParsedDocument` with extracted segments.
pub fn parse(source_text: &str, source_path: &Path, target_lang: &str) -> ParsedDocument {
    let fm_split = frontmatter::split_front_matter(source_text);

    let front_matter = if fm_split.line_count > 0 {
        Some(FrontMatter {
            raw: fm_split.raw,
            data: fm_split.data.clone(),
        })
    } else {
        None
    };

    let segments = extractor::extract(
        &fm_split.body,
        fm_split.line_count,
        &fm_split.data,
    );

    ParsedDocument {
        source_path: source_path.to_owned(),
        source_text: source_text.to_owned(),
        body_text: fm_split.body,
        target_lang: target_lang.to_owned(),
        front_matter,
        body_line_offset: fm_split.line_count,
        segments,
    }
}
