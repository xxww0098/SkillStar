use std::sync::{Mutex, OnceLock};

use super::parse_official_publishers_html;

/// Serializes tests that mutate the process-global `SKILLSTAR_DATA_DIR`
/// (marketplace mirror config reads it through the shared config path).
fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

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

#[test]
fn marketplace_hosts_primary_always_first_and_mirrors_deduplicated() {
    // The mirror config lives in the shared data dir; isolate it.
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let temp = tempfile::TempDir::new().unwrap();
    unsafe {
        std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
    }

    let config = skillstar_core::config::marketplace_mirror::MarketplaceMirrorConfig {
        enabled: true,
        hosts: vec![
            "https://skills.sh/".into(),       // duplicate of primary → dropped
            "https://mirror.example".into(),   // no trailing slash → normalized
            "http://insecure.example/".into(), // http → rejected
            "not-a-url".into(),                // rejected
            "https://mirror.example/".into(),  // duplicate → dropped
        ],
    };
    skillstar_core::config::marketplace_mirror::save_config(&config).unwrap();

    let hosts = super::marketplace_hosts();
    assert_eq!(
        hosts,
        vec![
            "https://skills.sh/".to_string(),
            "https://mirror.example/".to_string(),
        ],
        "primary first, http/non-URL dropped, duplicates removed, trailing slash normalized"
    );

    unsafe {
        std::env::remove_var("SKILLSTAR_DATA_DIR");
    }
}

#[test]
fn marketplace_hosts_disabled_or_missing_config_returns_only_primary() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let temp = tempfile::TempDir::new().unwrap();
    unsafe {
        std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
    }

    // No config file → default disabled → primary only.
    let hosts = super::marketplace_hosts();
    assert_eq!(hosts, vec!["https://skills.sh/".to_string()]);

    // Explicitly disabled → primary only.
    let config = skillstar_core::config::marketplace_mirror::MarketplaceMirrorConfig {
        enabled: false,
        hosts: vec!["https://mirror.example/".into()],
    };
    skillstar_core::config::marketplace_mirror::save_config(&config).unwrap();
    let hosts = super::marketplace_hosts();
    assert_eq!(hosts, vec!["https://skills.sh/".to_string()]);

    unsafe {
        std::env::remove_var("SKILLSTAR_DATA_DIR");
    }
}

/// The regression that took the whole marketplace down: the primary host had no
/// trailing slash while `fetch_with_failover` stripped the path's leading one,
/// so every request went to `https://skills.shhot` / `https://skills.shofficial`
/// (NXDOMAIN). Assert the final URLs, not just the host list — asserting hosts
/// alone is exactly why 58 green tests missed this.
#[test]
fn join_url_produces_the_real_request_urls() {
    for host in ["https://skills.sh", "https://skills.sh/"] {
        assert_eq!(super::join_url(host, "/hot"), "https://skills.sh/hot");
        assert_eq!(
            super::join_url(host, "/trending"),
            "https://skills.sh/trending"
        );
        assert_eq!(
            super::join_url(host, "/official"),
            "https://skills.sh/official"
        );
        assert_eq!(
            super::join_url(host, "/api/search?q=a&limit=1"),
            "https://skills.sh/api/search?q=a&limit=1"
        );
        // The leaderboard root must stay a single slash, never `//`.
        assert_eq!(super::join_url(host, "/"), "https://skills.sh/");
    }
}

/// The test above is not enough on its own and a mutation experiment proved
/// it: replacing the *call site* with a hand-written
/// `format!("{host}{}", path.trim_start_matches('/'))` left every test green,
/// because `join_url` is defensive on both ends and the host list is
/// normalized, so both spellings agree. What the request path actually needs is
/// that it stays correct for a host that has *lost* its trailing slash — the
/// exact shape that produced `https://skills.shhot`.
#[test]
fn failover_targets_build_the_real_request_urls_for_any_host_shape() {
    let targets = super::failover_targets_for(
        vec![
            "https://skills.sh".to_string(), // un-normalized: the regression shape
            "https://mirror.example/".to_string(),
        ],
        "/hot",
    );
    assert_eq!(
        targets,
        vec![
            (
                "https://skills.sh".to_string(),
                "https://skills.sh/hot".to_string()
            ),
            (
                "https://mirror.example/".to_string(),
                "https://mirror.example/hot".to_string()
            ),
        ],
        "a host without a trailing slash must never swallow the path separator"
    );

    // Every path the fetchers actually request, over both host shapes.
    for host in ["https://skills.sh", "https://skills.sh/"] {
        for (path, expected) in [
            ("/hot", "https://skills.sh/hot"),
            ("/trending", "https://skills.sh/trending"),
            ("/official", "https://skills.sh/official"),
            (
                "/api/search?q=a&limit=1",
                "https://skills.sh/api/search?q=a&limit=1",
            ),
            ("/", "https://skills.sh/"),
        ] {
            let targets = super::failover_targets_for(vec![host.to_string()], path);
            assert_eq!(targets[0].1, expected, "{host} + {path}");
        }
    }
}

