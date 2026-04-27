use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use futures::StreamExt;
use regex::{Captures, Regex};
use tracing::{debug, warn};

use crate::cache::{self, TranslationKind};
use skillstar_ai::ai_provider::AiConfig;

use crate::services;

#[derive(Debug, Clone, Copy)]
struct MarkdownOptions {
    translate_frontmatter: bool,
    translate_multiline_code: bool,
    translate_latex: bool,
    translate_link_text: bool,
}

impl Default for MarkdownOptions {
    fn default() -> Self {
        Self {
            translate_frontmatter: false,
            translate_multiline_code: false,
            translate_latex: false,
            translate_link_text: true,
        }
    }
}

#[derive(Debug, Clone)]
struct ProcessedMarkdown {
    content_lines: Vec<String>,
    placeholders: HashMap<String, String>,
}

#[derive(Debug, Clone)]
enum LinePart {
    Literal(String),
    Translatable {
        index: usize,
        leading: String,
        trailing: String,
    },
}

#[derive(Debug, Clone)]
struct LinePlan {
    parts: Vec<LinePart>,
}

#[derive(Debug, Clone)]
struct PlaceholderCounters {
    frontmatter: usize,
    code: usize,
    link: usize,
    heading: usize,
    list: usize,
    blockquote: usize,
    latex_block: usize,
    latex_inline: usize,
    html: usize,
}

impl Default for PlaceholderCounters {
    fn default() -> Self {
        Self {
            frontmatter: 100,
            code: 100,
            link: 100,
            heading: 100,
            list: 100,
            blockquote: 100,
            latex_block: 100,
            latex_inline: 100,
            html: 100,
        }
    }
}

