//! Markdown AST helpers for translation — parse, extract translatable nodes,
//! replace with translated content, and render back to CommonMark.
//!
//! Mirrors the design of `md-translator-rs`: walks the comrak AST in DFS order,
//! collects container-level (Paragraph/Heading/Item/BlockQuote) and leaf-level
//! (Image alt, Link text) translation units identified by an incrementing
//! integer id, then re-walks in the same order to substitute translations and
//! `format_commonmark` back to a string.
//!
//! Skipped node kinds (never translated): CodeBlock, Code, HtmlBlock, HtmlInline,
//! Math, FrontMatter (handled separately by the caller — SKILL.md frontmatter
//! is preserved as-is).

use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, Options, format_commonmark, parse_document};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct TranslatableNode {
    pub id: usize,
    pub text: String,
}

/// Parser options tuned for SKILL.md content.
///
/// Note: we deliberately do NOT enable the frontmatter extension — the caller
/// strips frontmatter before parsing and re-attaches it verbatim. This keeps
/// the AST round-trip clean (comrak's frontmatter re-emission can rewrite
/// quoting / indentation in surprising ways).
pub fn parse_options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.tasklist = true;
    options.extension.strikethrough = true;
    options.extension.autolink = true;
    options.extension.math_dollars = true;
    options
}

/// Parse + extract translatable nodes in a single arena lifetime.
pub fn extract(markdown: &str) -> Vec<TranslatableNode> {
    let arena = Arena::new();
    let opts = parse_options();
    let root = parse_document(&arena, markdown, &opts);
    let mut next_id = 1;
    let mut out = Vec::new();
    walk_extract(root, &mut next_id, &mut out);
    out
}

fn walk_extract<'a>(node: &'a AstNode<'a>, next_id: &mut usize, out: &mut Vec<TranslatableNode>) {
    match &node.data.borrow().value {
        // Real translation units: contain inline content directly.
        // (Item / BlockQuote / List wrap these — we descend through them.)
        NodeValue::Paragraph | NodeValue::Heading(_) | NodeValue::TableCell => {
            if let Some(text) = collect_container_text(node) {
                out.push(TranslatableNode { id: *next_id, text });
                *next_id += 1;
            }
            // Descend to catch leaf-level nodes (e.g. images) inside this block.
            for child in node.children() {
                walk_extract(child, next_id, out);
            }
        }
        NodeValue::Image(_) => {
            if let Some(text) = collect_leaf_text(node) {
                out.push(TranslatableNode { id: *next_id, text });
                *next_id += 1;
            }
            // Don't descend — image alt is a leaf.
        }
        // Skip these entirely.
        NodeValue::CodeBlock(_)
        | NodeValue::Code(_)
        | NodeValue::HtmlBlock(_)
        | NodeValue::HtmlInline(_)
        | NodeValue::Math(_)
        | NodeValue::FrontMatter(_) => {}
        // Everything else (Document, Item, TaskItem, List, BlockQuote, Table,
        // TableRow, Strong, Emph, Link, Text, …) — just descend.
        _ => {
            for child in node.children() {
                walk_extract(child, next_id, out);
            }
        }
    }
}

