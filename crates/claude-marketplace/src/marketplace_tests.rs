use crate::marketplace::{
    MarketplaceManifest, PluginEntry, PluginSource, RemoteSource, MARKETPLACE_SCHEMA_URL,
};

/// obra/superpowers-marketplace 的真实清单节选（metadata 嵌套形态 + url 远程源）。
const SUPERPOWERS_FIXTURE: &str = r##"{
  "name": "superpowers-marketplace",
  "owner": {
    "name": "Jesse Vincent",
    "email": "jesse@fsck.com"
  },
  "metadata": {
    "description": "Skills, workflows, and productivity tools",
    "version": "1.0.13"
  },
  "plugins": [
    {
      "name": "superpowers",
      "source": {
        "source": "url",
        "url": "https://github.com/obra/superpowers.git"
      },
      "description": "Core skills library: TDD, debugging, collaboration patterns, and proven techniques",
      "version": "6.3.0",
      "strict": true
    }
  ]
}"##;

/// anthropics/claude-code 官方清单节选（顶层 version/description 形态 + author/category）。
const OFFICIAL_FIXTURE: &str = r##"{
  "$schema": "https://json.schemastore.org/claude-code-marketplace.json",
  "name": "claude-code-plugins",
  "version": "1.0.0",
  "description": "Bundled plugins for Claude Code including Agent SDK development tools, PR review toolkit, and commit workflows",
  "owner": {
    "name": "Anthropic",
    "email": "support@anthropic.com"
  },
  "plugins": [
    {
      "name": "agent-sdk-dev",
      "description": "Development kit for working with the Claude Agent SDK",
      "source": "./plugins/agent-sdk-dev",
      "category": "development"
    }
  ]
}"##;

#[test]
fn parses_superpowers_metadata_style_fixture() {
    let manifest: MarketplaceManifest = serde_json::from_str(SUPERPOWERS_FIXTURE).unwrap();
    assert_eq!(manifest.name, "superpowers-marketplace");
    assert_eq!(manifest.owner.name, "Jesse Vincent");
    assert_eq!(manifest.owner.email.as_deref(), Some("jesse@fsck.com"));
    let metadata = manifest.metadata.as_ref().unwrap();
    assert_eq!(metadata.version.as_deref(), Some("1.0.13"));
    assert!(manifest.version.is_none());
    assert!(manifest.description.is_none());

    let entry = &manifest.plugins[0];
    assert_eq!(entry.name, "superpowers");
    assert_eq!(entry.version.as_deref(), Some("6.3.0"));
    assert_eq!(entry.strict, Some(true));
    match &entry.source {
        PluginSource::Remote(RemoteSource::Url { url, git_ref }) => {
            assert_eq!(url, "https://github.com/obra/superpowers.git");
            assert!(git_ref.is_none());
        }
        other => panic!("expected url remote source, got {other:?}"),
    }
}

#[test]
fn parses_official_top_level_style_fixture() {
    let manifest: MarketplaceManifest = serde_json::from_str(OFFICIAL_FIXTURE).unwrap();
    assert_eq!(manifest.schema.as_deref(), Some(MARKETPLACE_SCHEMA_URL));
    assert_eq!(manifest.name, "claude-code-plugins");
    assert_eq!(manifest.version.as_deref(), Some("1.0.0"));
    assert!(manifest.metadata.is_none());

    let entry = &manifest.plugins[0];
    assert_eq!(entry.category.as_deref(), Some("development"));
    assert_eq!(entry.source.as_relative(), Some("./plugins/agent-sdk-dev"));
    assert!(entry.author.is_none());
    assert!(entry.version.is_none());
}

#[test]
fn round_trips_official_fixture_with_stable_json_shape() {
    let manifest: MarketplaceManifest = serde_json::from_str(OFFICIAL_FIXTURE).unwrap();
    let emitted = serde_json::to_value(&manifest).unwrap();
    let expected: serde_json::Value = serde_json::from_str(OFFICIAL_FIXTURE).unwrap();
    // 生成路径只写已知字段且顺序固定;对官方 fixture 应得到键集合一致的等价 JSON。
    assert_eq!(emitted, expected);
}

type SourceCheck = fn(&PluginSource) -> bool;

#[test]
fn parses_remote_source_variants() {
    let cases: &[(&str, SourceCheck)] = &[
        (
            r#"{"source": "github", "repo": "obra/superpowers"}"#,
            |source| {
                matches!(
                    source,
                    PluginSource::Remote(RemoteSource::Github { repo, git_ref })
                        if repo == "obra/superpowers" && git_ref.is_none()
                )
            },
        ),
        (
            r#"{"source": "git-subdir", "url": "https://example.com/repo.git", "path": "skills/x", "ref": "v2"}"#,
            |source| {
                matches!(
                    source,
                    PluginSource::Remote(RemoteSource::GitSubdir { url, path, git_ref })
                        if url == "https://example.com/repo.git"
                            && path == "skills/x"
                            && git_ref.as_deref() == Some("v2")
                )
            },
        ),
        (
            r#"{"source": "archive", "url": "https://example.com/p.zip"}"#,
            |source| {
                matches!(
                    source,
                    PluginSource::Remote(RemoteSource::Archive { url })
                        if url == "https://example.com/p.zip"
                )
            },
        ),
        (
            r#"{"source": "npm", "package": "some-plugin"}"#,
            |source| {
                matches!(
                    source,
                    PluginSource::Remote(RemoteSource::Npm { package })
                        if package == "some-plugin"
                )
            },
        ),
    ];
    for (raw, check) in cases {
        let source: PluginSource = serde_json::from_str(raw).unwrap();
        assert!(check(&source), "variant did not parse as expected: {raw}");
    }
}

#[test]
fn unknown_source_discriminant_falls_back_to_other() {
    let source: PluginSource =
        serde_json::from_str(r#"{"source": "weird-future-kind", "x": 1}"#).unwrap();
    match &source {
        PluginSource::Other(value) => assert_eq!(value.get("x"), Some(&serde_json::json!(1))),
        other => panic!("expected Other fallback, got {other:?}"),
    }
}

#[test]
fn emits_relative_entry_source_as_plain_string() {
    let entry = PluginEntry::relative("my-skill", "./plugins/my-skill");
    let value = serde_json::to_value(&entry).unwrap();
    assert_eq!(value["source"], serde_json::json!("./plugins/my-skill"));
    // 可选字段缺省时完全不出现,保持生成 JSON 最小。
    assert!(value.get("version").is_none());
    assert!(value.get("strict").is_none());
    assert!(value.get("author").is_none());
}
