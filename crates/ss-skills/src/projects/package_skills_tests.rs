use std::fs;
use std::path::Path;

use super::{UninstallPlan, plan_package_skills_uninstall, preview_package_skills};

fn write_project(dir: &Path, package_json: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("package.json"), package_json).unwrap();
}

fn preview_at(dir: &Path, installed: &[&str]) -> super::PackageSkillsPreview {
    let installed = installed
        .iter()
        .map(|name| (*name).to_string())
        .collect::<Vec<_>>();
    preview_package_skills(dir, &installed, &[], false)
}

#[test]
fn parses_npm_and_git_entries_without_writing_node_modules() {
    let root = std::env::temp_dir().join(format!("skillstar-pkg-skills-{}", uuid_like()));
    let project = root.join("project");
    write_project(
        &project,
        r#"{
          "skills": [
            "npm:@vueuse/skills",
            {"source": "npm:my-lib", "skills": ["a", "b"]},
            "owner/repo@one",
            {"source": "owner/repo", "ref": "v1", "skills": ["two"]},
            "https://gitlab.com/group/repo/-/tree/main/skills"
          ]
        }"#,
    );
    let marker = project.join("node_modules").join("keep.txt");
    fs::create_dir_all(marker.parent().unwrap()).unwrap();
    fs::write(&marker, "untouched").unwrap();

    let preview = preview_at(&project, &["two"]);

    assert_eq!(
        preview.errors,
        [
            ".: cannot resolve \"npm:@vueuse/skills\"; is it a dependency?".to_string(),
            ".: cannot resolve \"npm:my-lib\"; is it a dependency?".to_string(),
        ]
    );
    assert_eq!(preview.npm.len(), 2);
    assert_eq!(preview.npm[0].package, "@vueuse/skills");
    assert!(preview.npm[0].resolved.is_none());
    assert_eq!(preview.npm[1].package, "my-lib");
    assert_eq!(preview.npm[1].skills, ["a", "b"]);
    assert_eq!(preview.remote[0].skills, ["one"]);
    assert_eq!(
        preview.remote[0].repo_url,
        "https://github.com/owner/repo.git"
    );
    assert_eq!(preview.remote[1].git_ref.as_deref(), Some("v1"));
    assert_eq!(preview.remote[1].skills, ["two"]);
    assert_eq!(preview.remote[2].git_ref.as_deref(), Some("main"));
    assert_eq!(preview.remote[2].subpath.as_deref(), Some("skills"));
    assert_eq!(preview.already_installed, ["two"]);
    assert!(preview.missing_named.contains(&"one".to_string()));
    assert!(matches!(preview.uninstall, UninstallPlan::NothingRecorded));
    assert_eq!(fs::read_to_string(&marker).unwrap(), "untouched");
    assert!(!project.join("node_modules").join("@vueuse").exists());
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn rejects_local_paths_non_git_http_and_conflicting_refs() {
    let root = std::env::temp_dir().join(format!("skillstar-pkg-reject-{}", uuid_like()));
    let project = root.join("project");
    fs::create_dir_all(project.join("local")).unwrap();
    write_project(
        &project,
        r#"{
          "skills": [
            "./local",
            "https://example.com/skills",
            {"source": "owner/repo#v1", "ref": "v2"},
            {"source": "https://github.com/o/r/tree/main/x", "ref": "v2"},
            {"source": "npm:x", "ref": "v1"},
            "C:\\skills"
          ]
        }"#,
    );
    let preview = preview_at(&project, &[]);
    let errors = preview.errors.join("\n");
    assert!(
        errors.contains("\"./local\" is not a git source"),
        "{errors}"
    );
    assert!(
        errors.contains("\"https://example.com/skills\" is not a git source"),
        "{errors}"
    );
    assert!(errors.contains("already has a ref"), "{errors}");
    assert!(
        errors.contains("\"ref\" cannot be used with \"npm:x\""),
        "{errors}"
    );
    assert!(
        errors.contains("\"C:\\skills\" is not a git source"),
        "{errors}"
    );
    assert!(preview.remote.is_empty());
    assert!(preview.npm.is_empty());
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn keeps_valid_entries_when_others_are_malformed() {
    let root = std::env::temp_dir().join(format!("skillstar-pkg-mixed-{}", uuid_like()));
    let project = root.join("project");
    write_project(
        &project,
        r#"{ "skills": [42, {"skills": ["a"]}, {"source": "npm:x", "skills": "a"}, "npm:ok"] }"#,
    );
    let preview = preview_at(&project, &[]);
    assert_eq!(preview.npm.len(), 1);
    assert_eq!(preview.npm[0].package, "ok");
    assert!(
        preview
            .errors
            .iter()
            .any(|error| error.contains("invalid \"skills\" entry 42"))
    );
    assert!(
        preview
            .errors
            .iter()
            .any(|error| error.contains("cannot resolve \"npm:ok\""))
    );
    assert_eq!(preview.errors.len(), 4, "{:?}", preview.errors);
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn reads_installed_npm_skills_and_skips_escaped_links() {
    let root = std::env::temp_dir().join(format!("skillstar-pkg-npm-{}", uuid_like()));
    let project = root.join("project");
    let package = project.join("node_modules").join("my-lib");
    let skill = package.join("skills").join("alpha");
    fs::create_dir_all(&skill).unwrap();
    fs::write(package.join("package.json"), r#"{"name":"my-lib"}"#).unwrap();
    fs::write(package.join("marker.txt"), "keep").unwrap();
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: alpha\ndescription: demo\n---\n",
    )
    .unwrap();
    let outside = root.join("outside");
    fs::create_dir_all(outside.join("skills").join("pwn")).unwrap();
    fs::write(
        outside.join("skills").join("pwn").join("SKILL.md"),
        "---\nname: pwn\ndescription: no\n---\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(
        outside.join("skills").join("pwn"),
        package.join("skills").join("linked"),
    )
    .unwrap();
    write_project(
        &project,
        r#"{ "skills": ["npm:my-lib", "npm:../../outside", "npm:@scope/../../x"] }"#,
    );

    let preview = preview_at(&project, &["alpha"]);

    assert_eq!(preview.npm.len(), 1);
    assert_eq!(preview.npm[0].shipped, ["alpha"]);
    assert!(preview.already_installed.contains(&"alpha".to_string()));
    assert!(
        !preview
            .npm
            .iter()
            .any(|item| item.shipped.iter().any(|name| name == "pwn"))
    );
    assert!(
        preview
            .errors
            .iter()
            .any(|error| error.contains("invalid npm package name"))
    );
    assert_eq!(
        fs::read_to_string(package.join("marker.txt")).unwrap(),
        "keep"
    );
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn missing_or_non_array_field_does_not_invent_installs() {
    let root = std::env::temp_dir().join(format!("skillstar-pkg-empty-{}", uuid_like()));
    let project = root.join("project");
    write_project(&project, r#"{ "name": "demo" }"#);
    let preview = preview_at(&project, &["already"]);
    assert!(preview.errors.is_empty());
    assert!(preview.remote.is_empty());
    assert!(preview.missing_named.is_empty());

    fs::write(
        project.join("package.json"),
        r#"{ "skills": { "source": "owner/repo" } }"#,
    )
    .unwrap();
    let preview = preview_at(&project, &[]);
    assert_eq!(
        preview.errors,
        ["package.json: \"skills\" must be an array"]
    );
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn uninstall_plan_never_deletes_and_requires_a_record_plus_confirmation() {
    let root = std::env::temp_dir().join(format!("skillstar-pkg-uninstall-{}", uuid_like()));
    fs::create_dir_all(&root).unwrap();
    let sentinel = root.join("skill.txt");
    fs::write(&sentinel, "owned-by-user").unwrap();

    assert!(matches!(
        plan_package_skills_uninstall(true, &[]),
        UninstallPlan::NothingRecorded
    ));
    assert!(matches!(
        plan_package_skills_uninstall(false, &["alpha".to_string()]),
        UninstallPlan::NeedsConfirmation { .. }
    ));
    assert!(matches!(
        plan_package_skills_uninstall(true, &["alpha".to_string()]),
        UninstallPlan::Confirmed { .. }
    ));
    assert_eq!(fs::read_to_string(&sentinel).unwrap(), "owned-by-user");
    fs::remove_dir_all(&root).unwrap();
}

fn uuid_like() -> String {
    format!(
        "{:x}",
        std::process::id() as u128
            + std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
    )
}
