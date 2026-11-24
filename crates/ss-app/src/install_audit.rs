//! Install-time security audits. The marketplace owns the query; this is the
//! cross-domain seam CLI and the import dialog call.

use ss_marketplace::SkillSecurityAudit;

pub fn may_query_install_audits(repo_url: &str) -> bool {
    ss_marketplace::may_query_install_audits(repo_url)
}

pub async fn fetch_public_github_skill_audits(
    repo_url: &str,
    skill_names: &[String],
) -> Vec<SkillSecurityAudit> {
    ss_marketplace::fetch_public_github_skill_audits(repo_url, skill_names).await
}
