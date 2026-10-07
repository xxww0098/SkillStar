use super::*;
use crate::test_sandbox::Sandbox;

fn valid_skill(root: &Path, name: &str) {
    let dir = root.join(format!("skills/{name}"));
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {name} does things\n---\n\n# {name}\n"),
    )
    .unwrap();
    std::fs::write(dir.join("scripts/run.sh"), "echo hi\n").unwrap();
}

#[test]
fn adopt_folder_copies_the_full_skill_directory() {
    let _sandbox = Sandbox::new();
    let source = tempfile::tempdir().unwrap();
    valid_skill(source.path(), "demo");

    let result = adopt_folder(source.path().to_str().unwrap(), None).unwrap();
    assert_eq!(result.adopted.len(), 1);
    assert_eq!(result.adopted[0].name, "demo");

    // The whole directory (scripts included) is preserved in skills-local.
    let local = ss_core::infra::paths::local_skills_dir().join("demo");
    assert!(local.join("SKILL.md").exists());
    assert!(local.join("scripts/run.sh").exists());
    // And exposed through the hub link.
    assert!(is_local_skill("demo"));
    let sidecar_id = crate::local_identity::read_local_identity(&local)
        .unwrap()
        .expect("adopted Skill must mint a local identity sidecar");
    assert!(!sidecar_id.is_nil());
    assert!(!source.path().join("skills/demo/.skillstar").exists());
    // The source folder is untouched (adoption copies, it does not move).
    assert!(source.path().join("skills/demo/SKILL.md").exists());
}

#[test]
fn adopt_folder_skips_skills_with_invalid_frontmatter() {
    let _sandbox = Sandbox::new();
    let source = tempfile::tempdir().unwrap();
    valid_skill(source.path(), "good");
    let bare_dir = source.path().join("skills/bare");
    std::fs::create_dir_all(&bare_dir).unwrap();
    std::fs::write(bare_dir.join("SKILL.md"), "# No frontmatter\n").unwrap();

    let result = adopt_folder(source.path().to_str().unwrap(), None).unwrap();
    assert_eq!(result.adopted.len(), 1);
    assert_eq!(result.adopted[0].name, "good");
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].name, "bare");
    assert!(
        result.skipped[0].reason.contains("description"),
        "{}",
        result.skipped[0].reason
    );
    assert!(!is_local_skill("bare"));
}

/// P0 regression: the frontmatter `name` becomes a folder under the data
/// dir, so a traversal name must never place files outside local storage.
#[test]
fn adopt_folder_maps_traversal_names_inside_local_storage() {
    let sandbox = Sandbox::new();
    let source = tempfile::tempdir().unwrap();
    for (folder, name) in [("evil", "../../escape"), ("dots", "../.."), ("cjk", "数据")] {
        let dir = source.path().join(format!("skills/{folder}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: \"{name}\"\ndescription: d\n---\n"),
        )
        .unwrap();
    }

    let result = adopt_folder(source.path().to_str().unwrap(), None).unwrap();
    let adopted: Vec<_> = result.adopted.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(adopted, ["escape"], "{:?}", result.skipped);
    assert_eq!(result.skipped.len(), 2, "{:?}", result.skipped);
    let local = ss_core::infra::paths::local_skills_dir();
    assert!(local.join("escape/SKILL.md").is_file());
    assert!(!sandbox.root().join("escape").exists());
    assert!(!local.parent().unwrap().join("escape").exists());
}

/// P0 regression: adoption copies, so a link out of the adopted folder must
/// not pull the outside file into local storage.
#[cfg(unix)]
#[test]
fn adopt_folder_does_not_copy_links_out_of_the_folder() {
    let sandbox = Sandbox::new();
    let secret = sandbox.home().join(".ssh/id_rsa");
    std::fs::create_dir_all(secret.parent().unwrap()).unwrap();
    std::fs::write(&secret, "PRIVATE KEY").unwrap();
    let source = sandbox.root().join("src");
    valid_skill(&source, "demo");
    std::os::unix::fs::symlink(&secret, source.join("skills/demo/key")).unwrap();

    adopt_folder(source.to_str().unwrap(), None).unwrap();
    let local = ss_core::infra::paths::local_skills_dir().join("demo");
    assert!(local.join("SKILL.md").is_file());
    assert!(local.join("key").symlink_metadata().is_err());
}
