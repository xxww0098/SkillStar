pub(super) fn is_rate_limited_response(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.contains("rate limit")
        || lower.contains("secondary rate")
        || lower.contains("abuse detection")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_primary_and_secondary_rate_limit_messages() {
        assert!(is_rate_limited_response("API rate limit exceeded"));
        assert!(is_rate_limited_response(
            "You exceeded a secondary rate limit"
        ));
        assert!(!is_rate_limited_response("Resource not accessible"));
    }
}
