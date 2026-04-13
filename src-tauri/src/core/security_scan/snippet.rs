/// Safely truncate a line snippet to `max_chars` without panicking on multi-byte UTF-8.
pub(crate) fn safe_snippet(line: &str, max_chars: usize) -> String {
    if line.len() <= max_chars {
        return line.to_string();
    }
    let end = line
        .char_indices()
        .take_while(|(i, _)| *i <= max_chars)
        .last()
        .map(|(i, _)| i)
        .unwrap_or(0);
    format!("{}...", &line[..end])
}
