use super::*;

#[test]
fn parse_install_input_handles_pasted_cli_commands_and_direct_sources() {
    let cmd = parse_install_input(
        "$ npx -y skills add vercel-labs/agent-skills --skill web-design -s audit,fix -a codex -g --copy",
    )
    .unwrap();
    assert!(cmd.was_command);
    assert_eq!(cmd.source, "vercel-labs/agent-skills");
    assert_eq!(cmd.skills, vec!["web-design", "audit", "fix"]);
    assert_eq!(cmd.agents, vec!["codex"]);
    assert!(cmd.global);
    assert!(cmd.copy);
    assert!(!cmd.yes);

    let bunx = parse_install_input("bunx skills add https://github.com/acme/repo --yes").unwrap();
    assert!(bunx.was_command);
    assert_eq!(bunx.source, "https://github.com/acme/repo");
    assert!(bunx.yes);

    let pnpm = parse_install_input("pnpm dlx skills i acme/tools --all").unwrap();
    assert!(pnpm.was_command);
    assert_eq!(pnpm.source, "acme/tools");
    assert!(pnpm.all);

    let yarn = parse_install_input("yarn dlx skills add 'acme/my skills' -gy").unwrap();
    assert!(yarn.was_command);
    assert_eq!(yarn.source, "acme/my skills");
    assert!(yarn.global);
    assert!(yarn.yes);

    let ss = parse_install_input("skillstar install owner/repo -s one").unwrap();
    assert!(ss.was_command);
    assert_eq!(ss.source, "owner/repo");
    assert_eq!(ss.skills, vec!["one"]);

    let plain = parse_install_input("owner/repo").unwrap();
    assert!(!plain.was_command);
    assert_eq!(plain.source, "owner/repo");

    assert!(looks_like_direct_source("npx skills add owner/repo"));
    assert!(looks_like_direct_source("owner/repo@my-skill"));
    assert!(looks_like_direct_source("https://github.com/owner/repo"));
    assert!(!looks_like_direct_source("react testing"));
    assert!(!looks_like_direct_source("find my skill"));
}

#[test]
fn looks_like_remote_source_accepts_remote_inputs_and_commands() {
    for input in [
        "https://github.com/vuejs-ai/skills",
        "https://github.com/vuejs-ai/skills/",
        "https://github.com/vuejs-ai/skills.git",
        "https://github.com/vuejs-ai/skills/tree/main/skills/vue",
        "https://gitlab.com/group/subgroup/repo/-/tree/release/skills/demo",
        "git@github.com:vuejs-ai/skills.git",
        "ssh://git@github.com/vuejs-ai/skills.git",
        "github:vuejs-ai/skills",
        "gitlab:org/repo",
        "vuejs-ai/skills",
        "vuejs-ai/skills/skills/vue",
        "vuejs-ai/skills@vue",
        "vuejs-ai/skills#main",
        "npx skills add vuejs-ai/skills --skill vue",
        "skillstar install vuejs-ai/skills",
        "\"https://github.com/vuejs-ai/skills\"",
    ] {
        assert!(
            looks_like_remote_source(input),
            "expected remote source for {input:?}"
        );
    }
}

#[test]
fn looks_like_remote_source_rejects_local_paths_bundles_and_share_codes() {
    let agd_with_slash = format!("agd-{}/{}", "A".repeat(20), "B".repeat(20));
    let ags_with_slash = format!("ags-{}/{}", "C".repeat(20), "D".repeat(20));
    for input in [
        "",
        "   ",
        "not-a-repo",
        "/tmp/pack.agd",
        "/definitely/not/a/deck-bundle.agd",
        "./pack.agd",
        "../pack.ags",
        "~/Downloads/deck.agd",
        "file:///tmp/pack.agd",
        r"C:\Users\me\deck.agd",
        "packs/my-deck.agd",
        agd_with_slash.as_str(),
        ags_with_slash.as_str(),
        "ext::sh -c id",
        "fd::/tmp/x",
    ] {
        assert!(
            !looks_like_remote_source(input),
            "expected non-remote for {input:?}"
        );
    }
}

#[test]
fn unsafe_git_transports_are_rejected_before_shorthand_parsing() {
    for input in ["ext::sh -c id", "FD::/tmp/socket", "  ext::helper"] {
        let error = Source::parse(input).expect_err(input);
        assert!(
            error.to_string().contains("Unsupported Git transport"),
            "{input}: {error}"
        );
    }
    assert!(Source::parse("owner/repo").is_ok());
}

