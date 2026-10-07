//! The source pill every skill surface shares: a small bordered chip that
//! names where a skill came from — a repo path or an author handle — and opens
//! that page when clicked.
//!
//! A surface supplies the label, the link, and what a click should do; the
//! chip owns its own hover motion. That keeps market-card and skill-card
//! painting the same control from one place instead of each
//! re-deriving a source line.

use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::*;
use ss_core::types::skill::Skill;

use crate::chrome::{InteractionSpring, MotionPaint};
use crate::theme::palette;

/// A source pill. [`skill_source_chip`] derives one from a skill; the
/// constructors cover the two shapes directly.
pub(crate) struct SourceChip {
    /// Page-unique id. Hover state and the spring are keyed by it.
    id: SharedString,
    label: SharedString,
    url: Option<SharedString>,
    glyph: Option<IconName>,
}

impl SourceChip {
    pub(crate) fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            url: None,
            glyph: None,
        }
    }

    /// Owner/repo path: branch glyph, opens the repository.
    pub(crate) fn repo(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        url: impl Into<SharedString>,
    ) -> Self {
        Self::new(id, label).glyph(IconName::GitBranch).link(url)
    }

    /// @handle: text only, opens the profile.
    pub(crate) fn author(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        url: impl Into<SharedString>,
    ) -> Self {
        Self::new(id, label).link(url)
    }

    pub(crate) fn glyph(mut self, glyph: IconName) -> Self {
        self.glyph = Some(glyph);
        self
    }

    pub(crate) fn link(mut self, url: impl Into<SharedString>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// Paint the chip. on_open receives the chip's url after the click has
    /// been kept from reaching the card behind it.
    pub(crate) fn render(
        self,
        _cx: &App,
        on_open: impl Fn(SharedString, &mut App) + 'static,
    ) -> impl IntoElement {
        let Self {
            id,
            label,
            url,
            glyph,
        } = self;
        let chip_id: SharedString = format!("{id}-chip").into();
        let mut chip = div()
            .group(id.clone())
            .id(ElementId::Name(chip_id.clone()))
            .flex()
            .items_center()
            .gap_1()
            .self_start()
            // One shared cap keeps a long repo path from crowding the card.
            .max_w(px(180.0))
            .px_1p5()
            .py(px(1.0))
            .rounded(px(6.0))
            .border_1()
            .text_size(px(10.0));
        if let Some(glyph) = glyph {
            chip = chip.child(
                div()
                    .opacity(0.7)
                    .group_hover(id.clone(), |style| {
                        style.text_color(rgb(palette().accent)).opacity(1.0)
                    })
                    .child(Icon::new(glyph).with_size(px(10.0))),
            );
        }
        chip = chip.child(
            div()
                .truncate()
                .group_hover(id.clone(), |style| style.underline())
                .child(label.to_string()),
        );
        if let Some(url) = url {
            chip = chip.cursor_pointer().on_click(move |_, _, app| {
                app.stop_propagation();
                on_open(url.clone(), app);
            });
        }
        let rest_bg = rgb(palette().well);
        let hot_bg = rgb(palette().card_hover);
        let rest_border = rgb(palette().border);
        let hot_border = rgb(palette().card_hover).blend(rgb(palette().accent).alpha(0.55));
        let rest_text = rgb(palette().tag);
        let hot_text = rgb(palette().fg);
        chip.bg(rest_bg)
            .border_color(rest_border)
            .text_color(rest_text)
            .interaction_spring(
                chip_id,
                true,
                MotionPaint::new()
                    .bg(rest_bg)
                    .border(rest_border)
                    .fg(rest_text),
                MotionPaint::new()
                    .bg(hot_bg)
                    .border(hot_border)
                    .fg(hot_text),
            )
    }
}

/// The source pill for a skill: the repo path when the skill carries one,
/// otherwise the author handle. None when it has neither, so the caller
/// omits the row instead of backfilling it.
pub(crate) fn skill_source_chip(scope: &str, skill: &Skill) -> Option<SourceChip> {
    if let Some(source) = skill
        .source
        .as_deref()
        .filter(|source| !source.is_empty() && *source != "remote")
    {
        let url = source_url(skill).unwrap_or_else(|| format!("https://github.com/{source}"));
        return Some(SourceChip::repo(
            format!("{scope}-repo-link-{}", skill.name),
            source.to_string(),
            url,
        ));
    }
    skill.author.as_ref().map(|author| {
        SourceChip::author(
            format!("{scope}-author-link-{}", skill.name),
            format!("@{author}"),
            format!("https://github.com/{}", author.trim_start_matches('@')),
        )
    })
}

/// The page a skill's repo chip opens: the tracked git url when it is already
/// an http link, otherwise the owner/repo source path.
pub(crate) fn source_url(skill: &Skill) -> Option<String> {
    if skill.git_url.starts_with("http") {
        Some(skill.git_url.clone())
    } else {
        skill
            .source
            .as_ref()
            .map(|source| format!("https://github.com/{source}"))
    }
}
