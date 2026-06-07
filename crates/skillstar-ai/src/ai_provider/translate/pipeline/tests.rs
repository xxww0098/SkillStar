use super::*;

#[test]
fn split_frontmatter_extracts_block() {
    let md = "---\nname: foo\ndescription: bar\n---\n\n# Body\n";
    let (fm, body) = split_frontmatter(md);
    assert_eq!(fm, Some("---\nname: foo\ndescription: bar\n---\n"));
    assert_eq!(body, "\n# Body\n");
}

#[test]
fn split_frontmatter_returns_none_when_absent() {
    let md = "# Just a heading\n\nBody.\n";
    let (fm, body) = split_frontmatter(md);
    assert_eq!(fm, None);
    assert_eq!(body, md);
}

#[test]
fn split_frontmatter_with_crlf() {
    let md = "---\r\nname: foo\r\n---\r\nBody\r\n";
    let (fm, _body) = split_frontmatter(md);
    assert!(fm.is_some());
    assert!(fm.unwrap().contains("name: foo"));
}

#[test]
fn reattach_preserves_frontmatter() {
    let md = "---\nname: foo\n---\n# Heading\n";
    let (fm, body) = split_frontmatter(md);
    let rebuilt = reattach_frontmatter(fm, body);
    assert_eq!(rebuilt, md);
}

#[test]
fn skip_when_already_chinese_target_zh() {
    let zh = "# 标题\n\n这是一段中文文本，足够多了。\n";
    assert!(should_skip_translation(zh, "zh-CN"));
}

#[test]
fn dont_skip_english_when_target_zh() {
    let en = "# Heading\n\nThis is some English text content.\n";
    assert!(!should_skip_translation(en, "zh-CN"));
}

#[test]
fn dont_skip_mixed_doc_when_target_zh() {
    let mixed = "# 已有中文\n\nThis English section still needs translation.\n";
    assert!(!should_skip_translation(mixed, "zh-CN"));
}

#[test]
fn skip_when_already_english_target_en() {
    let en = "# Heading\n\nAll lowercase ascii body text here.\n";
    assert!(should_skip_translation(en, "en"));
}

#[test]
fn long_segment_split_preserves_text_and_bounds() {
    let source = "This is a long technical paragraph about Markdown translation, code blocks, links, and cached AI provider routing. ".repeat(80);
    let chunks = split_text_for_translation(&source, MAX_CHARS_PER_SEGMENT);
    assert!(chunks.len() > 1);
    assert_eq!(chunks.concat(), source);
    assert!(
        chunks
            .iter()
            .all(|chunk| chunk.chars().count() <= MAX_CHARS_PER_SEGMENT)
    );
}

#[test]
fn long_segment_split_preserves_utf8_boundaries() {
    let source = "Translate this carefully，保留中文标点和 emoji 🚀 while keeping technical identifiers intact. ".repeat(90);
    let chunks = split_text_for_translation(&source, MAX_CHARS_PER_SEGMENT);
    assert!(chunks.len() > 1);
    assert_eq!(chunks.concat(), source);
    assert!(
        chunks
            .iter()
            .all(|chunk| chunk.is_char_boundary(chunk.len()))
    );
}

#[test]
fn large_skill_file_plan_from_env_has_bounded_segments() {
    let Ok(path) = std::env::var("SKILLSTAR_LARGE_SKILL_MD") else {
        eprintln!("SKILLSTAR_LARGE_SKILL_MD not set; skipping large-file plan test");
        return;
    };
    let markdown = std::fs::read_to_string(&path).expect("read large SKILL.md");
    assert!(
        markdown.len() > 20_000,
        "large fixture should be meaningfully large: {} bytes",
        markdown.len()
    );

    let (_frontmatter, body) = split_frontmatter(&markdown);
    let nodes = ast::extract(body);
    assert!(
        !nodes.is_empty(),
        "large fixture should have translatable nodes"
    );

    let mut unit_id = 1usize;
    let mut units = Vec::new();
    for node in &nodes {
        for chunk in split_text_for_translation(&node.text, MAX_CHARS_PER_SEGMENT) {
            units.push(TranslatableNode {
                id: unit_id,
                text: chunk,
            });
            unit_id += 1;
        }
    }

    let batches = batch::pack(&units, &BatchConfig::default());
    let max_unit_chars = units
        .iter()
        .map(|unit| unit.text.chars().count())
        .max()
        .unwrap_or(0);
    println!(
        "large skill plan: bytes={} nodes={} units={} batches={} max_unit_chars={}",
        markdown.len(),
        nodes.len(),
        units.len(),
        batches.len(),
        max_unit_chars
    );

    assert!(
        batches.len() > 1,
        "large fixture should span multiple batches"
    );
    assert!(max_unit_chars <= MAX_CHARS_PER_SEGMENT);
}

#[tokio::test]
#[ignore = "requires live AI provider env and makes network calls"]
async fn live_large_skill_translation_from_env() {
    use crate::ai_provider::ApiFormat;

    let path = std::env::var("SKILLSTAR_LIVE_TRANSLATE_SKILL_MD")
        .expect("SKILLSTAR_LIVE_TRANSLATE_SKILL_MD must point at a large SKILL.md");
    let api_key = std::env::var("SKILLSTAR_LIVE_TRANSLATE_API_KEY")
        .expect("SKILLSTAR_LIVE_TRANSLATE_API_KEY is required");
    let base_url = std::env::var("SKILLSTAR_LIVE_TRANSLATE_BASE_URL")
        .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
    let model = std::env::var("SKILLSTAR_LIVE_TRANSLATE_MODEL")
        .unwrap_or_else(|_| "gpt-4o-mini".to_string());
    let request_timeout_secs = std::env::var("SKILLSTAR_LIVE_TRANSLATE_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(90);

    let mut markdown = std::fs::read_to_string(&path).expect("read live large SKILL.md");
    if let Ok(max_bytes) = std::env::var("SKILLSTAR_LIVE_TRANSLATE_MAX_BYTES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or(())
    {
        let end = markdown
            .char_indices()
            .map(|(idx, _)| idx)
            .chain(std::iter::once(markdown.len()))
            .take_while(|idx| *idx <= max_bytes)
            .last()
            .unwrap_or(markdown.len());
        markdown.truncate(end);
    }

    assert!(
        markdown.len() > 20_000,
        "live fixture should stay large: {} bytes",
        markdown.len()
    );

    let config = AiConfig {
        enabled: true,
        api_format: ApiFormat::Openai,
        provider_ref: None,
        base_url,
        api_key,
        model,
        target_language: "zh-CN".to_string(),
        max_concurrent_requests: 6,
        request_timeout_secs: Some(request_timeout_secs),
        ..AiConfig::default()
    };
    let force_refresh = std::env::var("SKILLSTAR_LIVE_TRANSLATE_FORCE_REFRESH")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let started = std::time::Instant::now();
    let mut progress_events = Vec::new();
    let translated = translate_skill_with_options(
        &config,
        &markdown,
        TranslateOptions { force_refresh },
        |progress| {
            println!(
                "progress {:?} {}/{}",
                progress.phase, progress.current, progress.total
            );
            progress_events.push(progress);
        },
    )
    .await
    .expect("live large translation should complete");

    println!(
        "live large translation: input_bytes={} output_bytes={} elapsed_ms={}",
        markdown.len(),
        translated.len(),
        started.elapsed().as_millis()
    );
    assert!(!progress_events.is_empty());
    assert!(translated.len() > markdown.len() / 4);
    assert!(
        cjk_ratio(&translated) > 0.05,
        "translated output should contain visible Chinese"
    );
}
