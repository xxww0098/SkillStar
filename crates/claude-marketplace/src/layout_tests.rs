use std::fs;
use std::path::{Path, PathBuf};

use crate::layout::{PluginPayload, SkillDir, write_marketplace};
use crate::marketplace::{MarketplaceManifest, PluginEntry, PluginOwner};
use crate::plugin::PluginManifest;
use crate::validate::MarketplaceError;
use tempfile::TempDir;

fn make_skill(root: &Path, name: &str, body: &str) -> PathBuf {
    let dir = root.join(name);
    fs::create_dir_all(dir.join("assets")).unwrap();
    fs::write(dir.join("SKILL.md"), body).unwrap();
    fs::write(dir.join("assets").join("data.txt"), "asset").unwrap();
    dir
}

fn make_marketplace() -> MarketplaceManifest {
    let mut manifest =
        MarketplaceManifest::new("team-channel", PluginOwner::new("acme-org"));
    manifest.description = Some("Skills from a SkillStar shared channel".into());
    manifest.version = Some("42".into());
    manifest.plugins = vec![
        PluginEntry::relative("pr-review", "./plugins/pr-review"),
        PluginEntry::relative("tdd", "./plugins/tdd"),
    ];
    manifest
}

#[test]
fn writes_self_contained_marketplace_tree() {
    let tmp = TempDir::new().unwrap();
    let skills = tmp.path().join("sources");
    let pr = make_skill(&skills, "pr-review-skill", "# PR Review\n");
    let tdd = make_skill(&skills, "tdd-skill", "# TDD\n");
    let out = tmp.path().join("exported-market");

    let mut manifest = make_marketplace();
    manifest.plugins[0].description = Some("Review pull requests".into());

    let payloads = vec![
        PluginPayload {
            entry: manifest.plugins[0].clone(),
            manifest: {
                let mut plugin = PluginManifest::new("pr-review");
                plugin.description = Some("Review pull requests".into());
                plugin.version = Some("42".into());
                plugin
            },
            skills: vec![SkillDir::new("pr-review", &pr)],
        },
        PluginPayload {
            entry: manifest.plugins[1].clone(),
            manifest: PluginManifest::new("tdd"),
            skills: vec![SkillDir::new("tdd", &tdd)],
        },
    ];

    let written = write_marketplace(&out, &manifest, &payloads).unwrap();
    assert_eq!(written.root, out);
    assert_eq!(written.plugins, ["pr-review", "tdd"]);
    // marketplace.json + 2 plugin.json + 每个 skill 2 文件
    assert_eq!(written.files, 7);

    let marketplace_json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(out.join(".claude-plugin").join("marketplace.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(marketplace_json["name"], "team-channel");
    assert_eq!(marketplace_json["owner"]["name"], "acme-org");
    assert_eq!(marketplace_json["version"], "42");
    assert_eq!(marketplace_json["plugins"].as_array().map(Vec::len), Some(2));
    assert_eq!(marketplace_json["plugins"][0]["source"], "./plugins/pr-review");

    let plugin_json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            out.join("plugins")
                .join("pr-review")
                .join(".claude-plugin")
                .join("plugin.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(plugin_json["name"], "pr-review");
    assert_eq!(plugin_json["version"], "42");

    let skill_md = fs::read_to_string(
        out.join("plugins")
            .join("pr-review")
            .join("skills")
            .join("pr-review")
            .join("SKILL.md"),
    )
    .unwrap();
    assert_eq!(skill_md, "# PR Review\n");
    assert_eq!(
        fs::read_to_string(
            out.join("plugins")
                .join("tdd")
                .join("skills")
                .join("tdd")
                .join("assets")
                .join("data.txt")
        )
        .unwrap(),
        "asset"
    );
}

#[test]
fn output_is_deterministic() {
    let tmp = TempDir::new().unwrap();
    let skills = tmp.path().join("sources");
    let skill = make_skill(&skills, "some-skill", "# Some\n");
    let manifest = make_marketplace();
    let payloads = vec![PluginPayload {
        entry: manifest.plugins[0].clone(),
        manifest: PluginManifest::new("pr-review"),
        skills: vec![SkillDir::new("pr-review", &skill)],
    }];
    let first = tmp.path().join("export-1");
    let second = tmp.path().join("export-2");
    write_marketplace(&first, &manifest, &payloads).unwrap();
    write_marketplace(&second, &manifest, &payloads).unwrap();
    assert_eq!(
        fs::read_to_string(first.join(".claude-plugin").join("marketplace.json")).unwrap(),
        fs::read_to_string(second.join(".claude-plugin").join("marketplace.json")).unwrap()
    );
}

#[test]
fn replaces_existing_empty_output_dir_but_rejects_non_empty() {
    let tmp = TempDir::new().unwrap();
    let skill = make_skill(tmp.path(), "some-skill", "# Some\n");
    let manifest = make_marketplace();
    let payloads = vec![PluginPayload {
        entry: manifest.plugins[0].clone(),
        manifest: PluginManifest::new("pr-review"),
        skills: vec![SkillDir::new("pr-review", &skill)],
    }];

    let empty = tmp.path().join("empty-out");
    fs::create_dir_all(&empty).unwrap();
    write_marketplace(&empty, &manifest, &payloads).unwrap();
    assert!(empty.join(".claude-plugin").join("marketplace.json").is_file());

    let occupied = tmp.path().join("occupied-out");
    fs::create_dir_all(&occupied).unwrap();
    fs::write(occupied.join("keep.txt"), "keep").unwrap();
    let error = write_marketplace(&occupied, &manifest, &payloads).unwrap_err();
    assert!(matches!(error, MarketplaceError::OutputNotEmpty(_)));
    assert!(occupied.join("keep.txt").is_file(), "existing files must survive");
    assert!(!tmp.path().join(".occupied-out.claude-marketplace-staging").exists());
}

#[test]
fn rejects_payload_rule_violations() {
    let tmp = TempDir::new().unwrap();
    let skill = make_skill(tmp.path(), "some-skill", "# Some\n");
    let manifest = make_marketplace();
    let out = tmp.path().join("out");

    // manifest.name 与 entry.name 不一致
    let mismatched = vec![PluginPayload {
        entry: PluginEntry::relative("pr-review", "./plugins/pr-review"),
        manifest: PluginManifest::new("other-name"),
        skills: vec![SkillDir::new("pr-review", &skill)],
    }];
    assert!(matches!(
        write_marketplace(&out, &manifest, &mismatched).unwrap_err(),
        MarketplaceError::EntryNameMismatch { .. }
    ));

    // 远程源不能用于生成
    let mut remote_entry = PluginEntry::relative("tdd", "./plugins/tdd");
    remote_entry.source = crate::marketplace::PluginSource::Remote(
        crate::marketplace::RemoteSource::Github {
            repo: "acme/channel".into(),
            git_ref: None,
        },
    );
    let remote = vec![PluginPayload {
        entry: remote_entry,
        manifest: PluginManifest::new("tdd"),
        skills: vec![SkillDir::new("tdd", &skill)],
    }];
    assert!(matches!(
        write_marketplace(&out, &manifest, &remote).unwrap_err(),
        MarketplaceError::NotRelativeSource(_)
    ));

    // 源路径互相嵌套
    let nested = vec![
        PluginPayload {
            entry: PluginEntry::relative("pr-review", "./plugins/a"),
            manifest: PluginManifest::new("pr-review"),
            skills: vec![SkillDir::new("pr-review", &skill)],
        },
        PluginPayload {
            entry: PluginEntry::relative("tdd", "./plugins/a/b"),
            manifest: PluginManifest::new("tdd"),
            skills: vec![SkillDir::new("tdd", &skill)],
        },
    ];
    assert!(matches!(
        write_marketplace(&out, &manifest, &nested).unwrap_err(),
        MarketplaceError::OverlappingSourcePaths(_, _)
    ));

    // 源路径占用了根 .claude-plugin
    let reserved = vec![PluginPayload {
        entry: PluginEntry::relative("pr-review", "./.claude-plugin"),
        manifest: PluginManifest::new("pr-review"),
        skills: vec![SkillDir::new("pr-review", &skill)],
    }];
    assert!(matches!(
        write_marketplace(&out, &manifest, &reserved).unwrap_err(),
        MarketplaceError::InvalidSourcePath { .. }
    ));

    // 同一 payload 里技能目录重名
    let duplicated = vec![PluginPayload {
        entry: PluginEntry::relative("pr-review", "./plugins/pr-review"),
        manifest: PluginManifest::new("pr-review"),
        skills: vec![
            SkillDir::new("pr-review", &skill),
            SkillDir::new("pr-review", &skill),
        ],
    }];
    assert!(matches!(
        write_marketplace(&out, &manifest, &duplicated).unwrap_err(),
        MarketplaceError::DuplicateSkillDir(_)
    ));

    assert!(fs::read_dir(&out).is_err(), "失败路径不得留下输出目录");
}

#[test]
fn rejects_symlinks_and_oversize_files_in_skill_content() {
    let tmp = TempDir::new().unwrap();
    let manifest = make_marketplace();
    let out = tmp.path().join("out");

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let skill = make_skill(tmp.path(), "linked-skill", "# Linked\n");
        symlink("SKILL.md", skill.join("shortcut")).unwrap();
        let payloads = vec![PluginPayload {
            entry: PluginEntry::relative("pr-review", "./plugins/pr-review"),
            manifest: PluginManifest::new("pr-review"),
            skills: vec![SkillDir::new("pr-review", &skill)],
        }];
        let error = write_marketplace(&out, &manifest, &payloads).unwrap_err();
        assert!(matches!(error, MarketplaceError::UnsupportedSymlink(_)));
        fs::remove_file(skill.join("shortcut")).unwrap();
    }

    let big = tmp.path().join("big-skill");
    fs::create_dir_all(&big).unwrap();
    let bytes = vec![0_u8; (crate::layout::COPY_MAX_FILE_BYTES + 1) as usize];
    fs::write(big.join("SKILL.md"), "# Big\n").unwrap();
    fs::write(big.join("blob.bin"), bytes).unwrap();
    let payloads = vec![PluginPayload {
        entry: PluginEntry::relative("pr-review", "./plugins/pr-review"),
        manifest: PluginManifest::new("pr-review"),
        skills: vec![SkillDir::new("pr-review", &big)],
    }];
    let error = write_marketplace(&out, &manifest, &payloads).unwrap_err();
    assert!(matches!(error, MarketplaceError::CopyLimit { .. }));
}
