//! Pure text helpers for the translation pipeline: segment splitting,
//! frontmatter handling, and the skip-when-already-target heuristic.
//!
//! These are mechanical string utilities with no provider or network
//! dependencies; they are unit-tested in `pipeline/tests.rs`.

pub(crate) fn split_text_for_translation(text: &str, max_chars: usize) -> Vec<String> {
    if max_chars == 0 || text.chars().count() <= max_chars {
        return vec![text.to_string()];
    }

    let mut boundaries: Vec<usize> = text.char_indices().map(|(idx, _)| idx).collect();
    boundaries.push(text.len());

    let mut chunks = Vec::new();
    let mut start_char = 0usize;
    let total_chars = boundaries.len().saturating_sub(1);

    while start_char < total_chars {
        let hard_end_char = (start_char + max_chars).min(total_chars);
        if hard_end_char == total_chars {
            chunks.push(text[boundaries[start_char]..].to_string());
            break;
        }

        let min_soft_end_char = start_char + (max_chars / 2).max(1);
        let mut end_char = hard_end_char;
        for candidate in (min_soft_end_char..=hard_end_char).rev() {
            let char_start = boundaries[candidate - 1];
            let char_end = boundaries[candidate];
            let Some(ch) = text[char_start..char_end].chars().next() else {
                continue;
            };
            if is_soft_split_char(ch) {
                end_char = candidate;
                break;
            }
        }

        chunks.push(text[boundaries[start_char]..boundaries[end_char]].to_string());
        start_char = end_char;
    }
    chunks
}

fn is_soft_split_char(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '.' | ','
                | ';'
                | ':'
                | '!'
                | '?'
                | ')'
                | ']'
                | '}'
                | '。'
                | '，'
                | '；'
                | '：'
                | '！'
                | '？'
                | '、'
                | '）'
                | '】'
                | '》'
        )
}

// ── Frontmatter helpers ─────────────────────────────────────────────

/// Split YAML frontmatter (`---\n…\n---\n`) from body. Returns
/// `(Some(frontmatter_including_fences), body)` or `(None, full_input)`.
pub(crate) fn split_frontmatter(input: &str) -> (Option<&str>, &str) {
    // Mirror src/lib/frontmatter.ts (FRONTMATTER_RE).
    let bom_stripped = input.strip_prefix('\u{FEFF}').unwrap_or(input);
    if !bom_stripped.starts_with("---") {
        return (None, input);
    }
    let after_open = &bom_stripped[3..];
    let after_open = after_open
        .strip_prefix("\r\n")
        .or_else(|| after_open.strip_prefix('\n'))
        .unwrap_or(after_open);

    let Some(close_idx) = find_closing_fence(after_open) else {
        return (None, input);
    };

    let bom_offset = input.len() - bom_stripped.len();
    let total_open = bom_offset + 3 + (bom_stripped[3..].len() - after_open.len());
    let frontmatter_end_offset = total_open + close_idx;
    let after_close = &after_open[close_idx..];

    // Consume the closing "---" line.
    let after_dashes = after_close.strip_prefix("---").unwrap_or(after_close);
    let body_start_offset_relative = after_close.len() - after_dashes.len();
    let body_after_close = after_dashes
        .strip_prefix("\r\n")
        .or_else(|| after_dashes.strip_prefix('\n'))
        .unwrap_or(after_dashes);
    let trailing_newline = after_dashes.len() - body_after_close.len();

    let frontmatter_block_end =
        frontmatter_end_offset + body_start_offset_relative + trailing_newline;

    let frontmatter = &input[..frontmatter_block_end];
    let body = &input[frontmatter_block_end..];
    (Some(frontmatter), body)
}

fn find_closing_fence(s: &str) -> Option<usize> {
    let mut start = 0;
    for line in s.split_inclusive('\n') {
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
        if trimmed == "---" {
            return Some(start);
        }
        start += line.len();
    }
    None
}

pub(crate) fn reattach_frontmatter(frontmatter: Option<&str>, body: &str) -> String {
    match frontmatter {
        Some(fm) => format!("{fm}{body}"),
        None => body.to_string(),
    }
}

// ── Skip-when-already-target heuristic ──────────────────────────────

pub(crate) fn should_skip_translation(content: &str, target_lang: &str) -> bool {
    if is_cjk_target(target_lang) {
        let cjk = cjk_ratio(content);
        let ascii_alpha = ascii_alpha_ratio(content);
        // Keep mixed English/Chinese skill docs translatable. Only skip content
        // that already reads predominantly CJK, while allowing technical names
        // like React, Tauri, CLI, etc. to remain as-is.
        return cjk > 0.55 && ascii_alpha < 0.35;
    }
    let target = target_lang.to_ascii_lowercase();
    if target == "en" {
        // Target English: skip if the text already looks predominantly ASCII letters.
        return ascii_alpha_ratio(content) > 0.75 && cjk_ratio(content) < 0.05;
    }
    false
}

pub(crate) fn is_cjk_target(target_lang: &str) -> bool {
    let target = target_lang.to_ascii_lowercase();
    target.starts_with("zh") || target == "ja" || target == "ko"
}

fn ascii_alpha_ratio(s: &str) -> f32 {
    let total = s.chars().filter(|c| !c.is_whitespace()).count().max(1);
    let ascii_alpha = s.chars().filter(|c| c.is_ascii_alphabetic()).count();
    ascii_alpha as f32 / total as f32
}

pub(crate) fn cjk_ratio(s: &str) -> f32 {
    let total = s.chars().filter(|c| !c.is_whitespace()).count().max(1);
    let cjk = s
        .chars()
        .filter(|c| {
            matches!(*c as u32,
                0x3040..=0x30FF      // Hiragana + Katakana
                | 0x3400..=0x4DBF    // CJK Ext A
                | 0x4E00..=0x9FFF    // CJK Unified
                | 0xAC00..=0xD7AF    // Hangul Syllables
                | 0xF900..=0xFAFF    // CJK Compatibility Ideographs
            )
        })
        .count();
    cjk as f32 / total as f32
}
