use super::*;
use crate::skill_lock::{SkillLockEntry, SourceType};
use crate::test_sandbox::Sandbox;

fn hub() -> PathBuf {
    ss_core::infra::paths::hub_skills_dir()
}

fn skill_md(name: &str) -> String {
    format!("---\nname: {name}\ndescription: {name} does things\n---\n\n# {name}\n")
}

fn hub_skill(name: &str) {
    let dir = hub().join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("SKILL.md"), skill_md(name)).unwrap();
}

fn write_tar_gz(path: &Path, entries: &[(&str, &[u8])]) {
    let file = std::fs::File::create(path).unwrap();
    let mut tar = Builder::new(GzEncoder::new(file, Compression::default()));
    for (name, content) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, name, *content).unwrap();
    }
    tar.into_inner().unwrap().finish().unwrap();
}

fn residue() -> Vec<String> {
    std::fs::read_dir(hub())
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name.starts_with('.'))
                .collect()
        })
        .unwrap_or_default()
}

fn git_entry() -> SkillLockEntry {
    SkillLockEntry {
        source: "owner/repo".into(),
        source_type: SourceType::Github,
        source_url: "https://github.com/owner/repo.git".into(),
        git_ref: None,
        skill_path: Some("alpha".into()),
        skill_folder_hash: Some("abc".into()),
        installed_at: String::new(),
        updated_at: String::new(),
        extra: Default::default(),
    }
}

#[test]
fn bundle_import_records_a_bundle_lock_entry_replacing_the_git_source() {
    let sandbox = Sandbox::new();
    hub_skill("alpha");
    let bundle = sandbox.root().join("alpha.ags");
    export_bundle("alpha", Some(&bundle.to_string_lossy())).unwrap();
    crate::skill_lock::mutate(|lock| lock.upsert("alpha", git_entry())).unwrap();

    let result = import_bundle(&bundle.to_string_lossy(), true).unwrap();

    assert!(result.replaced);
    let entry = crate::skill_lock::load().skills.remove("alpha").unwrap();
    assert_eq!(entry.source_type, SourceType::Bundle);
    assert!(entry.source_url.ends_with("alpha.ags"));
    assert_eq!(entry.skill_folder_hash, None);
    assert!(residue().is_empty(), "left behind: {:?}", residue());
}

#[test]
fn a_rejected_bundle_leaves_no_staging_in_the_canonical_root() {
    let sandbox = Sandbox::new();
    std::fs::create_dir_all(hub()).unwrap();
    let body = b"no frontmatter here".as_slice();
    let manifest = BundleManifest {
        format_version: FORMAT_VERSION,
        name: "broken".into(),
        description: String::new(),
        version: "1.0.0".into(),
        author: String::new(),
        created_at: String::new(),
        files: vec!["SKILL.md".into()],
        checksum: import::checksum_for_test(&[body]),
    };
    let manifest = serde_json::to_vec(&manifest).unwrap();
    let bundle = sandbox.root().join("broken.ags");
    write_tar_gz(&bundle, &[(MANIFEST_NAME, &manifest), ("SKILL.md", body)]);

    let error = import_bundle(&bundle.to_string_lossy(), false).unwrap_err();

    assert!(
        format!("{error:#}").contains("not installable"),
        "{error:#}"
    );
    assert!(!hub().join("broken").exists());
    assert!(residue().is_empty(), "left behind: {:?}", residue());
    assert!(crate::skill_lock::load().skills.is_empty());
}

#[test]
fn a_tampered_multi_bundle_installs_nothing() {
    let sandbox = Sandbox::new();
    let manifest = MultiManifest {
        format_version: FORMAT_VERSION,
        created_at: String::new(),
        skills: vec![MultiManifestEntry {
            name: "alpha".into(),
            description: String::new(),
            file_count: 1,
        }],
        checksum: import::checksum_for_test(&[b"original".as_slice()]),
    };
    let manifest = serde_json::to_vec(&manifest).unwrap();
    let body = skill_md("alpha");
    let bundle = sandbox.root().join("deck.agd");
    write_tar_gz(
        &bundle,
        &[
            (MULTI_MANIFEST_NAME, &manifest),
            ("alpha/SKILL.md", body.as_bytes()),
        ],
    );

    let error = import_multi_bundle(&bundle.to_string_lossy(), false).unwrap_err();

    assert!(error.to_string().contains("Checksum mismatch"), "{error:#}");
    assert!(!hub().join("alpha").exists());
}

#[test]
fn one_invalid_skill_rolls_back_the_whole_deck() {
    let sandbox = Sandbox::new();
    let good = skill_md("alpha");
    let bad = b"not a skill".as_slice();
    let manifest = MultiManifest {
        format_version: FORMAT_VERSION,
        created_at: String::new(),
        skills: ["alpha", "beta"]
            .into_iter()
            .map(|name| MultiManifestEntry {
                name: name.into(),
                description: String::new(),
                file_count: 1,
            })
            .collect(),
        checksum: import::checksum_for_test(&[good.as_bytes(), bad]),
    };
    let manifest = serde_json::to_vec(&manifest).unwrap();
    let bundle = sandbox.root().join("deck.agd");
    write_tar_gz(
        &bundle,
        &[
            (MULTI_MANIFEST_NAME, &manifest),
            ("alpha/SKILL.md", good.as_bytes()),
            ("beta/SKILL.md", bad),
        ],
    );

    assert!(import_multi_bundle(&bundle.to_string_lossy(), false).is_err());
    assert!(
        !hub().join("alpha").exists(),
        "alpha must roll back with beta"
    );
    assert!(residue().is_empty(), "left behind: {:?}", residue());
}

#[test]
fn an_oversized_member_is_refused_before_extraction() {
    let sandbox = Sandbox::new();
    let huge = vec![0u8; (import::MAX_ENTRY_BYTES + 1) as usize];
    let bundle = sandbox.root().join("bomb.ags");
    write_tar_gz(&bundle, &[(MANIFEST_NAME, b"{}"), ("SKILL.md", &huge)]);

    let error = import_bundle(&bundle.to_string_lossy(), false).unwrap_err();

    assert!(error.to_string().contains("exceeds"), "{error:#}");
    assert!(!hub().exists() || residue().is_empty());
}
