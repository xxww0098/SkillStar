use super::archive::{safe_archive_path, unpack_archive};
use super::index::{
    index_candidates, indexes_to_try, is_candidate, parse_index, scope_miss_if_root_published,
    scope_of, valid_digest, ArtifactKind, FetchedIndex, IndexEntry, ParsedIndex, SCHEMA_V2,
};
use crate::skill_lock::{source_type_for, SourceType};
use crate::source_resolver::Source;

fn digest() -> String {
    format!("sha256:{}", "ab".repeat(32))
}

fn v2_skill(extra: &str) -> serde_json::Value {
    serde_json::json!({
        "$schema": SCHEMA_V2,
        "skills": [{
            "name": "pdf-tools",
            "description": "Read PDFs",
            "type": "skill-md",
            "url": "skills/pdf-tools/SKILL.md",
            "digest": digest(),
            "extra": extra
        }]
    })
}

#[test]
fn candidate_urls_skip_git_hosts_and_refs() {
    assert!(is_candidate("https://example.com/docs"));
    assert!(is_candidate("http://skills.example.com"));
    assert!(!is_candidate("https://github.com/owner/repo"));
    assert!(!is_candidate("https://www.github.com/owner/repo"));
    assert!(!is_candidate("https://gitlab.com/owner/repo"));
    assert!(!is_candidate("https://example.com/repo.git"));
    assert!(!is_candidate("https://example.com/docs#v1"));
    assert!(!is_candidate("owner/repo"));
}

#[test]
fn scoped_miss_does_not_select_the_root_index() {
    let scope = scope_of("https://example.com/s/my-list").expect("scoped");
    let root = FetchedIndex {
        resolved_base: "https://example.com".to_string(),
        entries: vec![IndexEntry::V2 {
            name: "host-catalog".to_string(),
            kind: ArtifactKind::SkillMd,
            artifact_url: "https://example.com/SKILL.md".to_string(),
            digest: digest(),
        }],
    };
    let hits = vec![root];
    assert!(indexes_to_try(Some(&scope), &hits).is_empty());
    let miss = scope_miss_if_root_published(Some(&scope), &hits).expect("root published");
    assert!(miss
        .message()
        .contains("Not falling back to the root skills index"));
    assert!(miss.message().contains("/s/my-list"));
    let secret = scope_of("https://user:secret@example.com/s/my-list").expect("auth scope");
    let leaked = scope_miss_if_root_published(
        Some(&secret),
        &[FetchedIndex {
            resolved_base: secret.root_base.clone(),
            entries: Vec::new(),
        }],
    )
    .expect("root published")
    .message();
    assert!(!leaked.contains("secret"), "{leaked}");
}

#[test]
fn unscoped_url_may_use_the_root_index() {
    assert!(scope_of("https://example.com").is_none());
    let root = FetchedIndex {
        resolved_base: "https://example.com".to_string(),
        entries: Vec::new(),
    };
    assert_eq!(indexes_to_try(None, &[root]).len(), 1);
}

#[test]
fn scoped_candidates_try_the_path_before_the_root() {
    let candidates = index_candidates("https://example.com/s/my-list");
    assert_eq!(
        candidates[0].index_url,
        "https://example.com/s/my-list/.well-known/agent-skills/index.json"
    );
    assert_eq!(
        candidates[1].index_url,
        "https://example.com/.well-known/agent-skills/index.json"
    );
    assert_ne!(candidates[0].resolved_base, candidates[1].resolved_base);
}

#[test]
fn discovery_v2_requires_a_digest_and_rejects_unknown_schemas() {
    let index_url = "https://example.com/s/my-list/.well-known/agent-skills/index.json";
    let parsed = parse_index(
        &v2_skill(""),
        index_url,
        "https://example.com/s/my-list",
        ".well-known/agent-skills",
    )
    .expect("v2");
    let ParsedIndex::Entries(parsed) = parsed else {
        panic!("expected entries");
    };
    let IndexEntry::V2 {
        artifact_url,
        digest,
        ..
    } = &parsed[0]
    else {
        panic!("expected v2");
    };
    assert_eq!(
        artifact_url,
        "https://example.com/s/my-list/.well-known/agent-skills/skills/pdf-tools/SKILL.md"
    );
    assert!(valid_digest(digest));

    let mut missing = v2_skill("");
    missing["skills"][0]
        .as_object_mut()
        .unwrap()
        .remove("digest");
    assert!(matches!(
        parse_index(&missing, index_url, "https://example.com/s/my-list", ".well-known/agent-skills"),
        Some(ParsedIndex::Entries(entries)) if entries.is_empty()
    ));

    let mut unknown = v2_skill("");
    unknown["$schema"] = serde_json::json!("https://example.invalid/schema.json");
    assert_eq!(
        parse_index(
            &unknown,
            index_url,
            "https://example.com/s/my-list",
            ".well-known/agent-skills"
        ),
        Some(ParsedIndex::Rejected)
    );
}