static MULTILINE_CODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)```.*?```|~~~.*?~~~").expect("valid multiline code regex"));
static LATEX_BLOCK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)\$\$.*?\$\$").expect("valid latex block regex"));
static INLINE_CODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`([^`\n]+?)`").expect("valid inline code regex"));
static INLINE_LATEX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$([^\$\n]+?)\$").expect("valid inline latex regex"));
static HTML_COMMENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<!--.*?-->").expect("valid html comment regex"));
static HTML_SELF_CLOSING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<([A-Za-z][A-Za-z0-9-]*)\s*[^>]*\/>"#).expect("valid html self closing regex")
});
static HTML_END_TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"</([A-Za-z][A-Za-z0-9-]*)>").expect("valid html end regex"));
static HTML_START_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<([A-Za-z][A-Za-z0-9-]*)(?:\s+[^>]*)?>"#).expect("valid html start regex")
});
static IMAGE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(!\[)(.*?)(\]\(.*?\))").expect("valid image regex"));
static LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\[)(.*?)(\]\(.*?\))").expect("valid link regex"));
static HEADING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(#{1,6}\s)(.*)$").expect("valid heading regex"));
static LIST_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*(?:[-*]|\d+\.)\s+)(.*)$").expect("valid list regex"));
static BLOCKQUOTE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(>\s)(.*)$").expect("valid blockquote regex"));
static PLACEHOLDER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"<<<(?:FRONTMATTER_\d+|MULTILINE_CODE_\d+|LATEX_BLOCK_\d+|CODE_\d+|LATEX_INLINE_\d+|LINK_PRE_\d+|LINK_SUF_\d+|LINK_\d+|HEADING_\d+|LIST_\d+|BLOCKQUOTE_\d+|STRONG_\d+|HTML_\d+)>>>",
    )
    .expect("valid placeholder regex")
});

#[async_trait::async_trait]
trait MarkdownFragmentTranslator: Send + Sync {
    async fn translate(&self, text: &str) -> Result<String, String>;
}

struct ApiFragmentTranslator {
    ai_config: AiConfig,
    provider_name: String,
    target_language: String,
}

#[async_trait::async_trait]
impl MarkdownFragmentTranslator for ApiFragmentTranslator {
    async fn translate(&self, text: &str) -> Result<String, String> {
        let result = services::translate_with_provider(
            &self.provider_name,
            &self.ai_config,
            text,
            "auto",
            &self.target_language,
        )
        .await
        .map_err(|err| err.to_string())?;

        Ok(result.translated_text)
    }
}

fn next_placeholder(kind: &str, counter: &mut usize) -> String {
    let placeholder = format!("<<<{kind}_{counter}>>>");
    *counter += 1;
    placeholder
}

fn split_leading_frontmatter(text: &str) -> Option<(&str, &str)> {
    if !text.starts_with("---\n") {
        return None;
    }

    let mut offset = 4;
    for line in text[4..].split_inclusive('\n') {
        let trimmed = line.trim_end_matches('\n');
        if trimmed == "---" {
            let end = offset + line.len();
            return Some((&text[..end], &text[end..]));
        }
        offset += line.len();
    }

    if text[4..].ends_with("\n---") {
        return Some((text, ""));
    }

    None
}

fn replace_all_with_placeholders(
    regex: &Regex,
    text: &str,
    kind: &str,
    counter: &mut usize,
    placeholders: &mut HashMap<String, String>,
) -> String {
    regex
        .replace_all(text, |caps: &Captures| {
            let matched = caps
                .get(0)
                .map(|m| m.as_str())
                .unwrap_or_default()
                .to_string();
            let placeholder = next_placeholder(kind, counter);
            placeholders.insert(placeholder.clone(), matched);
            placeholder
        })
        .into_owned()
}

fn split_lines_preserve_newline(text: &str) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }

    text.split_inclusive('\n').map(str::to_string).collect()
}

fn split_line_ending(line: &str) -> (&str, &str) {
    if let Some(stripped) = line.strip_suffix('\n') {
        (stripped, "\n")
    } else {
        (line, "")
    }
}

fn filter_markdown_lines(content: &str, options: MarkdownOptions) -> ProcessedMarkdown {
    let mut placeholders = HashMap::new();
    let mut counters = PlaceholderCounters::default();
    let mut full_text = content.to_string();

    if !options.translate_frontmatter {
        if let Some((frontmatter, rest)) = split_leading_frontmatter(&full_text) {
            let placeholder = next_placeholder("FRONTMATTER", &mut counters.frontmatter);
            placeholders.insert(placeholder.clone(), frontmatter.to_string());
            full_text = format!("{placeholder}{rest}");
        }
    }

    if !options.translate_multiline_code {
        full_text = replace_all_with_placeholders(
            &MULTILINE_CODE_RE,
            &full_text,
            "MULTILINE_CODE",
            &mut counters.code,
            &mut placeholders,
        );
    }

    if !options.translate_latex {
        full_text = replace_all_with_placeholders(
            &LATEX_BLOCK_RE,
            &full_text,
            "LATEX_BLOCK",
            &mut counters.latex_block,
            &mut placeholders,
        );
    }

    let mut content_lines = Vec::new();
    for raw_line in split_lines_preserve_newline(&full_text) {
        let (body, ending) = split_line_ending(&raw_line);
        let mut line = body.to_string();

        line = replace_all_with_placeholders(
            &INLINE_CODE_RE,
            &line,
            "CODE",
            &mut counters.code,
            &mut placeholders,
        );

        if !options.translate_latex {
            line = INLINE_LATEX_RE
                .replace_all(&line, |caps: &Captures| {
                    let matched = caps.get(0).map(|m| m.as_str()).unwrap_or_default();
                    let inner = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
                    if inner
                        .chars()
                        .all(|ch| ch.is_ascii_digit() || matches!(ch, ' ' | ',' | '.'))
                        && !inner.contains('\\')
                    {
                        return matched.to_string();
                    }

                    let placeholder = next_placeholder("LATEX_INLINE", &mut counters.latex_inline);
                    placeholders.insert(placeholder.clone(), matched.to_string());
                    placeholder
                })
                .into_owned();
        }

        line = replace_all_with_placeholders(
            &HTML_COMMENT_RE,
            &line,
            "HTML",
            &mut counters.html,
            &mut placeholders,
        );
        line = replace_all_with_placeholders(
            &HTML_SELF_CLOSING_RE,
            &line,
            "HTML",
            &mut counters.html,
            &mut placeholders,
        );
        line = replace_all_with_placeholders(
            &HTML_END_TAG_RE,
            &line,
            "HTML",
            &mut counters.html,
            &mut placeholders,
        );
        line = replace_all_with_placeholders(
            &HTML_START_TAG_RE,
            &line,
            "HTML",
            &mut counters.html,
            &mut placeholders,
        );

        line = IMAGE_RE
            .replace_all(&line, |caps: &Captures| {
                let prefix = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
                let inner = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
                let suffix = caps.get(3).map(|m| m.as_str()).unwrap_or_default();

                if inner.trim().is_empty() {
                    let placeholder = next_placeholder("LINK", &mut counters.link);
                    placeholders.insert(
                        placeholder.clone(),
                        caps.get(0)
                            .map(|m| m.as_str())
                            .unwrap_or_default()
                            .to_string(),
                    );
                    return placeholder;
                }

                let prefix_placeholder = next_placeholder("LINK_PRE", &mut counters.link);
                placeholders.insert(prefix_placeholder.clone(), prefix.to_string());
                let suffix_placeholder = next_placeholder("LINK_SUF", &mut counters.link);
                placeholders.insert(suffix_placeholder.clone(), suffix.to_string());
                format!("{prefix_placeholder}{inner}{suffix_placeholder}")
            })
            .into_owned();

        line = LINK_RE
            .replace_all(&line, |caps: &Captures| {
                let prefix = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
                let inner = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
                let suffix = caps.get(3).map(|m| m.as_str()).unwrap_or_default();
                let full = caps.get(0).map(|m| m.as_str()).unwrap_or_default();

                if options.translate_link_text {
                    let prefix_placeholder = next_placeholder("LINK_PRE", &mut counters.link);
                    placeholders.insert(prefix_placeholder.clone(), prefix.to_string());
                    let suffix_placeholder = next_placeholder("LINK_SUF", &mut counters.link);
                    placeholders.insert(suffix_placeholder.clone(), suffix.to_string());
                    format!("{prefix_placeholder}{inner}{suffix_placeholder}")
                } else {
                    let placeholder = next_placeholder("LINK", &mut counters.link);
                    placeholders.insert(placeholder.clone(), full.to_string());
                    placeholder
                }
            })
            .into_owned();

        line = HEADING_RE
            .replace(&line, |caps: &Captures| {
                let prefix = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
                let content = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
                let placeholder = next_placeholder("HEADING", &mut counters.heading);
                placeholders.insert(placeholder.clone(), prefix.to_string());
                format!("{placeholder}{content}")
            })
            .into_owned();

        line = LIST_RE
            .replace(&line, |caps: &Captures| {
                let prefix = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
                let content = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
                let placeholder = next_placeholder("LIST", &mut counters.list);
                placeholders.insert(placeholder.clone(), prefix.to_string());
                format!("{placeholder}{content}")
            })
            .into_owned();

        line = BLOCKQUOTE_RE
            .replace(&line, |caps: &Captures| {
                let prefix = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
                let content = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
                let placeholder = next_placeholder("BLOCKQUOTE", &mut counters.blockquote);
                placeholders.insert(placeholder.clone(), prefix.to_string());
                format!("{placeholder}{content}")
            })
            .into_owned();

        content_lines.push(format!("{line}{ending}"));
    }

    ProcessedMarkdown {
        content_lines,
        placeholders,
    }
}

fn has_translatable_letters(text: &str) -> bool {
    text.chars().any(char::is_alphabetic)
}

fn register_unique_text(
    unique_texts: &mut Vec<String>,
    index_map: &mut HashMap<String, usize>,
    text: &str,
) -> usize {
    if let Some(index) = index_map.get(text) {
        return *index;
    }

    let index = unique_texts.len();
    let owned = text.to_string();
    unique_texts.push(owned.clone());
    index_map.insert(owned, index);
    index
}

fn build_line_plans(lines: &[String]) -> (Vec<LinePlan>, Vec<String>) {
    let mut plans = Vec::with_capacity(lines.len());
    let mut unique_texts = Vec::new();
    let mut index_map = HashMap::new();

    for line in lines {
        let mut parts = Vec::new();
        let mut last = 0;

        for matched in PLACEHOLDER_RE.find_iter(line) {
            if matched.start() > last {
                push_text_part(
                    &line[last..matched.start()],
                    &mut parts,
                    &mut unique_texts,
                    &mut index_map,
                );
            }
            parts.push(LinePart::Literal(matched.as_str().to_string()));
            last = matched.end();
        }

        if last < line.len() {
            push_text_part(&line[last..], &mut parts, &mut unique_texts, &mut index_map);
        }

        plans.push(LinePlan { parts });
    }

    (plans, unique_texts)
}

fn push_text_part(
    text: &str,
    parts: &mut Vec<LinePart>,
    unique_texts: &mut Vec<String>,
    index_map: &mut HashMap<String, usize>,
) {
    if text.is_empty() {
        return;
    }

    let trimmed = text.trim();
    if trimmed.is_empty() || !has_translatable_letters(trimmed) {
        parts.push(LinePart::Literal(text.to_string()));
        return;
    }

    let leading_len = text.len() - text.trim_start().len();
    let trailing_len = text.len() - text.trim_end().len();
    let leading = text[..leading_len].to_string();
    let trailing = if trailing_len == 0 {
        String::new()
    } else {
        text[text.len() - trailing_len..].to_string()
    };
    let end = text.len() - trailing_len;
    let core = &text[leading_len..end];
    let index = register_unique_text(unique_texts, index_map, core);
    parts.push(LinePart::Translatable {
        index,
        leading,
        trailing,
    });
}

async fn translate_unique_fragments(
    translator: Arc<dyn MarkdownFragmentTranslator>,
    unique_texts: &[String],
    target_language: &str,
    provider_identity: Option<&str>,
) -> Result<Vec<String>, String> {
    if unique_texts.is_empty() {
        return Ok(Vec::new());
    }

    let target_language = target_language.to_string();
    let provider_identity = provider_identity
        .map(str::to_string)
        .unwrap_or_else(|| "translation_api:markdown_fragments".to_string());

    let results = futures::stream::iter(unique_texts.iter().cloned().enumerate())
        .map(|(index, text)| {
            let translator = translator.clone();
            let target_language = target_language.clone();
            let provider_identity = provider_identity.clone();

            async move {
                if let Ok(Some(cached)) = cache::get_cached_translation_for_provider(
                    TranslationKind::SkillSection,
                    &target_language,
                    &text,
                    Some(&provider_identity),
                ) {
                    return Ok((index, cached.translated_text));
                }

                let translated = translator.translate(&text).await?;

                if let Err(err) = cache::upsert_translation_for_provider(
                    TranslationKind::SkillSection,
                    &target_language,
                    &text,
                    &translated,
                    Some(&provider_identity),
                    Some(&provider_identity),
                ) {
                    warn!(
                        target: "translate",
                        provider_identity = %provider_identity,
                        error = %err,
                        "markdown fragment cache write failed"
                    );
                }

                Ok((index, translated))
            }
        })
        .buffered(4)
        .collect::<Vec<Result<(usize, String), String>>>()
        .await;

    let mut out = vec![String::new(); unique_texts.len()];
    for item in results {
        let (index, translated) = item?;
        out[index] = translated;
    }
    Ok(out)
}

fn render_translated_lines(plans: &[LinePlan], translations: &[String]) -> String {
    let mut rendered = String::new();

    for plan in plans {
        for part in &plan.parts {
            match part {
                LinePart::Literal(text) => rendered.push_str(text),
                LinePart::Translatable {
                    index,
                    leading,
                    trailing,
                } => {
                    rendered.push_str(leading);
                    rendered.push_str(&translations[*index]);
                    rendered.push_str(trailing);
                }
            }
        }
    }

    rendered
}

fn restore_placeholders(text: String, placeholders: &HashMap<String, String>) -> String {
    let mut restored = text;
    let mut items: Vec<_> = placeholders.iter().collect();
    items.sort_by(|(left, _), (right, _)| right.len().cmp(&left.len()));

    for (placeholder, value) in items {
        restored = restored.replace(placeholder, value);
    }

    restored
}

async fn translate_markdown_with_translator(
    translator: Arc<dyn MarkdownFragmentTranslator>,
    content: &str,
    target_language: &str,
    provider_identity: Option<&str>,
) -> Result<String, String> {
    if content.trim().is_empty() {
        return Ok(content.to_string());
    }

    let processed = filter_markdown_lines(content, MarkdownOptions::default());
    let (plans, unique_texts) = build_line_plans(&processed.content_lines);
    debug!(
        target: "translate",
        unique_fragments = unique_texts.len(),
        placeholders = processed.placeholders.len(),
        "markdown fragment translation prepared"
    );

    let translations = translate_unique_fragments(
        translator,
        &unique_texts,
        target_language,
        provider_identity,
    )
    .await?;
    let rendered = render_translated_lines(&plans, &translations);
    Ok(restore_placeholders(rendered, &processed.placeholders))
}

pub async fn translate_markdown_with_provider(
    ai_config: &AiConfig,
    provider_name: &str,
    content: &str,
    target_language: &str,
    provider_identity: Option<&str>,
) -> Result<String, String> {
    let translator: Arc<dyn MarkdownFragmentTranslator> = Arc::new(ApiFragmentTranslator {
        ai_config: ai_config.clone(),
        provider_name: provider_name.to_string(),
        target_language: target_language.to_string(),
    });

    translate_markdown_with_translator(translator, content, target_language, provider_identity)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockTranslator;

    #[async_trait::async_trait]
    impl MarkdownFragmentTranslator for MockTranslator {
        async fn translate(&self, text: &str) -> Result<String, String> {
            Ok(format!("[zh]{text}"))
        }
    }

    #[tokio::test]
    async fn markdown_pipeline_preserves_frontmatter_code_and_links() {
        let input = "---\nname: demo\ndescription: desc\n---\n\n# Title\n\nUse `cargo test` in [docs](https://example.com).\n\n```ts\nconst x = 1;\n```\n";
        let out = translate_markdown_with_translator(
            Arc::new(MockTranslator),
            input,
            "zh-CN",
            Some("translation_api:test"),
        )
        .await
        .expect("translation should succeed");

        assert!(out.contains("name: demo"));
        assert!(out.contains("# [zh]Title"));
        assert!(out.contains("`cargo test`"));
        assert!(out.contains("[zh]docs](https://example.com)"));
        assert!(out.contains("```ts\nconst x = 1;\n```"));
    }

    #[test]
    fn restore_placeholders_reverses_filter() {
        let mut placeholders = HashMap::new();
        placeholders.insert("<<<CODE_100>>>".to_string(), "`cargo test`".to_string());
        placeholders.insert(
            "<<<LINK_100>>>".to_string(),
            "[docs](https://example.com)".to_string(),
        );

        let text = "Run <<<CODE_100>>> and see <<<LINK_100>>>".to_string();
        let restored = restore_placeholders(text, &placeholders);
        assert_eq!(
            restored,
            "Run `cargo test` and see [docs](https://example.com)"
        );
    }

    #[test]
    fn restore_placeholders_sorts_by_length_descending() {
        let mut placeholders = HashMap::new();
        placeholders.insert("<<<A>>>".to_string(), "x".to_string());
        placeholders.insert("<<<AB>>>".to_string(), "y".to_string());

        let text = "<<<AB>>><<<A>>>".to_string();
        let restored = restore_placeholders(text, &placeholders);
        assert_eq!(restored, "yx");
    }

    #[test]
    fn split_lines_preserve_newline_empty() {
        let lines = split_lines_preserve_newline("");
        assert_eq!(lines, vec![""]);
    }

    #[test]
    fn split_lines_preserve_newline_basic() {
        let lines = split_lines_preserve_newline("a\nb\n");
        assert_eq!(lines, vec!["a\n", "b\n"]);
    }

    #[test]
    fn split_lines_preserve_newline_no_trailing_newline() {
        let lines = split_lines_preserve_newline("a\nb");
        assert_eq!(lines, vec!["a\n", "b"]);
    }

    #[test]
    fn has_translatable_letters_detects_latin() {
        assert!(has_translatable_letters("hello"));
        assert!(has_translatable_letters("Hello World"));
    }

    #[test]
    fn has_translatable_letters_empty_false() {
        assert!(!has_translatable_letters(""));
        assert!(!has_translatable_letters("12345"));
        assert!(!has_translatable_letters("!@#$%"));
    }

    #[test]
    fn next_placeholder_increments_counter() {
        let mut counter = 100;
        let p1 = next_placeholder("CODE", &mut counter);
        let p2 = next_placeholder("CODE", &mut counter);
        assert_eq!(p1, "<<<CODE_100>>>");
        assert_eq!(p2, "<<<CODE_101>>>");
        assert_eq!(counter, 102);
    }

    #[test]
    fn split_leading_frontmatter_basic() {
        let text = "---\nname: demo\n---\n\n# Body\n";
        let (fm, rest) = split_leading_frontmatter(text).unwrap();
        assert_eq!(fm, "---\nname: demo\n---\n");
        assert_eq!(rest, "\n# Body\n");
    }

    #[test]
    fn split_leading_frontmatter_no_terminator() {
        let text = "---\nname: demo\n";
        assert!(split_leading_frontmatter(text).is_none());
    }

    #[test]
    fn split_leading_frontmatter_no_leading_dashes() {
        let text = "# Title\n---\nname: demo\n---\n";
        assert!(split_leading_frontmatter(text).is_none());
    }

    #[test]
    fn split_line_ending_with_newline() {
        let (body, ending) = split_line_ending("hello\n");
        assert_eq!(body, "hello");
        assert_eq!(ending, "\n");
    }

    #[test]
    fn split_line_ending_without_newline() {
        let (body, ending) = split_line_ending("hello");
        assert_eq!(body, "hello");
        assert_eq!(ending, "");
    }

    #[test]
    fn filter_markdown_lines_preserves_inline_code() {
        let processed =
            filter_markdown_lines("Use `cargo test` here.\n", MarkdownOptions::default());
        let joined = processed.content_lines.join("");
        assert!(joined.contains("<<<CODE_"));
        assert!(processed.placeholders.values().any(|v| v == "`cargo test`"));
    }

    #[test]
    fn filter_markdown_lines_preserves_multiline_code() {
        let input = "```ts\nconst x = 1;\n```\n";
        let processed = filter_markdown_lines(input, MarkdownOptions::default());
        let joined = processed.content_lines.join("");
        assert!(joined.contains("<<<MULTILINE_CODE_"));
    }

    #[test]
    fn filter_markdown_lines_preserves_links_when_translate_link_text_true() {
        let input = "See [docs](https://example.com).\n";
        let processed = filter_markdown_lines(input, MarkdownOptions::default());
        let joined = processed.content_lines.join("");
        assert!(joined.contains("<<<LINK_PRE_"));
        assert!(joined.contains("<<<LINK_SUF_"));
        assert!(processed.placeholders.values().any(|v| v == "["));
    }

    #[test]
    fn filter_markdown_lines_preserves_headings() {
        let input = "# Title\n";
        let processed = filter_markdown_lines(input, MarkdownOptions::default());
        let joined = processed.content_lines.join("");
        assert!(joined.contains("<<<HEADING_"));
        assert!(processed.placeholders.values().any(|v| v == "# "));
    }

    #[test]
    fn filter_markdown_lines_preserves_list_prefix() {
        let input = "- Item\n";
        let processed = filter_markdown_lines(input, MarkdownOptions::default());
        let joined = processed.content_lines.join("");
        assert!(joined.contains("<<<LIST_"));
    }

    #[test]
    fn filter_markdown_lines_preserves_blockquote() {
        let input = "> Quote\n";
        let processed = filter_markdown_lines(input, MarkdownOptions::default());
        let joined = processed.content_lines.join("");
        assert!(joined.contains("<<<BLOCKQUOTE_"));
    }

    #[test]
    fn build_line_plans_with_no_placeholders() {
        let lines = vec!["hello world\n".to_string()];
        let (plans, unique_texts) = build_line_plans(&lines);
        assert_eq!(plans.len(), 1);
        assert_eq!(unique_texts.len(), 1);
        assert_eq!(unique_texts[0], "hello world");
    }

    #[test]
    fn build_line_plans_skips_untranslatable_text() {
        let lines = vec!["12345\n".to_string()];
        let (plans, unique_texts) = build_line_plans(&lines);
        assert_eq!(unique_texts.len(), 0);
        match &plans[0].parts[0] {
            LinePart::Literal(text) => assert_eq!(text, "12345\n"),
            _ => panic!("expected literal part"),
        }
    }

    #[test]
    fn render_translated_lines_basic() {
        let lines = vec!["hello\n".to_string()];
        let (plans, _unique_texts) = build_line_plans(&lines);
        let translations = vec!["你好".to_string()];
        let rendered = render_translated_lines(&plans, &translations);
        assert_eq!(rendered, "你好\n");
    }

    #[test]
    fn render_translated_lines_with_leading_trailing_whitespace() {
        let lines = vec!["  hello  \n".to_string()];
        let (plans, _unique_texts) = build_line_plans(&lines);
        let translations = vec!["你好".to_string()];
        let rendered = render_translated_lines(&plans, &translations);
        assert_eq!(rendered, "  你好  \n");
    }
}
