use super::ai_search_by_keywords;

/// Integration test: verifies keyword_skill_map is correctly populated.
/// Uses real network calls, so marked #[ignore] for CI.
/// Run with: cargo test ai_search_returns_keyword_map -- --ignored --nocapture
#[tokio::test]
#[ignore]
async fn ai_search_returns_keyword_map() {
    let keywords = vec!["react".to_string(), "typescript".to_string()];
    let result = ai_search_by_keywords(&keywords).await.unwrap();

    eprintln!("Total skills returned: {}", result.skills.len());
    eprintln!(
        "keyword_skill_map keys: {:?}",
        result.keyword_skill_map.keys().collect::<Vec<_>>()
    );

    // 1) Should return some skills
    assert!(
        !result.skills.is_empty(),
        "Expected at least 1 skill from search"
    );

    // 2) keyword_skill_map should have entries for each keyword
    for kw in &keywords {
        let names = result.keyword_skill_map.get(kw);
        assert!(
            names.is_some(),
            "keyword_skill_map missing entry for '{}'",
            kw
        );
        let names = names.unwrap();
        eprintln!(
            "Keyword '{}' found {} skills: {:?}",
            kw,
            names.len(),
            &names[..names.len().min(5)]
        );
        assert!(
            !names.is_empty(),
            "Expected at least 1 skill for keyword '{}'",
            kw
        );
    }

    // 3) total_count matches skills vec length
    assert_eq!(
        result.total_count as usize,
        result.skills.len(),
        "total_count should match skills.len()"
    );

    // 4) Every returned skill should be attributed to at least one keyword
    let all_attributed: std::collections::HashSet<String> = result
        .keyword_skill_map
        .values()
        .flat_map(|v| v.iter().cloned())
        .collect();
    for skill in &result.skills {
        assert!(
            all_attributed.contains(&skill.name),
            "Skill '{}' not found in any keyword_skill_map entry",
            skill.name
        );
    }

    // 5) Serializes to JSON correctly (simulates what Tauri sends to frontend)
    let json = serde_json::to_value(&result).unwrap();
    assert!(
        json["keyword_skill_map"].is_object(),
        "keyword_skill_map should serialize as JSON object"
    );
    assert!(
        json["skills"].is_array(),
        "skills should serialize as JSON array"
    );
    let map = json["keyword_skill_map"].as_object().unwrap();
    eprintln!("JSON keyword_skill_map: {} keys", map.len());
    for (k, v) in map {
        eprintln!("  '{}': {} skills", k, v.as_array().unwrap().len());
    }
}

#[tokio::test]
#[ignore]
async fn ai_search_empty_keywords_returns_empty() {
    let result = ai_search_by_keywords(&[]).await.unwrap();
    assert!(result.skills.is_empty());
    assert_eq!(result.total_count, 0);
    assert!(result.keyword_skill_map.is_empty());
}