#[test]
fn legacy_index_without_schema_is_accepted() {
    let raw = serde_json::json!({"skills": [{"name": "pdf-tools", "description": "Read PDFs", "files": ["SKILL.md"]}]});
    let parsed = parse_index(
        &raw,
        "https://example.com/.well-known/skills/index.json",
        "https://example.com",
        ".well-known/skills",
    )
    .expect("legacy");
    let ParsedIndex::Entries(parsed) = parsed else {
        panic!("expected legacy entries");
    };
    assert!(matches!(parsed[0], IndexEntry::V1 { .. }));
}

#[test]
fn archive_paths_reject_links_and_escape() {
    assert!(safe_archive_path("SKILL.md").is_some());
    assert!(safe_archive_path("scripts/run.sh").is_some());
    assert!(safe_archive_path("../SKILL.md").is_none());
    assert!(safe_archive_path("/etc/passwd").is_none());
    assert!(safe_archive_path("C:/Windows/system.ini").is_none());
    assert!(safe_archive_path("foo\\bar").is_none());
}

#[test]
fn zip_unpack_writes_skill_md_and_rejects_parent_paths() {
    let parent =
        std::env::temp_dir().join(format!("skillstar-wellknown-test-{}", uuid::Uuid::new_v4()));
    let dir = parent.join("skill");
    std::fs::create_dir_all(&dir).unwrap();
    unpack_archive(
        &stored_zip("SKILL.md", b"---\nname: pdf-tools\n---\n"),
        "https://example.com/skill.zip",
        &dir,
    )
    .unwrap();
    assert!(dir.join("SKILL.md").is_file());
    let err = unpack_archive(
        &stored_zip("../SKILL.md", b"nope"),
        "https://example.com/bad.zip",
        &dir,
    )
    .unwrap_err();
    assert!(err.to_string().contains("Unsafe archive path"), "{err}");
    assert!(!parent.join("SKILL.md").exists());
    let _ = std::fs::remove_dir_all(&parent);
}

#[test]
fn well_known_source_is_not_a_git_update() {
    let spec = Source {
        repo_url: "https://example.com/docs".to_string(),
        short: "wellknown/example.com/docs".to_string(),
        git_ref: None,
        subpath: None,
        skill_filter: None,
        well_known: true,
    };
    assert_eq!(source_type_for(&spec), SourceType::WellKnown);
    assert!(!SourceType::WellKnown.is_updatable());
    let parsed: Source =
        serde_json::from_str("{\"source_url\":\"https://github.com/o/r.git\",\"source\":\"o/r\"}")
            .unwrap();
    assert!(!parsed.well_known);
}

fn stored_zip(name: &str, data: &[u8]) -> Vec<u8> {
    let name = name.as_bytes();
    let crc = crc32(data);
    let mut local = Vec::new();
    local.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
    local.extend_from_slice(&20u16.to_le_bytes());
    local.extend_from_slice(&0u16.to_le_bytes());
    local.extend_from_slice(&0u16.to_le_bytes());
    local.extend_from_slice(&0u16.to_le_bytes());
    local.extend_from_slice(&0u16.to_le_bytes());
    local.extend_from_slice(&crc.to_le_bytes());
    local.extend_from_slice(&(data.len() as u32).to_le_bytes());
    local.extend_from_slice(&(data.len() as u32).to_le_bytes());
    local.extend_from_slice(&(name.len() as u16).to_le_bytes());
    local.extend_from_slice(&0u16.to_le_bytes());
    local.extend_from_slice(name);
    local.extend_from_slice(data);
    let mut central = Vec::new();
    central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
    central.extend_from_slice(&20u16.to_le_bytes());
    central.extend_from_slice(&20u16.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&crc.to_le_bytes());
    central.extend_from_slice(&(data.len() as u32).to_le_bytes());
    central.extend_from_slice(&(data.len() as u32).to_le_bytes());
    central.extend_from_slice(&(name.len() as u16).to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u32.to_le_bytes());
    central.extend_from_slice(&0u32.to_le_bytes());
    central.extend_from_slice(name);
    let mut out = local;
    let central_offset = out.len() as u32;
    out.extend_from_slice(&central);
    let central_size = central.len() as u32;
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&central_size.to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}
