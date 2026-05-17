pub mod lockfile;
pub mod skill;
pub mod update_checker;

pub use lockfile::{LockEntry, Lockfile, default_lockfile_path, get_mutex};
pub use skill::{
    OfficialPublisher, Skill, SkillCategory, SkillContent, SkillType,
    extract_github_source_from_url, extract_skill_description, parse_skill_content,
};
pub use update_checker::{
    check_update, check_update_local, compute_subtree_hash, is_repo_cached_skill,
    is_repo_cached_skill_target_path, normalize_path_for_compare, prefetch_unique_repos,
    resolve_skill_repo_root, resolve_symlink,
};