/// Collect text content within a container node, skipping code / HTML / math
/// inline children. Inline HTML opens a "depth" — text inside an open HTML tag
/// is not extracted (to avoid translating attribute values etc).
fn collect_container_text<'a>(node: &'a AstNode<'a>) -> Option<String> {
    let mut text = String::new();
    let mut html_depth = 0usize;
    for child in node.children() {
        collect_inline_text(child, &mut html_depth, &mut text);
    }
    let trimmed = text.trim();
    if trimmed.is_empty() || is_probable_noise(trimmed) {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn collect_leaf_text<'a>(node: &'a AstNode<'a>) -> Option<String> {
    let mut text = String::new();
    let mut html_depth = 0usize;
    collect_inline_text(node, &mut html_depth, &mut text);
    let trimmed = text.trim();
    if trimmed.is_empty() || is_probable_noise(trimmed) {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn collect_inline_text<'a>(node: &'a AstNode<'a>, html_depth: &mut usize, out: &mut String) {
    match &node.data.borrow().value {
        NodeValue::Text(text) => {
            if *html_depth == 0 {
                out.push_str(text);
            }
        }
        NodeValue::Code(_)
        | NodeValue::CodeBlock(_)
        | NodeValue::HtmlBlock(_)
        | NodeValue::Math(_) => {}
        NodeValue::HtmlInline(raw) => update_html_depth(raw, html_depth),
        NodeValue::SoftBreak | NodeValue::LineBreak => {
            if *html_depth == 0 {
                out.push(' ');
            }
        }
        NodeValue::Image(_) => {}
        // Descend into emph/strong/link/etc.
        _ => {
            for child in node.children() {
                collect_inline_text(child, html_depth, out);
            }
        }
    }
}

fn update_html_depth(raw: &str, depth: &mut usize) {
    let trimmed = raw.trim();
    if trimmed.starts_with("</") {
        *depth = depth.saturating_sub(1);
    } else if trimmed.starts_with('<') && !trimmed.starts_with("<!--") && !trimmed.ends_with("/>") {
        *depth += 1;
    }
}

/// Filter "noise" segments where translating wastes a request:
///   - URLs / paths
///   - pure identifiers (no whitespace, looks code-like)
///   - single-token strings entirely ASCII punctuation
fn is_probable_noise(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return true;
    }
    if trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("mailto:")
        || trimmed.starts_with('/')
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
    {
        return true;
    }
    let has_ws = trimmed.contains(char::is_whitespace);
    if !has_ws && (trimmed.contains('/') || trimmed.contains('\\')) {
        return true;
    }
    if !has_ws && trimmed.chars().all(|c| c.is_ascii_punctuation()) {
        return true;
    }
    false
}

/// Replace translated nodes back into the AST and render to markdown.
///
/// Walks the same DFS order as `extract`, looking up the i-th id in
/// `translations`. For containers we replace the first text descendant with
/// the translated text and clear all subsequent text descendants — that means
/// inline emphasis / inline code / inline HTML in the container gets folded
/// into the translation. This is the same trade-off md-translator-rs makes
/// (structure preservation: high; in-container inline preservation: medium).
///
/// If a translation is missing for an id, the original text is left in place
/// (best-effort partial translation rather than a hard error).
pub fn replace_and_render(markdown: &str, translations: &HashMap<usize, String>) -> String {
    let arena = Arena::new();
    let opts = parse_options();
    let root = parse_document(&arena, markdown, &opts);
    let mut next_id = 1;
    walk_replace(root, translations, &mut next_id);

    let mut out = Vec::new();
    if format_commonmark(root, &opts, &mut out).is_err() {
        return markdown.to_string();
    }
    String::from_utf8(out).unwrap_or_else(|_| markdown.to_string())
}

fn walk_replace<'a>(
    node: &'a AstNode<'a>,
    translations: &HashMap<usize, String>,
    next_id: &mut usize,
) {
    let value = node.data.borrow().value.clone();
    match value {
        NodeValue::Paragraph | NodeValue::Heading(_) | NodeValue::TableCell => {
            if container_has_replaceable_text(node) {
                if let Some(replacement) = translations.get(next_id) {
                    replace_first_text_descendant(node, replacement);
                }
                *next_id += 1;
            }
            for child in node.children() {
                walk_replace(child, translations, next_id);
            }
        }
        NodeValue::Image(_) => {
            if container_has_replaceable_text(node) {
                if let Some(replacement) = translations.get(next_id) {
                    replace_first_text_descendant(node, replacement);
                }
                *next_id += 1;
            }
        }
        NodeValue::CodeBlock(_)
        | NodeValue::Code(_)
        | NodeValue::HtmlBlock(_)
        | NodeValue::HtmlInline(_)
        | NodeValue::Math(_)
        | NodeValue::FrontMatter(_) => {}
        _ => {
            for child in node.children() {
                walk_replace(child, translations, next_id);
            }
        }
    }
}

fn container_has_replaceable_text<'a>(node: &'a AstNode<'a>) -> bool {
    let mut text = String::new();
    let mut depth = 0;
    for child in node.children() {
        collect_inline_text(child, &mut depth, &mut text);
    }
    let trimmed = text.trim();
    !trimmed.is_empty() && !is_probable_noise(trimmed)
}