#[test]
fn same_remote_url_matches() {
    assert!(same_remote_url(
        "https://github.com/owner/repo.git",
        "https://github.com/owner/repo",
    ));
    assert!(same_remote_url(
        "https://github.com/Owner/Repo",
        "https://github.com/owner/repo",
    ));
    assert!(same_remote_url(
        "http://www.github.com:80/owner/repo",
        "https://github.com/owner/repo.git",
    ));
    assert!(same_remote_url(
        "ssh://git@GitHub.com:22/owner/repo",
        "https://github.com/owner/repo.git",
    ));
}

#[test]
fn same_remote_url_different_repos() {
    assert!(!same_remote_url(
        "https://github.com/owner/repo-a",
        "https://github.com/owner/repo-b",
    ));
}

// Source::parse tests

#[test]
fn source_parse_owner_repo() {
    let s = Source::parse("vercel-labs/skills").unwrap();
    assert_eq!(s.repo_url, "https://github.com/vercel-labs/skills.git");
    assert_eq!(s.short, "vercel-labs/skills");
}

#[test]
fn source_parse_full_url() {
    let s = Source::parse("https://github.com/vercel-labs/skills").unwrap();
    assert_eq!(s.repo_url, "https://github.com/vercel-labs/skills.git");
    assert_eq!(s.short, "vercel-labs/skills");
}

#[test]
fn source_parse_url_with_git_suffix() {
    let s = Source::parse("https://github.com/vercel-labs/skills.git").unwrap();
    assert_eq!(s.repo_url, "https://github.com/vercel-labs/skills.git");
    assert_eq!(s.short, "vercel-labs/skills");
}

#[test]
fn source_parse_url_with_trailing_slash() {
    let s = Source::parse("https://github.com/vercel-labs/skills/").unwrap();
    assert_eq!(s.repo_url, "https://github.com/vercel-labs/skills.git");
    assert_eq!(s.short, "vercel-labs/skills");
}

#[test]
fn source_parse_owner_repo_with_git_suffix() {
    let s = Source::parse("owner/repo.git").unwrap();
    assert_eq!(s.repo_url, "https://github.com/owner/repo.git");
    assert_eq!(s.short, "owner/repo");
}

#[test]
fn source_parse_shorthand_subpath_and_inline_skill() {
    let subtree = Source::parse("owner/repo/skills/react").unwrap();
    assert_eq!(subtree.repo_url, "https://github.com/owner/repo.git");
    assert_eq!(subtree.subpath.as_deref(), Some("skills/react"));
    assert_eq!(subtree.skill_filter, None);

    let selected = Source::parse("owner/repo@react").unwrap();
    assert_eq!(selected.repo_url, "https://github.com/owner/repo.git");
    assert_eq!(selected.subpath, None);
    assert_eq!(selected.skill_filter.as_deref(), Some("react"));
}

#[test]
fn source_parse_github_tree_url() {
    let s = Source::parse(
        "https://github.com/vercel-labs/skills/tree/canary/skills/react-best-practices",
    )
    .unwrap();
    assert_eq!(s.repo_url, "https://github.com/vercel-labs/skills.git");
    assert_eq!(s.short, "vercel-labs/skills");
    assert_eq!(s.git_ref.as_deref(), Some("canary"));
    assert_eq!(s.subpath.as_deref(), Some("skills/react-best-practices"));
}

#[test]
fn source_parse_gitlab_tree_and_ssh_ref() {
    let gitlab = Source::parse("https://gitlab.com/group/subgroup/repo/-/tree/release/skills/demo")
        .unwrap();
    assert_eq!(
        gitlab.repo_url,
        "https://gitlab.com/group/subgroup/repo.git"
    );
    assert_eq!(gitlab.short, "group/subgroup/repo");
    assert_eq!(gitlab.git_ref.as_deref(), Some("release"));
    assert_eq!(gitlab.subpath.as_deref(), Some("skills/demo"));

    let ssh = Source::parse("git@github.com:owner/repo.git#v2@demo").unwrap();
    assert_eq!(ssh.repo_url, "https://github.com/owner/repo.git");
    assert_eq!(ssh.short, "owner/repo");
    assert_eq!(ssh.git_ref.as_deref(), Some("v2"));
    assert_eq!(ssh.skill_filter.as_deref(), Some("demo"));
}

#[test]
fn source_parse_rejects_unsafe_subpaths() {
    assert!(Source::parse("owner/repo/../secrets").is_err());
    assert!(Source::parse("owner/repo/./skills").is_err());
}

#[test]
fn same_remote_url_ignores_tree_selection() {
    assert!(same_remote_url(
        "https://github.com/owner/repo/tree/main/skills/demo",
        "owner/repo"
    ));
}

#[test]
fn cache_dir_name_sanitizes_ref_punctuation() {
    assert_eq!(
        cache_dir_name("owner/repo:feature\\demo"),
        "owner--repo-feature--demo"
    );
}