/// The live host list feeds the same builder: the primary host must produce a
/// resolvable URL with no mirror configured at all.
#[test]
fn failover_targets_over_the_live_host_list_hit_the_primary_first() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let temp = tempfile::TempDir::new().unwrap();
    unsafe {
        std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
    }

    let targets = super::failover_targets("/hot");
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].1, "https://skills.sh/hot");

    unsafe {
        std::env::remove_var("SKILLSTAR_DATA_DIR");
    }
}

#[test]
fn every_marketplace_host_joins_paths_correctly() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let temp = tempfile::TempDir::new().unwrap();
    unsafe {
        std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
    }

    let config = skillstar_core::config::marketplace_mirror::MarketplaceMirrorConfig {
        enabled: true,
        hosts: vec!["https://mirror.example".into()],
    };
    skillstar_core::config::marketplace_mirror::save_config(&config).unwrap();

    for host in super::marketplace_hosts() {
        for path in ["/hot", "/", "/api/search?q=a&limit=1", "/official"] {
            let url = super::join_url(&host, path);
            assert!(
                url.starts_with("https://") && !url.contains("shhot") && !url.contains("//api"),
                "{host} + {path} produced a malformed URL: {url}"
            );
            assert_eq!(
                url.matches("//").count(),
                1,
                "{host} + {path} produced a doubled slash: {url}"
            );
        }
    }

    unsafe {
        std::env::remove_var("SKILLSTAR_DATA_DIR");
    }
}

#[test]
fn enabled_github_mirrors_wrap_skills_sh_after_the_primary() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let temp = tempfile::TempDir::new().unwrap();
    unsafe {
        std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
    }

    skillstar_core::config::github_mirror::save_config(
        &skillstar_core::config::github_mirror::GitHubMirrorConfig {
            enabled: true,
            preset_id: Some("ghproxy_vip".into()),
            custom_url: None,
        },
    )
    .unwrap();

    let hosts = super::marketplace_hosts();
    assert_eq!(hosts[0], "https://skills.sh/");
    assert!(
        hosts
            .iter()
            .any(|host| host.starts_with("https://ghproxy.vip/https://skills.sh")),
        "healthy GitHub mirrors must wrap skills.sh: {hosts:?}"
    );
    for host in &hosts {
        let url = super::join_url(host, "/hot");
        assert!(!url.contains("shhot"), "wrapped host {host} produced {url}");
        assert!(
            url.ends_with("/hot") || url.contains("skills.sh/hot"),
            "wrapped host {host} produced {url}"
        );
    }

    unsafe {
        std::env::remove_var("SKILLSTAR_DATA_DIR");
    }
}

#[test]
fn etag_is_only_sent_to_the_host_that_issued_it() {
    assert!(super::should_send_etag(
        Some("\"v1\""),
        Some("https://skills.sh/"),
        "https://skills.sh/"
    ));
    assert!(super::should_send_etag(
        Some("\"v1\""),
        Some("https://skills.sh"),
        "https://skills.sh/"
    ));
    assert!(!super::should_send_etag(
        Some("\"v1\""),
        Some("https://skills.sh/"),
        "https://ghproxy.vip/https://skills.sh/"
    ));
    assert!(!super::should_send_etag(
        Some("\"v1\""),
        None,
        "https://skills.sh/"
    ));
    assert!(!super::should_send_etag(
        None,
        Some("https://skills.sh/"),
        "https://skills.sh/"
    ));
}

