//! Side detail column for a marketplace skill card.
//!
//! Same column as My Skills: a fixed-width sibling under the toolbar, not an
//! overlay. The grid recounts its columns from the width that remains.
//! Opening a card reads the local skill detail for that source and name.
//! A later response is dropped when the epoch or identity no longer matches.

mod column;

use gpui_kit::*;
use ss_core::types::skill::{Skill, extract_github_source_from_url};
use ss_marketplace::MarketplaceSkillDetails;
use ss_marketplace::snapshot::{
    LocalFirstResult, SnapshotStatus, get_skill_detail_local, sync_scope_skill_detail,
};

use super::MarketplacePage;
use crate::spawn_domain;

pub(crate) use column::market_detail_column;

/// Track the card grid reserves for the floating sheet, matching the skill
/// detail column: one card plus one gap ([`crate::layout::DETAIL_COLUMN_W`]),
/// so the open grid loses exactly one column. A test locks the two widths
/// together. The sheet itself paints [`crate::chrome::SHEET_GAP`] narrower
/// on each side of the track.
pub(super) const DRAWER_W: f32 = crate::layout::DETAIL_COLUMN_W;

/// Pane width the card grid may use. The open column's track is subtracted
/// before the column count is chosen, the same way My Skills does it; the
/// sheet's floating ring lives inside that track.
pub(super) fn market_grid_width(viewport_width: f32, drawer_open: bool) -> f32 {
    let width = crate::skill_card::card_content_width(viewport_width);
    if drawer_open { width - DRAWER_W } else { width }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct MarketSkillId {
    source: Option<String>,
    name: String,
}

impl crate::DomainOutput for MarketSkillId {
    fn from_panic(_panic: crate::DomainPanic) -> Self {
        Self {
            source: None,
            name: String::new(),
        }
    }
}

impl MarketSkillId {
    fn from_skill(skill: &Skill) -> Self {
        let source = skill
            .source
            .as_deref()
            .map(str::trim)
            .filter(|source| !source.is_empty())
            .map(str::to_string)
            .or_else(|| extract_github_source_from_url(&skill.git_url));
        Self {
            source,
            name: skill.name.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum DetailPhase {
    /// No source to fetch. The column shows the list row only.
    Idle,
    Loading,
    Ready(MarketplaceSkillDetails),
    /// Technical cause. The visible sentence comes from i18n.
    Failed(String),
}

struct MarketSelection {
    id: MarketSkillId,
    skill: Skill,
    phase: DetailPhase,
    /// One background sync per open. A stale or missing snapshot must not loop.
    refreshed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DetailIntent {
    None,
    Load {
        source: String,
        name: String,
        epoch: u64,
        refresh: bool,
    },
}

pub(super) struct MarketDetailController {
    selected: Option<MarketSelection>,
    epoch: u64,
}

impl MarketDetailController {
    pub(super) fn new() -> Self {
        Self {
            selected: None,
            epoch: 0,
        }
    }

    pub(super) fn is_open(&self) -> bool {
        self.selected.is_some()
    }

    pub(super) fn matches(&self, skill: &Skill) -> bool {
        self.selected
            .as_ref()
            .is_some_and(|selected| selected.id == MarketSkillId::from_skill(skill))
    }

    pub(super) fn selected_source(&self) -> Option<&str> {
        self.selected
            .as_ref()
            .and_then(|selected| selected.id.source.as_deref())
    }

    pub(super) fn skill(&self) -> Option<&Skill> {
        self.selected.as_ref().map(|selected| &selected.skill)
    }

    pub(super) fn phase(&self) -> Option<&DetailPhase> {
        self.selected.as_ref().map(|selected| &selected.phase)
    }

    pub(super) fn clear(&mut self) {
        if self.selected.is_none() {
            return;
        }
        self.selected = None;
        self.epoch = self.epoch.wrapping_add(1);
    }

    /// Same card again closes. A different card replaces the column and
    /// invalidates any detail request still in flight.
    pub(super) fn toggle(&mut self, skill: &Skill) -> DetailIntent {
        let id = MarketSkillId::from_skill(skill);
        if self
            .selected
            .as_ref()
            .is_some_and(|selected| selected.id == id)
        {
            self.clear();
            return DetailIntent::None;
        }
        self.epoch = self.epoch.wrapping_add(1);
        let epoch = self.epoch;
        let source = id.source.clone();
        self.selected = Some(MarketSelection {
            id,
            skill: skill.clone(),
            phase: if source.is_some() {
                DetailPhase::Loading
            } else {
                DetailPhase::Idle
            },
            refreshed: false,
        });
        match source {
            Some(source) => DetailIntent::Load {
                source,
                name: skill.name.clone(),
                epoch,
                refresh: false,
            },
            None => DetailIntent::None,
        }
    }

    pub(super) fn retry(&mut self) -> DetailIntent {
        let Some(selected) = &mut self.selected else {
            return DetailIntent::None;
        };
        let Some(source) = selected.id.source.clone() else {
            return DetailIntent::None;
        };
        let name = selected.id.name.clone();
        selected.phase = DetailPhase::Loading;
        selected.refreshed = true;
        self.epoch = self.epoch.wrapping_add(1);
        DetailIntent::Load {
            source,
            name,
            epoch: self.epoch,
            refresh: true,
        }
    }

    /// Keep the column only while the skill is still in the list that owns it.
    pub(super) fn retain(&mut self, skills: &[Skill]) {
        let Some(selected) = &self.selected else {
            return;
        };
        let id = selected.id.clone();
        if let Some(live) = skills
            .iter()
            .find(|skill| MarketSkillId::from_skill(skill) == id)
        {
            if let Some(selected) = &mut self.selected {
                selected.skill = live.clone();
            }
        } else {
            self.clear();
        }
    }

    pub(super) fn note_installed(&mut self, name: &str, installed: bool) {
        if let Some(selected) = &mut self.selected {
            if selected.skill.name == name {
                selected.skill.installed = installed;
            }
        }
    }

    /// Apply a detail response. Returns a follow-up load only for the first
    /// stale or missing snapshot of this selection.
    pub(super) fn apply(
        &mut self,
        epoch: u64,
        id: MarketSkillId,
        was_refresh: bool,
        result: anyhow::Result<LocalFirstResult<MarketplaceSkillDetails>>,
    ) -> DetailIntent {
        let Some(selected) = &mut self.selected else {
            return DetailIntent::None;
        };
        if epoch != self.epoch || selected.id != id {
            return DetailIntent::None;
        }
        match result {
            Ok(local) => self.apply_local(epoch, id, was_refresh, local),
            Err(err) => {
                if !matches!(selected.phase, DetailPhase::Ready(_)) {
                    selected.phase = DetailPhase::Failed(err.to_string());
                }
                DetailIntent::None
            }
        }
    }

    fn apply_local(
        &mut self,
        epoch: u64,
        id: MarketSkillId,
        was_refresh: bool,
        local: LocalFirstResult<MarketplaceSkillDetails>,
    ) -> DetailIntent {
        let Some(selected) = &mut self.selected else {
            return DetailIntent::None;
        };
        let empty = detail_body_empty(&local.data);
        let refreshable = matches!(
            local.snapshot_status,
            SnapshotStatus::Stale | SnapshotStatus::Miss
        );
        if refreshable && !selected.refreshed && !was_refresh {
            selected.refreshed = true;
            if !empty {
                selected.phase = DetailPhase::Ready(local.data);
            }
            if let Some(source) = id.source {
                return DetailIntent::Load {
                    source,
                    name: id.name,
                    epoch,
                    refresh: true,
                };
            }
            return DetailIntent::None;
        }
        if matches!(
            local.snapshot_status,
            SnapshotStatus::RemoteError | SnapshotStatus::ErrorFallback
        ) && empty
            && !matches!(selected.phase, DetailPhase::Ready(_))
        {
            selected.phase = DetailPhase::Failed(local.error.unwrap_or_default());
            return DetailIntent::None;
        }
        selected.phase = DetailPhase::Ready(local.data);
        DetailIntent::None
    }
}

fn detail_body_empty(details: &MarketplaceSkillDetails) -> bool {
    let summary = details.summary.as_deref().map(str::trim).unwrap_or("");
    let readme = details.readme.as_deref().map(str::trim).unwrap_or("");
    summary.is_empty() && readme.is_empty()
}

pub(super) fn spawn_market_detail<V, F>(
    view: &Entity<V>,
    cx: &mut Context<V>,
    intent: DetailIntent,
    apply: F,
) where
    V: 'static,
    F: FnOnce(
            &mut V,
            &mut Context<V>,
            u64,
            MarketSkillId,
            bool,
            anyhow::Result<LocalFirstResult<MarketplaceSkillDetails>>,
        ) + 'static,
{
    let DetailIntent::Load {
        source,
        name,
        epoch,
        refresh,
    } = intent
    else {
        return;
    };
    spawn_domain(
        view,
        cx,
        async move {
            if refresh {
                let _ = sync_scope_skill_detail(&source, &name).await;
            }
            let id = MarketSkillId {
                source: Some(source.clone()),
                name: name.clone(),
            };
            let result = get_skill_detail_local(&source, &name).await;
            (epoch, id, refresh, result)
        },
        move |this, cx, (epoch, id, refresh, result)| {
            apply(this, cx, epoch, id, refresh, result);
        },
    );
}

impl MarketplacePage {
    pub(super) fn open_market_skill(&mut self, skill: &Skill, cx: &mut Context<Self>) {
        let intent = self.detail.toggle(skill);
        self.follow_market_detail(intent, cx);
        self.revise(cx);
    }

    pub(super) fn close_market_detail(&mut self, cx: &mut Context<Self>) {
        self.detail.clear();
        self.revise(cx);
    }

    fn retry_market_detail(&mut self, cx: &mut Context<Self>) {
        let intent = self.detail.retry();
        self.follow_market_detail(intent, cx);
        self.revise(cx);
    }

    fn follow_market_detail(&mut self, intent: DetailIntent, cx: &mut Context<Self>) {
        let view = cx.entity();
        spawn_market_detail(&view, cx, intent, |this, cx, epoch, id, refresh, result| {
            let next = this.detail.apply(epoch, id, refresh, result);
            this.follow_market_detail(next, cx);
        });
    }

    pub(super) fn render_detail_column(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(skill) = self.detail.skill().cloned() else {
            return div().into_any_element();
        };
        let Some(phase) = self.detail.phase().cloned() else {
            return div().into_any_element();
        };
        let busy = self.busy.as_deref() == Some(skill.name.as_str());
        let install = !skill.installed;
        let url = skill.git_url.clone();
        let name = skill.name.clone();
        let view = cx.entity().downgrade();
        let retry_view = view.clone();
        let close_view = view.clone();
        market_detail_column(
            "mk",
            &skill,
            &phase,
            busy,
            move |_, _, cx| {
                let _ = close_view.update(cx, |this, cx| this.close_market_detail(cx));
            },
            move |_, _, cx| {
                let url = url.clone();
                let name = name.clone();
                let _ = view.update(cx, |this, cx| {
                    this.set_installed(url, name, install, cx);
                });
            },
            move |_, _, cx| {
                let _ = retry_view.update(cx, |this, cx| this.retry_market_detail(cx));
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DRAWER_W, DetailIntent, DetailPhase, MarketDetailController, MarketSkillId,
        market_grid_width,
    };
    use crate::skill_card::grid_columns;
    use ss_core::types::skill::{Skill, SkillCategory, SkillType};
    use ss_marketplace::MarketplaceSkillDetails;
    use ss_marketplace::snapshot::{LocalFirstResult, SnapshotStatus};

    fn fixture(name: &str, source: Option<&str>) -> Skill {
        Skill {
            name: name.to_string(),
            description: String::new(),
            localized_description: None,
            skill_type: SkillType::Hub,
            stars: 0,
            installed: false,
            update_available: false,
            upstream_change: None,
            last_updated: String::new(),
            git_url: String::new(),
            tree_hash: None,
            category: SkillCategory::None,
            author: None,
            topics: Vec::new(),
            agent_links: None,
            rank: None,
            source: source.map(str::to_string),
        }
    }

    fn payload(summary: &str, status: SnapshotStatus) -> LocalFirstResult<MarketplaceSkillDetails> {
        LocalFirstResult {
            data: MarketplaceSkillDetails {
                summary: Some(summary.to_string()),
                readme: None,
                weekly_installs: None,
                github_stars: None,
                first_seen: None,
                security_audits: Vec::new(),
            },
            snapshot_status: status,
            snapshot_updated_at: None,
            error: None,
        }
    }

    #[test]
    fn drawer_matches_the_skill_column_width() {
        assert_eq!(DRAWER_W, crate::my_skills::detail_drawer::DRAWER_W);
    }

    #[test]
    fn an_open_column_narrows_the_grid() {
        let viewport = crate::layout::WINDOW_W;
        let closed = market_grid_width(viewport, false);
        let open = market_grid_width(viewport, true);
        assert_eq!(open, closed - DRAWER_W);
        assert!(grid_columns(open) <= grid_columns(closed));
    }

    #[test]
    fn an_open_column_keeps_two_cards_in_the_default_window() {
        let open = market_grid_width(crate::layout::WINDOW_W, true);
        let two_cards = crate::skill_card::CARD_W * 2.0 + crate::skill_card::CARD_GAP;
        assert_eq!(grid_columns(open), 2);
        assert_eq!(
            open, two_cards,
            "the open pane is exactly two card tracks, no remainder"
        );
    }

    #[test]
    fn clicking_the_open_card_closes_the_column() {
        let mut detail = MarketDetailController::new();
        let skill = fixture("find-skills", Some("vercel-labs/skills"));
        assert!(matches!(detail.toggle(&skill), DetailIntent::Load { .. }));
        assert!(detail.is_open());
        assert!(matches!(detail.toggle(&skill), DetailIntent::None));
        assert!(!detail.is_open());
    }

    #[test]
    fn a_card_without_source_does_not_fetch() {
        let mut detail = MarketDetailController::new();
        let skill = fixture("local-only", None);
        assert!(matches!(detail.toggle(&skill), DetailIntent::None));
        assert!(matches!(detail.phase(), Some(DetailPhase::Idle)));
    }

    #[test]
    fn source_falls_back_to_the_git_url() {
        let mut skill = fixture("find-skills", None);
        skill.git_url = "https://github.com/vercel-labs/skills".to_string();
        assert_eq!(
            MarketSkillId::from_skill(&skill).source.as_deref(),
            Some("vercel-labs/skills")
        );
    }

    #[test]
    fn a_stale_response_does_not_replace_the_new_selection() {
        let mut detail = MarketDetailController::new();
        let first = fixture("find-skills", Some("vercel-labs/skills"));
        let DetailIntent::Load { epoch, .. } = detail.toggle(&first) else {
            panic!("expected a detail load");
        };
        let second = fixture("grill-me", Some("mattpocock/skills"));
        let _ = detail.toggle(&second);
        let ignored = detail.apply(
            epoch,
            MarketSkillId::from_skill(&first),
            false,
            Ok(payload("old", SnapshotStatus::Fresh)),
        );
        assert!(matches!(ignored, DetailIntent::None));
        assert_eq!(detail.skill().unwrap().name, "grill-me");
        assert!(matches!(detail.phase(), Some(DetailPhase::Loading)));
    }

    #[test]
    fn a_stale_snapshot_refreshes_once_and_keeps_the_summary() {
        let mut detail = MarketDetailController::new();
        let skill = fixture("find-skills", Some("vercel-labs/skills"));
        let DetailIntent::Load { epoch, .. } = detail.toggle(&skill) else {
            panic!("expected a detail load");
        };
        let id = MarketSkillId::from_skill(&skill);
        let next = detail.apply(
            epoch,
            id.clone(),
            false,
            Ok(payload("Helps users", SnapshotStatus::Stale)),
        );
        assert!(matches!(next, DetailIntent::Load { refresh: true, .. }));
        assert!(matches!(detail.phase(), Some(DetailPhase::Ready(_))));
        let failed = detail.apply(epoch, id, true, Err(anyhow::anyhow!("network")));
        assert!(matches!(failed, DetailIntent::None));
        assert!(matches!(detail.phase(), Some(DetailPhase::Ready(_))));
    }

    #[test]
    fn leaving_the_list_closes_the_column() {
        let mut detail = MarketDetailController::new();
        let skill = fixture("find-skills", Some("vercel-labs/skills"));
        let _ = detail.toggle(&skill);
        detail.retain(&[]);
        assert!(!detail.is_open());
    }
}
