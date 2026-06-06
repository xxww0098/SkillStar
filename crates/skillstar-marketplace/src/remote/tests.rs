use super::parse_official_publishers_html;

#[test]
fn parses_current_official_row_repo_and_skill_counts() {
    let html = r#"<a class="group grid grid-cols-[1fr_4rem_4rem]" href="/anthropics"><div class="min-w-0 flex items-center gap-3"><span class="font-semibold text-foreground">anthropics</span><span class="font-mono text-sm text-(--ds-gray-600)">skills</span></div><div class="text-right font-mono text-sm text-(--ds-gray-600)">11</div><div class="text-right font-mono text-sm text-(--ds-gray-600)">256</div></a>"#;
    let publishers = parse_official_publishers_html(html);
    assert_eq!(publishers.len(), 1);
    assert_eq!(publishers[0].name, "anthropics");
    assert_eq!(publishers[0].repo, "skills");
    assert_eq!(publishers[0].repo_count, 11);
    assert_eq!(publishers[0].skill_count, 256);
}

#[test]
fn parses_publisher_repos_from_official_ssr_payload() {
    use super::parse_publisher_repos_from_official_payload;

    // Simulate the SSR payload with backslash-escaped quotes (as seen in real HTML)
    let html = r#"some prefix{\"owner\":\"github\",\"repos\":[{\"repo\":\"github/awesome-copilot\",\"totalInstalls\":2424777,\"skills\":[{\"name\":\"git-commit\",\"installs\":22757}]},{\"repo\":\"github/gh-aw\",\"totalInstalls\":100,\"skills\":[{\"name\":\"developer\",\"installs\":50},{\"name\":\"console\",\"installs\":50}]},{\"repo\":\"github/copilot-plugins\",\"totalInstalls\":30,\"skills\":[{\"name\":\"spark\",\"installs\":30}]},{\"repo\":\"github/gh-aw-firewall\",\"totalInstalls\":3,\"skills\":[{\"name\":\"awf-skill\",\"installs\":3}]},{\"repo\":\"github/synapsync\",\"totalInstalls\":2,\"skills\":[{\"name\":\"code-analyzer\",\"installs\":2}]}],\"totalInstalls\":2424881}some suffix"#;

    let repos = parse_publisher_repos_from_official_payload(html, "github");
    assert_eq!(
        repos.len(),
        5,
        "Should find all 5 repos including low-traffic ones"
    );
    assert_eq!(repos[0].repo, "awesome-copilot");
    assert_eq!(repos[0].skill_count, 1); // 1 skill in test data
    assert_eq!(repos[0].installs, 2424777);
    assert_eq!(repos[4].repo, "synapsync");
    assert_eq!(repos[4].installs, 2);
}
