//! Flattened catalog ids: the `provider/model` list consumers show.

use super::schema::Catalog;

/// Every catalog id as `provider/model`, providers then models, both in
/// key (alphabetical) order — the cache parses into ordered maps.
///
/// Empty names, and names holding `/`, cannot survive the flattened form and
/// are skipped. Saved groups are not catalog rows; the caller appends them.
pub fn catalog_ids() -> Vec<String> {
    flat_ids(&super::load())
}

pub(crate) fn flat_ids(catalog: &Catalog) -> Vec<String> {
    let mut ids = Vec::new();
    for (provider, entry) in &catalog.providers {
        if provider.is_empty() || provider.contains('/') {
            continue;
        }
        for model in entry.models.keys() {
            if model.is_empty() || model.contains('/') {
                continue;
            }
            ids.push(format!("{provider}/{model}"));
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(body: &[u8]) -> Catalog {
        serde_json::from_slice(body).unwrap_or_default()
    }

    #[test]
    fn ids_skip_empty_and_slashed_names_and_broken_rows() {
        let catalog = parse(
            br#"{"openai":{"models":{"gpt-test":{},"":{},"a/b":{}}},"":{"models":{"m":{}}},"p/q":{"models":{"m":{}}},"broken":{"models":"nope"},"list":[1]}"#,
        );
        assert_eq!(flat_ids(&catalog), vec!["openai/gpt-test".to_string()]);
        assert!(flat_ids(&parse(b"")).is_empty());
        assert!(flat_ids(&parse(b"[]")).is_empty());
        assert!(flat_ids(&parse(b"not json")).is_empty());
    }
}
