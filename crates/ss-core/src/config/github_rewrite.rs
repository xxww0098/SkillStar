//! GitHub-family URL helpers shared by Git `insteadOf`, HTTP wrapping, and
//! the marketplace `skills.sh` accelerator chain.
//!
//! Mirrors are third-party intermediaries. They may only see **anonymous**
//! traffic: never wrap a URL that carries `Authorization` or Git credentials
//! (D-014 / D-050).

/// Origins rewritten through git `url.*.insteadOf` for a single mirror.
/// `api.github.com` is intentionally absent: git does not speak REST, and
/// authenticated API traffic must never transit a public accelerator.
pub const GIT_INSTEAD_OF_ORIGINS: &[&str] = &[
    "https://github.com/",
    "https://www.github.com/",
    "https://raw.githubusercontent.com/",
    "https://codeload.github.com/",
    "https://objects.githubusercontent.com/",
    "https://gist.github.com/",
    "https://gist.githubusercontent.com/",
];

/// Public GitHub-family hosts that ghproxy-style accelerators typically
/// reverse-proxy. `api.github.com` is included for **anonymous** HTTP only.
const HTTP_FAMILY_HOSTS: &[&str] = &[
    "github.com",
    "www.github.com",
    "raw.githubusercontent.com",
    "codeload.github.com",
    "objects.githubusercontent.com",
    "gist.github.com",
    "gist.githubusercontent.com",
    "api.github.com",
    "uploads.github.com",
];

const PROBE_TARGET: &str = "https://raw.githubusercontent.com/octocat/Hello-World/master/README";

/// Normalize a mirror URL to a trailing-slash http(s) URL, or `None`.
pub fn normalize_mirror_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let with_slash = if trimmed.ends_with('/') {
        trimmed.to_string()
    } else {
        format!("{trimmed}/")
    };
    (with_slash.starts_with("https://") || with_slash.starts_with("http://")).then_some(with_slash)
}

/// Host of an absolute http(s) URL, without port or path.
pub fn http_host(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next()?.split(':').next()?;
    if host.is_empty() { None } else { Some(host) }
}

pub fn is_github_family_host(host: &str) -> bool {
    HTTP_FAMILY_HOSTS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(host))
}

/// True when `url` is an absolute GitHub-family http(s) URL with no userinfo.
pub fn is_anonymous_rewritable(url: &str) -> bool {
    if url.contains('@') && url.contains("://") {
        // `https://user:token@github.com/...` must never be wrapped.
        if let Some(after_scheme) = url.split("://").nth(1)
            && after_scheme.contains('@')
        {
            return false;
        }
    }
    http_host(url).is_some_and(is_github_family_host)
}

/// `{mirror}{url}`, or `None` if the mirror is unusable. Already-wrapped
/// URLs are returned unchanged so a chain cannot nest prefixes.
pub fn wrap_with_mirror(url: &str, mirror: &str) -> Option<String> {
    let mirror = normalize_mirror_url(mirror)?;
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with(&mirror) {
        return Some(trimmed.to_string());
    }
    Some(format!("{mirror}{trimmed}"))
}

/// Connectivity probe used by Settings "Test" and the network doctor.
/// Hits a tiny public raw file, not the accelerator root (a 200 on `/` does
/// not prove git/raw proxying works).
pub fn mirror_probe_url(mirror: &str) -> Option<String> {
    wrap_with_mirror(PROBE_TARGET, mirror)
}

/// Wrap `https://skills.sh/` through a GitHub-family accelerator.
pub fn wrap_skills_sh(mirror: &str) -> Option<String> {
    wrap_with_mirror("https://skills.sh/", mirror)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_prefixes_github_family_urls() {
        assert_eq!(
            wrap_with_mirror("https://github.com/acme/repo.git", "https://ghproxy.vip").as_deref(),
            Some("https://ghproxy.vip/https://github.com/acme/repo.git")
        );
        assert_eq!(
            wrap_with_mirror(
                "https://raw.githubusercontent.com/octocat/Hello-World/master/README",
                "https://gh-proxy.com/"
            )
            .as_deref(),
            Some(
                "https://gh-proxy.com/https://raw.githubusercontent.com/octocat/Hello-World/master/README"
            )
        );
    }

    #[test]
    fn wrap_does_not_nest_the_same_mirror() {
        let once =
            wrap_with_mirror("https://github.com/acme/repo", "https://ghproxy.vip/").unwrap();
        let twice = wrap_with_mirror(&once, "https://ghproxy.vip/").unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn credential_bearing_urls_are_not_rewritable() {
        assert!(!is_anonymous_rewritable(
            "https://x-access-token:secret@github.com/acme/private.git"
        ));
        assert!(is_anonymous_rewritable(
            "https://github.com/acme/public.git"
        ));
        assert!(is_anonymous_rewritable(
            "https://api.github.com/repos/acme/public"
        ));
        assert!(!is_anonymous_rewritable("https://gitlab.com/acme/repo"));
    }

    #[test]
    fn probe_url_hits_raw_not_the_mirror_root() {
        let probe = mirror_probe_url("https://ghproxy.vip").unwrap();
        assert!(
            probe.ends_with("https://raw.githubusercontent.com/octocat/Hello-World/master/README")
        );
        assert!(probe.starts_with("https://ghproxy.vip/"));
    }

    #[test]
    fn git_instead_of_origins_never_include_api() {
        assert!(
            GIT_INSTEAD_OF_ORIGINS
                .iter()
                .all(|origin| !origin.contains("api.github.com"))
        );
    }
}
