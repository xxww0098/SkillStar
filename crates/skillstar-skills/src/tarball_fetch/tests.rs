use super::*;

use flate2::write::GzEncoder;
use std::fs;
use tar::Builder;

fn build_archive(path: &Path, entries: &[(&str, &str)]) {
    let file = fs::File::create(path).unwrap();
    let mut builder = Builder::new(GzEncoder::new(file, flate2::Compression::default()));
    for (name, contents) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(if name.ends_with(".sh") { 0o755 } else { 0o644 });
        header.set_cksum();
        builder.append_data(&mut header, name, contents.as_bytes()).unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap();
}

/// Build a synthetic cache entry from a local archive — the offline core of
/// the tarball fallback — and verify the resulting git shape.
#[test]
fn assembles_synthetic_repo_from_archive_subset() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("repo.tar.gz");
    build_archive(
        &archive,
        &[
            ("pack-HEAD/.claude-plugin/marketplace.json", "{ }"),
            ("pack-HEAD/.agents/skills/impeccable/SKILL.md", "---\nname: impeccable\n---\n"),
            ("pack-HEAD/.cursor/skills/impeccable/SKILL.md", "---\nname: impeccable\n---\n"),
            ("pack-HEAD/crates/engine/src/lib.rs", "pub fn f() {}"),
            ("pack-HEAD/README.md", "# pack"),
        ],
    );

    let dest = temp.path().join("cache-entry");
    assemble_from_archive(
        &archive,
        &dest,
        &[".agents/skills/impeccable".to_string()],
        "https://github.com/pbakaus/impeccable.git",
    )
    .expect("assembly succeeds");

    // Only the planned directory (plus nothing else) landed on disk.
    assert!(dest.join(".agents/skills/impeccable/SKILL.md").is_file());
    assert!(!dest.join(".cursor/skills/impeccable").exists());
    assert!(!dest.join("crates").exists());
    assert!(!dest.join("README.md").exists());

    // The synthetic repository shape every downstream path expects.
    assert!(dest.join(".git").exists());
    assert!(is_tarball_cache(&dest));
    assert_eq!(
        run_local_git(&dest, &["remote", "get-url", "origin"]).unwrap().trim(),
        "https://github.com/pbakaus/impeccable.git"
    );
    let tree_hash = crate::git::ops::compute_tree_hash(&dest).unwrap();
    assert_eq!(tree_hash.len(), 40);
}

#[test]
fn extraction_skips_links_and_outside_paths() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("repo.tar.gz");
    let file = fs::File::create(&archive).unwrap();
    let mut builder = Builder::new(GzEncoder::new(file, flate2::Compression::default()));
    let mut header = tar::Header::new_gnu();
    header.set_size(0);
    header.set_mode(0o644);
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_cksum();
    builder
        .append_data(&mut header, "pack-HEAD/skills/alpha/evil-link", std::io::empty())
        .unwrap();
    let mut header = tar::Header::new_gnu();
    header.set_size(3);
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append_data(&mut header, "pack-HEAD/skills/alpha/SKILL.md", "a".as_bytes())
        .unwrap();
    builder.into_inner().unwrap().finish().unwrap();

    let target = temp.path().join("out");
    fs::create_dir_all(&target).unwrap();
    extract_subset(&archive, &target, &["skills/alpha".to_string()]).unwrap();
    assert!(target.join("skills/alpha/SKILL.md").is_file());
    // Symlink entries never land: a link target is an escape hatch.
    assert!(!target.join("skills/alpha/evil-link").exists());
    // Nothing outside the plan directory landed.
    assert_eq!(fs::read_dir(&target).unwrap().count(), 1);
}

#[test]
fn extraction_fails_when_nothing_matches() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("repo.tar.gz");
    build_archive(&archive, &[("pack-HEAD/README.md", "# r")]);

    let target = temp.path().join("out");
    fs::create_dir_all(&target).unwrap();
    let error = extract_subset(&archive, &target, &["skills/alpha".to_string()]).unwrap_err();
    assert!(error.to_string().contains("No files matched"));
}

#[test]
fn executable_bits_survive_extraction_on_unix() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("repo.tar.gz");
    build_archive(
        &archive,
        &[("pack-HEAD/skills/alpha/run.sh", "#!/bin/sh\n")],
    );

    let target = temp.path().join("out");
    fs::create_dir_all(&target).unwrap();
    extract_subset(&archive, &target, &["skills/alpha".to_string()]).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = target.join("skills/alpha/run.sh").metadata().unwrap().permissions();
        let mode = permissions.mode();
        assert_ne!(mode & 0o111, 0, "execute bits must survive extraction");
    }
}

#[test]
fn prefix_and_matching_helpers() {
    assert_eq!(
        strip_archive_prefix("impeccable-HEAD/.claude/skills/x/SKILL.md"),
        Some(".claude/skills/x/SKILL.md".to_string())
    );
    assert_eq!(strip_archive_prefix("impeccable-main"), Some(String::new()));
    assert_eq!(strip_archive_prefix("nosegment"), Some(String::new()));

    let dirs = vec!["skills".to_string(), ".agents/skills/imp".to_string()];
    assert!(is_under_dirs("skills/alpha/SKILL.md", &dirs));
    assert!(is_under_dirs("skills", &dirs));
    assert!(is_under_dirs(".agents/skills/imp/SKILL.md", &dirs));
    assert!(!is_under_dirs("skills2/alpha", &dirs));
    assert!(!is_under_dirs(".agents/skills/other", &dirs));
}

#[test]
fn supports_and_archive_url() {
    assert!(supports_tarball("https://github.com/pbakaus/impeccable.git"));
    assert!(!supports_tarball("https://gitlab.com/foo/bar.git"));
    assert!(!supports_tarball("git@github.com:foo/bar.git"));

    let url = archive_url("https://github.com/pbakaus/impeccable.git", "HEAD").unwrap();
    assert_eq!(
        url,
        "https://codeload.github.com/pbakaus/impeccable/tar.gz/HEAD"
    );
    let pinned = archive_url("https://github.com/acme/skills.git", "v1.2.3").unwrap();
    assert_eq!(
        pinned,
        "https://codeload.github.com/acme/skills/tar.gz/v1.2.3"
    );
}
