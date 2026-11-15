use std::fs;
use std::path::Path;

use super::{build_use_prompt, prepare_use, resolve_selector, select_use_skill};
use crate::discovery::DiscoveredSkill;

fn skill(id: &str, installable: bool, internal: bool) -> DiscoveredSkill {
    DiscoveredSkill {
        id: id.to_string(),
        folder_path: id.to_string(),
        description: "demo".to_string(),
        already_installed: false,
        installable,
        frontmatter_issues: if installable {
            Vec::new()
        } else {
            vec!["missing_description".to_string()]
        },
        internal,
    }
}

#[test]
fn unnamed_use_skips_internal_and_requires_one_skill() {
    let skills = vec![skill("hidden", true, true), skill("visible", true, false)];
    assert_eq!(select_use_skill(&skills, None).unwrap().id, "visible");

    let many = vec![skill("one", true, false), skill("two", true, false)];
    let error = select_use_skill(&many, None).unwrap_err().to_string();
    assert!(error.contains("--skill"), "{error}");
    assert!(error.contains("one"), "{error}");
}

#[test]
fn named_internal_skill_is_usable_and_conflicting_selectors_fail() {
    let skills = vec![skill("hidden", true, true)];
    assert_eq!(
        select_use_skill(&skills, Some("hidden")).unwrap().id,
        "hidden"
    );
    let error = resolve_selector(Some("one"), Some("two"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("Conflicting skill selectors"), "{error}");
}

#[test]
fn prompt_includes_supporting_directory_only_when_present() {
    let bare = build_use_prompt("instructions", None, false);
    assert!(bare.contains("<SKILL.md>\ninstructions\n</SKILL.md>"));
    assert!(!bare.contains("Supporting files"));

    let with_files = build_use_prompt("instructions", Some(Path::new("/tmp/skill")), true);
    assert!(with_files.contains("Supporting files for this skill were downloaded to:\n/tmp/skill"));
}

#[test]
fn local_use_reads_the_skill_and_does_not_create_an_install() {
    let root = std::env::temp_dir().join(format!("skillstar-use-local-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let skill_dir = root.join("demo");
    fs::create_dir_all(skill_dir.join("refs")).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: demo\ndescription: local use\n---\n\nDo the thing.\n",
    )
    .unwrap();
    fs::write(skill_dir.join("refs").join("note.txt"), "extra").unwrap();
    let before = fs::read_dir(&root).unwrap().count();

    let prepared = prepare_use(skill_dir.to_str().unwrap(), None, false).unwrap();

    assert_eq!(prepared.skill, "demo");
    assert!(prepared.prompt.contains("Do the thing."));
    assert!(prepared.support_dir.is_some());
    assert_eq!(fs::read_dir(&root).unwrap().count(), before);
    assert!(!root.join(".skill-lock.json").exists());
    fs::remove_dir_all(&root).unwrap();
}