#[test]
fn source_parse_empty_fails() {
    assert!(Source::parse("").is_err());
}

#[test]
fn source_parse_whitespace_fails() {
    assert!(Source::parse("  ").is_err());
}

#[test]
fn source_parse_invalid_format_fails() {
    assert!(Source::parse("not-valid").is_err());
    assert!(Source::parse("owner/").is_err());
    assert!(Source::parse("/repo").is_err());
}

#[test]
fn source_parse_local_git_directory() {
    let dir = tempfile::tempdir().unwrap();
    let parsed = Source::parse(dir.path().to_str().expect("utf-8 temp path")).unwrap();
    assert!(
        parsed.repo_url.starts_with("file://"),
        "{}",
        parsed.repo_url
    );
    assert!(parsed.short.starts_with("local/"), "{}", parsed.short);
    assert_eq!(parsed.git_ref, None);
    assert_eq!(parsed.subpath, None);
}

#[test]
fn source_parse_missing_local_path_fails() {
    assert!(Source::parse("/definitely-not-a-skillstar-source").is_err());
    assert!(Source::parse("./no-such-local-skill").is_err());
}

#[test]
fn source_serde_uses_scan_field_names() {
    let source =
        Source::parse("https://github.com/owner/repo/tree/main/skills/demo#stable@react").unwrap();
    let json = serde_json::to_value(&source).unwrap();
    assert_eq!(json["source_url"], "https://github.com/owner/repo.git");
    assert_eq!(json["source"], "owner/repo");
    assert_eq!(json["git_ref"], "main");
    assert_eq!(json["subpath"], "skills/demo");
    assert_eq!(json["skill_filter"], "react");
    assert!(json.get("repo_url").is_none());
    assert!(json.get("short").is_none());

    let roundtripped: Source = serde_json::from_value(json).unwrap();
    assert_eq!(roundtripped, source);
}

#[test]
fn parse_install_input_keeps_windows_path_backslashes() {
    // Inside double quotes a backslash only escapes `"`, `\`, `` ` `` and `$`
    // (the POSIX rule); PowerShell and cmd never use it as an escape at all.
    let quoted =
        parse_install_input("npx skills add \"C:\\Users\\dev\\My Repo\\skills\" --skill beta").unwrap();
    assert_eq!(quoted.source, "C:\\Users\\dev\\My Repo\\skills");
    assert_eq!(quoted.skills, vec!["beta"]);

    // Unquoted: a word that opens with a drive letter is a Windows path.
    let bare = parse_install_input("npx skills add C:\\Users\\dev\\skills --all").unwrap();
    assert_eq!(bare.source, "C:\\Users\\dev\\skills");
    assert!(bare.all);

    // The POSIX escapes keep their meaning everywhere else.
    let escaped = parse_install_input("npx skills add \"a\\\"b\"").unwrap();
    assert_eq!(escaped.source, "a\"b");
    let spaced = parse_install_input("npx skills add my\\ skills").unwrap();
    assert_eq!(spaced.source, "my skills");
}

#[test]
fn file_url_path_input_keeps_the_caller_prefixing_rule() {
    use super::file_url_to_path_input;
    assert_eq!(
        file_url_to_path_input("file:///var/tmp/repo"),
        Some("/var/tmp/repo")
    );
    // `file://server/share/repo` is a host path here; the caller adds the
    // leading slash, as it always has.
    assert_eq!(
        file_url_to_path_input("file://server/share/repo"),
        Some("server/share/repo")
    );
    assert_eq!(file_url_to_path_input("https://github.com/o/r"), None);
}

#[cfg(windows)]
#[test]
fn file_url_path_input_drops_the_slash_before_a_drive_letter() {
    use super::file_url_to_path_input;
    // Both spellings are what `local_file_url` and a user may hand over.
    assert_eq!(
        file_url_to_path_input("file:///C:/Users/dev/repo"),
        Some("C:/Users/dev/repo")
    );
    assert_eq!(
        file_url_to_path_input("file://C:/Users/dev/repo"),
        Some("C:/Users/dev/repo")
    );
}

#[cfg(windows)]
#[test]
fn source_parse_accepts_a_drive_letter_file_url() {
    let dir = tempfile::tempdir().unwrap();
    let url = crate::git::ops::local_file_url(dir.path());
    let source =
        Source::parse(&url).expect("a `file:///C:/…` URL names the directory it came from");
    let name = dir.path().file_name().unwrap().to_str().unwrap();
    assert_eq!(source.short, format!("local/{name}"));
    // `scan_repo_with_refresh` parses the canonical URL a second time.
    Source::parse(&source.repo_url).expect("the canonical URL re-parses");
}
