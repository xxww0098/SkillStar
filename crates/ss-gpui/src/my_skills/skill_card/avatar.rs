//! Category glyph and cached GitHub owner image for a skill card.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::*;
use ss_core::types::skill::{Skill, SkillType};

use crate::spawn_domain;

pub(super) struct AvatarLook {
    pub icon: IconName,
    pub bg: u32,
    pub border: u32,
    pub fg: u32,
}

/// Pick the dark or light triple for the active mode — category hues
/// flip from neon-on-navy to ink-on-pastel so they stay readable on
/// white cards.
fn look(icon: IconName, dark: (u32, u32, u32), light: (u32, u32, u32)) -> AvatarLook {
    let (bg, border, fg) = if crate::theme::is_light() {
        light
    } else {
        dark
    };
    AvatarLook {
        icon,
        bg,
        border,
        fg,
    }
}

pub(super) fn skill_owner(skill: &Skill) -> Option<String> {
    if skill.skill_type == SkillType::Local {
        return None;
    }
    let raw = if let Some(source) = skill
        .source
        .as_deref()
        .filter(|source| *source != "remote" && *source != "local")
    {
        source.split('/').next().unwrap_or("").trim().to_string()
    } else if let Some(author) = skill.author.as_deref() {
        author.trim().trim_start_matches('@').to_string()
    } else {
        github_owner_from_url(&skill.git_url).unwrap_or_default()
    };
    let owner: String = raw
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect();
    (!owner.is_empty()).then_some(owner)
}

fn github_owner_from_url(git_url: &str) -> Option<String> {
    let rest = git_url
        .split("github.com")
        .nth(1)?
        .trim_start_matches(['/', ':']);
    let owner = rest.split('/').next()?.trim();
    (!owner.is_empty()).then(|| owner.to_string())
}

fn avatar_cache_path(owner: &str) -> PathBuf {
    ss_core::infra::paths::cache_dir()
        .join("avatars")
        .join(format!("{owner}.png"))
}

pub(super) fn cached_owner_avatar(skill: &Skill) -> Option<PathBuf> {
    let path = avatar_cache_path(skill_owner(skill)?.as_str());
    path.is_file().then_some(path)
}

/// Fetch the owner avatar once. Any page can call this; success just redraws.
pub fn prefetch_skill_avatar<V: 'static>(skill: &Skill, view: &Entity<V>, cx: &mut Context<V>) {
    let Some(owner) = skill_owner(skill) else {
        return;
    };
    let dest = avatar_cache_path(&owner);
    if dest.is_file() {
        return;
    }
    static STARTED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let started = STARTED.get_or_init(|| Mutex::new(HashSet::new()));
    if !started
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .insert(owner.clone())
    {
        return;
    }
    spawn_domain(
        view,
        cx,
        async move { fetch_owner_avatar(&owner, &dest).await },
        |_, cx, result| {
            if result.is_ok() {
                cx.notify();
            }
        },
    );
}

async fn fetch_owner_avatar(owner: &str, dest: &Path) -> Result<(), String> {
    let client = ss_core::infra::http_client::probe_http_client(Duration::from_secs(10))
        .map_err(|err| err.to_string())?;
    let url = format!("https://github.com/{owner}.png?size=120");
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(response.status().to_string());
    }
    let bytes = response.bytes().await.map_err(|err| err.to_string())?;
    let png = bytes.starts_with(b"\x89PNG");
    let jpeg = bytes.starts_with(&[0xff, 0xd8]);
    if bytes.len() < 32 || !(png || jpeg) {
        return Err("not an image".into());
    }
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|err| err.to_string())?;
    }
    let tmp = dest.with_extension("png.part");
    tokio::fs::write(&tmp, &bytes)
        .await
        .map_err(|err| err.to_string())?;
    tokio::fs::rename(&tmp, dest)
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

pub(super) fn avatar_look(skill: &Skill) -> AvatarLook {
    let name = skill.name.to_ascii_lowercase();
    let source = skill.source.as_deref().unwrap_or("").to_ascii_lowercase();
    let topic_hit = |needle: &str| {
        skill
            .topics
            .iter()
            .any(|topic| topic.to_ascii_lowercase().contains(needle))
    };
    if name.contains("rust")
        || name.contains("cargo")
        || topic_hit("rust")
        || source.contains("rust")
    {
        return look(
            IconName::Code,
            (0x3a2414, 0x9a4d16, 0xfdba74),
            (0xfde8d3, 0xf3bd84, 0xb45309),
        );
    }
    if name.contains("git")
        || name.contains("pr")
        || name.contains("merge")
        || name.contains("stack")
        || topic_hit("pull request")
    {
        return look(
            if name.contains("pr") || name.contains("merge") {
                IconName::GitPullRequest
            } else {
                IconName::GitBranch
            },
            (0x2a1840, 0x6d28d9, 0xd8b4fe),
            (0xeee5fa, 0xcbb0f0, 0x7c3aed),
        );
    }
    if name.contains("browser")
        || name.contains("gif")
        || name.contains("video")
        || name.contains("record")
        || name.contains("screen")
    {
        return look(
            if name.contains("gif") || name.contains("record") {
                IconName::MonitorPlay
            } else {
                IconName::Video
            },
            (0x12343a, 0x0e7490, 0x67e8f9),
            (0xdceef2, 0x9fcfe0, 0x0e7490),
        );
    }
    if name.contains("doc")
        || name.contains("prose")
        || name.contains("translate")
        || name.contains("note")
        || name.contains("archive")
    {
        return look(
            if name.contains("translate") {
                IconName::Languages
            } else {
                IconName::BookOpen
            },
            (0x12352c, 0x047857, 0x6ee7b7),
            (0xddf0e6, 0xa4dcc0, 0x047857),
        );
    }
    if name.contains("cot")
        || name.contains("deepseek")
        || source.contains("deepseek")
        || name.contains("ai")
        || name.contains("reason")
    {
        return look(
            IconName::Sparkles,
            (0x1a2450, 0x3730a3, 0xa5b4fc),
            (0xe5e9f9, 0xb5bef0, 0x4338ca),
        );
    }
    if name.contains("test")
        || name.contains("check")
        || name.contains("review")
        || name.contains("lint")
        || name.contains("simplif")
    {
        return look(
            IconName::ShieldCheck,
            (0x3a2e12, 0xb45309, 0xfcd34d),
            (0xfdf0d0, 0xefcd77, 0xa16207),
        );
    }
    if name.contains("code") || name.contains("script") || name.contains("tool") {
        return look(
            IconName::FileCode,
            (0x10283c, 0x0369a1, 0x7dd3fc),
            (0xddeaf5, 0xa9cde6, 0x0369a1),
        );
    }
    look(
        IconName::Terminal,
        (0x17233c, 0x2f4d7a, 0x93c5fd),
        (0xe2e9f6, 0xb9c7e6, 0x1d4ed8),
    )
}