/// The rank-1 skill used to be lost: the object regex allowed `{` inside the
/// character class, so the leftmost match started at the enclosing
/// `{\"initialSkills\":[{` and produced unbalanced JSON that serde dropped.
#[test]
fn escaped_ssr_payload_keeps_the_first_ranked_skill() {
    let html = concat!(
        r#"<script>self.__next_f.push([1,"{\"initialSkills\":["#,
        r#"{\"source\":\"acme/top\",\"skillId\":\"top-skill\",\"name\":\"top-skill\",\"installs\":999},"#,
        r#"{\"source\":\"acme/second\",\"skillId\":\"second\",\"name\":\"second\",\"installs\":500},"#,
        r#"{\"source\":\"acme/third\",\"skillId\":\"third\",\"name\":\"third\",\"installs\":10}"#,
        r#"]}"])</script>"#,
    );

    let skills = super::leaderboard::extract_skills_from_escaped_payload(html);
    let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["top-skill", "second", "third"],
        "no entry may be swallowed by the enclosing initialSkills object"
    );
}

#[test]
fn sha256_hex_is_deterministic_and_64_chars() {
    let a = super::sha256_hex("hello");
    let b = super::sha256_hex("hello");
    let c = super::sha256_hex("world");
    assert_eq!(a, b);
    assert_eq!(a.len(), 64);
    assert_ne!(a, c);
}

fn leaderboard_skill(name: &str, stars: u32) -> crate::Skill {
    crate::Skill::from_skills_sh(
        name.to_string(),
        String::new(),
        stars,
        "acme/repo".to_string(),
        "https://github.com/acme/repo".to_string(),
    )
}

/// The degraded flag has to be decided on what the *HTML* produced, before the
/// search-API supplement is appended — after the append the two are
/// indistinguishable and `skills.is_empty()` is always false.
///
/// This is the real failure this mechanism exists for: skills.sh changes its
/// SSR structure, the leaderboard parses to nothing, and `/api/search` happily
/// returns its capped 200 fuzzy matches for the literal word "skill". Judged
/// after the append, that payload used to come back as a complete leaderboard —
/// full 6-hour TTL, "fresh" label — and the snapshot writer deleted the ~600
/// real rows to make room for it.
#[test]
fn a_leaderboard_that_came_only_from_the_search_api_is_degraded() {
    use super::leaderboard::combine_leaderboard;

    // Upstream redesign: HTML parses to nothing, the API supplement answers.
    let (skills, degraded) = combine_leaderboard(
        Vec::new(),
        Ok(vec![
            leaderboard_skill("api-a", 10),
            leaderboard_skill("api-b", 99),
        ]),
    )
    .expect("the API answer is still data");
    assert_eq!(skills.len(), 2, "the fallback rows are still returned…");
    assert!(
        degraded,
        "…but a listing built entirely from the capped search API is degraded"
    );
    assert_eq!(
        skills.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        vec!["api-b", "api-a"],
        "fallback rows are ranked by stars, since skills.sh gave no ranking"
    );
    assert_eq!(skills[0].rank, Some(1));

    // The healthy case: the HTML parsed, the API only supplements it.
    let (skills, degraded) = combine_leaderboard(
        vec![leaderboard_skill("ssr-top", 500)],
        Ok(vec![
            leaderboard_skill("api-a", 10),
            leaderboard_skill("ssr-top", 1),
        ]),
    )
    .expect("complete payload");
    assert_eq!(skills.len(), 2, "the duplicate is dropped, not re-ranked");
    assert!(
        !degraded,
        "a supplemented leaderboard is a real leaderboard"
    );
    assert_eq!(skills[0].name, "ssr-top");
    assert_eq!(skills[1].rank, Some(2));

    // SSR-only (the supplement failed) is complete too — nothing is missing
    // from the leaderboard itself.
    let (skills, degraded) = combine_leaderboard(
        vec![leaderboard_skill("ssr-top", 500)],
        Err(anyhow::anyhow!("api down")),
    )
    .expect("SSR alone is a leaderboard");
    assert_eq!(skills.len(), 1);
    assert!(!degraded);

    // Neither half produced anything: that is no payload at all, not a degraded
    // one. Committing it would wipe the stored leaderboard.
    assert!(
        combine_leaderboard(Vec::new(), Err(anyhow::anyhow!("api down"))).is_err(),
        "an empty leaderboard must fail the refresh, not overwrite the snapshot"
    );
    assert!(combine_leaderboard(Vec::new(), Ok(Vec::new())).is_err());
}

