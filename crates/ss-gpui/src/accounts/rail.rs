//! Provider rows for the accounts-mode sidebar.
//!
//! The shell's left menu hosts this list. The accounts page is only the
//! scrolling legend cards.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_usage::catalog::{CatalogEntry, catalog};

use super::AccountsPage;

use crate::accounts::theme::palette;
use crate::chrome::icon;
use crate::chrome::{InteractionSpring, MotionPaint};

/// Agent glyph id for a catalog provider. Brand files live under `agents/`.
pub fn icon_id(catalog_id: &str) -> &str {
    match catalog_id {
        "opencode-go" => "opencode",
        "xai" => "grok",
        "kimi" => "kimi-code-cli",
        other => other,
    }
}

fn brand_mark(catalog_id: &str, size: f32) -> impl IntoElement {
    img(crate::agent_icons::agent_icon_path(icon_id(catalog_id)))
        .w(px(size))
        .h(px(size))
        .flex_shrink_0()
}

impl AccountsPage {
    fn count_for(&self, catalog_id: &str) -> usize {
        self.subscriptions
            .iter()
            .filter(|sub| sub.catalog_id == catalog_id)
            .count()
    }

    pub(crate) fn visible_families(&self) -> Vec<CatalogEntry> {
        let all = catalog();
        match &self.selected_filter {
            None => all,
            Some(id) => all.into_iter().filter(|entry| entry.id == id).collect(),
        }
    }

    /// Accounts-mode sidebar body: 供应商, 全部, then every catalog family.
    pub(crate) fn render_provider_menu(&self, collapsed: bool, view: WeakEntity<Self>) -> Div {
        let mut list = div().w_full().flex().flex_col().gap(px(2.0));

        if !collapsed {
            list = list.child(
                div()
                    .px(px(10.0))
                    .pt(px(2.0))
                    .pb(px(4.0))
                    .text_size(px(11.0))
                    .text_color(rgb(palette().os_faint))
                    .child(crate::i18n::t("usage.sidebarNav")),
            );
        }

        let all_label = crate::i18n::t("usage.allProviders");
        list = list.child(self.menu_item(
            "rail-all",
            None,
            all_label.as_ref(),
            self.subscriptions.len(),
            true,
            collapsed,
            view.clone(),
        ));

        for entry in catalog() {
            let id = entry.id.to_string();
            list = list.child(self.menu_item(
                &format!("rail-{id}"),
                Some(id),
                entry.display_name,
                self.count_for(entry.id),
                false,
                collapsed,
                view.clone(),
            ));
        }

        list
    }

    fn menu_item(
        &self,
        element_id: &str,
        catalog_id: Option<String>,
        name: &str,
        count: usize,
        all: bool,
        collapsed: bool,
        view: WeakEntity<Self>,
    ) -> impl IntoElement {
        let selected = self.selected_filter == catalog_id;
        let paint = provider_row_paint(selected);
        let mark_size = if collapsed { 18.0 } else { 16.0 };
        let catalog_id_click = catalog_id.clone();
        let mark = if all {
            icon(
                gpui_kit::assets::IconName::LayoutGrid,
                mark_size,
                paint.glyph,
            )
            .into_any_element()
        } else {
            brand_mark(catalog_id.as_deref().unwrap_or(""), mark_size).into_any_element()
        };
        let mut rest = MotionPaint::new().fg(rgb(paint.label));
        if let Some(bg) = paint.bg {
            rest = rest.bg(rgb(bg));
        }
        let hover = MotionPaint::new()
            .fg(rgb(paint.label))
            .bg(rgb(paint.hover_bg));

        let row = div()
            .id(ElementId::Name(element_id.to_string().into()))
            .w_full()
            .min_h(px(32.0))
            .rounded(px(8.0))
            .flex()
            .items_center()
            .cursor_pointer()
            .when_some(paint.bg, |row, bg| row.bg(rgb(bg)))
            .interaction_spring(element_id.to_string(), true, rest, hover)
            .on_click(move |_, _, cx| {
                let next = catalog_id_click.clone();
                let _ = view.update(cx, |this, cx| {
                    this.selected_filter = next;
                    this.confirm_delete_id = None;
                    this.confirm_reset_id = None;
                    this.revise(cx);
                });
            })
            .child(mark);

        if collapsed {
            row.justify_center().py(px(6.0))
        } else {
            row.gap(px(10.0))
                .px(px(10.0))
                .py(px(4.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .text_size(px(12.5))
                        .font_weight(if selected {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .text_color(rgb(paint.label))
                        .child(name.to_string()),
                )
                .child(
                    div()
                        .flex_none()
                        .min_w(px(20.0))
                        .text_right()
                        .text_size(px(11.0))
                        .font_weight(if selected {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .text_color(rgb(paint.count))
                        .child(count.to_string()),
                )
        }
    }
}

/// Colors for one provider row. The list sits on the shell canvas, so a
/// card well (`os_fill_2`) disappears into that gray. Selection uses the
/// accent fill; hover stays a separate wash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProviderRowPaint {
    bg: Option<u32>,
    hover_bg: u32,
    label: u32,
    count: u32,
    glyph: u32,
}

fn provider_row_paint(selected: bool) -> ProviderRowPaint {
    let palette = palette();
    if selected {
        ProviderRowPaint {
            bg: Some(palette.accent),
            hover_bg: palette.accent_hover,
            label: palette.on_accent,
            count: palette.on_accent,
            glyph: palette.on_accent,
        }
    } else {
        ProviderRowPaint {
            bg: None,
            hover_bg: palette.panel_hover,
            label: palette.fg,
            count: palette.os_faint,
            glyph: palette.os_muted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{icon_id, palette, provider_row_paint};

    #[test]
    fn selected_provider_row_is_an_accent_fill_distinct_from_hover() {
        let selected = provider_row_paint(true);
        let idle = provider_row_paint(false);
        assert_eq!(selected.bg, Some(palette().accent));
        assert_eq!(selected.label, palette().on_accent);
        assert_eq!(selected.count, palette().on_accent);
        assert_ne!(selected.bg, Some(palette().os_fill_2));
        assert_eq!(idle.bg, None);
        assert_ne!(idle.hover_bg, selected.hover_bg);
        assert_ne!(idle.label, selected.label);
    }

    #[test]
    fn chatgpt_keeps_its_own_mark() {
        assert_eq!(icon_id("chatgpt"), "chatgpt");
        assert_eq!(icon_id("codex"), "codex");
    }
}