/// Inject `replacement` into the first Text descendant in DFS order, and zero
/// out every subsequent Text descendant in that subtree.
fn replace_first_text_descendant<'a>(node: &'a AstNode<'a>, replacement: &str) {
    let mut state = ReplaceState::NotYet;
    replace_walk(node, replacement, &mut state);
}

enum ReplaceState {
    NotYet,
    Done,
}

fn replace_walk<'a>(node: &'a AstNode<'a>, replacement: &str, state: &mut ReplaceState) {
    let value = node.data.borrow().value.clone();
    match value {
        NodeValue::Text(_) => {
            match state {
                ReplaceState::NotYet => {
                    node.data.borrow_mut().value = NodeValue::Text(replacement.to_string());
                    *state = ReplaceState::Done;
                }
                ReplaceState::Done => {
                    node.data.borrow_mut().value = NodeValue::Text(String::new());
                }
            }
            return;
        }
        // Don't descend into protected nodes — keep their content intact.
        NodeValue::Code(_)
        | NodeValue::CodeBlock(_)
        | NodeValue::HtmlBlock(_)
        | NodeValue::HtmlInline(_)
        | NodeValue::Math(_) => return,
        _ => {}
    }
    for child in node.children() {
        replace_walk(child, replacement, state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_paragraphs_and_headings() {
        let md = "# Title\n\nFirst paragraph.\n\nSecond paragraph.\n";
        let nodes = extract(md);
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].text, "Title");
        assert_eq!(nodes[1].text, "First paragraph.");
        assert_eq!(nodes[2].text, "Second paragraph.");
    }

    #[test]
    fn skips_code_blocks_and_inline_code() {
        let md = "Run `cargo test` then\n\n```rust\nfn main() {}\n```\n\nAfterward.";
        let nodes = extract(md);
        let texts: Vec<&str> = nodes.iter().map(|n| n.text.as_str()).collect();
        assert_eq!(texts, vec!["Run  then", "Afterward."]);
    }

    #[test]
    fn extracts_list_items() {
        let md = "- alpha\n- beta\n- gamma\n";
        let nodes = extract(md);
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].text, "alpha");
        assert_eq!(nodes[1].text, "beta");
        assert_eq!(nodes[2].text, "gamma");
    }

    #[test]
    fn skips_pure_url_paragraph() {
        let md = "https://example.com\n\nReal text here.";
        let nodes = extract(md);
        // Pure URL paragraph is filtered out.
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].text, "Real text here.");
    }

    #[test]
    fn roundtrip_substitutes_translations() {
        let md = "# Hello\n\nWorld text.";
        let nodes = extract(md);
        let mut translations = HashMap::new();
        translations.insert(nodes[0].id, "你好".to_string());
        translations.insert(nodes[1].id, "世界文本".to_string());
        let out = replace_and_render(md, &translations);
        assert!(out.contains("你好"), "out: {out}");
        assert!(out.contains("世界文本"), "out: {out}");
        assert!(!out.contains("Hello"));
        assert!(!out.contains("World text"));
    }

    #[test]
    fn roundtrip_preserves_code_blocks() {
        let md = "Heading text\n\n```rust\nfn main() {}\n```\n\nMore text.";
        let nodes = extract(md);
        let mut translations = HashMap::new();
        for n in &nodes {
            translations.insert(n.id, format!("译文-{}", n.id));
        }
        let out = replace_and_render(md, &translations);
        assert!(out.contains("fn main()"), "code block must survive: {out}");
        assert!(out.contains("译文-"));
    }

    #[test]
    fn roundtrip_preserves_inline_code() {
        let md = "Use `cargo build` to compile.";
        let nodes = extract(md);
        assert_eq!(nodes.len(), 1);
        let mut translations = HashMap::new();
        translations.insert(nodes[0].id, "用来编译".to_string());
        let out = replace_and_render(md, &translations);
        assert!(
            out.contains("`cargo build`"),
            "inline code must survive: {out}"
        );
    }
}