/// A `/api/search` body with `count` rows, in the exact shape skills.sh
/// returns: `id`/`skillId`/`name`/`installs`/`source`, and **no**
/// `description`/`repoUrl`.
fn search_supplement_body(count: usize) -> String {
    let rows: Vec<String> = (0..count)
        .map(|i| {
            format!(
                r#"{{"id":"acme/repo/skill-{i}","skillId":"skill-{i}","name":"skill-{i}","installs":{installs},"source":"acme/repo"}}"#,
                installs = 1000 - i
            )
        })
        .collect();
    format!(
        r#"{{"query":"skill","searchType":"fuzzy","skills":[{}],"count":{count},"duration_ms":42}}"#,
        rows.join(",")
    )
}

/// The supplement asks for exactly the cap the server enforces.
///
/// It used to ask for `limit=100000` while the comment claimed a ~50K full
/// registry dump. The endpoint has no pagination at all — `offset`, `page`,
/// `cursor`, `after`, `skip`, `start`, `from` and `pageSize` are silently
/// ignored — so an inflated `limit` bought nothing except the false impression
/// that this call returns the whole registry.
#[test]
fn search_supplement_requests_exactly_the_server_cap() {
    use super::leaderboard::{SEARCH_API_HARD_LIMIT, search_supplement_path};

    let path = search_supplement_path();
    assert_eq!(
        path,
        format!("/api/search?q=skill&limit={SEARCH_API_HARD_LIMIT}")
    );
    assert_eq!(SEARCH_API_HARD_LIMIT, 200);
    assert!(
        !path.contains("100000"),
        "asking above the cap does not raise it; it only reads as a full dump"
    );

    // `/api/search` rejects a query below 2 characters, so the supplement term
    // can never be shortened into a wildcard.
    let query = path
        .split("q=")
        .nth(1)
        .and_then(|rest| rest.split('&').next())
        .unwrap();
    assert!(query.len() >= 2, "q must stay at least 2 characters");
}

/// The cap is data, not a log line: reaching it means rows exist that this
/// endpoint cannot hand over, and callers must be able to see that.
#[test]
fn search_supplement_reports_when_the_server_cap_swallowed_rows() {
    use super::leaderboard::parse_search_supplement;

    let under = parse_search_supplement(&search_supplement_body(199)).unwrap();
    assert_eq!(under.skills.len(), 199);
    assert!(!under.truncated, "a short page means the matches ran out");

    let at_cap = parse_search_supplement(&search_supplement_body(200)).unwrap();
    assert_eq!(at_cap.skills.len(), 200);
    assert!(
        at_cap.truncated,
        "a full page is the cap, and there is no offset parameter to get the rest"
    );

    let empty = parse_search_supplement(&search_supplement_body(0)).unwrap();
    assert!(empty.skills.is_empty());
    assert!(!empty.truncated);
}

/// The capped rows carry no `description`/`repoUrl`, so every supplemented
/// entry is thinner than an SSR row — another reason it is not a registry dump.
#[test]
fn search_supplement_rows_have_no_description_and_a_derived_repo_url() {
    use super::leaderboard::parse_search_supplement;

    let supplement = parse_search_supplement(&search_supplement_body(2)).unwrap();
    let first = &supplement.skills[0];
    assert_eq!(first.name, "skill-0");
    assert_eq!(first.stars, 1000);
    assert_eq!(
        first.description, "",
        "the endpoint never sends description"
    );
    assert_eq!(
        first.git_url, "https://github.com/acme/repo",
        "repoUrl is absent, so it is reconstructed from `source`"
    );
    assert_eq!(first.source.as_deref(), Some("acme/repo"));
}

#[test]
fn search_supplement_parse_failure_is_an_error_not_an_empty_page() {
    use super::leaderboard::parse_search_supplement;

    // A WAF challenge page or an upstream redesign must not read as
    // "the registry has no skills".
    assert!(parse_search_supplement("<!DOCTYPE html><html>nope</html>").is_err());
    assert!(parse_search_supplement(r#"{"error":"Query must be at least 2 characters"}"#).is_err());
}
