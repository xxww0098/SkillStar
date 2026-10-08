//! Render regions of the page body: the card grid/list, its loading
//! skeletons, and the empty and remote scope states.

use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::*;

use super::MySkillsPage;
use super::empty_state::{
    self, render_channels_scope, render_empty_installed, render_empty_search, render_empty_updates,
    render_remote_scope,
};
use super::types::MySkillsScope;
use crate::chrome::{back_to_top_button, is_list_scrolled_down, pulse};
use crate::skill_card::{CARD_GAP, CardFace, card_placeholder, card_rows, grid_columns};
use crate::theme::palette;

/// Gap between full-width list rows. Grid rows use [`CARD_GAP`].
pub(crate) const LIST_GAP: f32 = 8.0;

impl MySkillsPage {
    pub(super) fn render_main_content(
        &self,
        content_width: f32,
        cx: &mut Context<Self>,
        view: WeakEntity<Self>,
    ) -> impl IntoElement {
        // Root of the replayed canvas. `size_full` fills the view bounds so the
        // scroller has a definite height and the cards stay inside it.
        let row = div()
            .flex()
            .size_full()
            .min_h_0()
            .min_w_0()
            .relative()
            .debug_selector(|| "skills-grid".into());
        let scroller = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .h_full()
            .overflow_y_scrollbar()
            .debug_selector(|| "skills-scroll".into());

        let columns = grid_columns(content_width);

        if self.loading {
            return self.pane(scroller.child(self.render_loading_grid(columns)), cx, row);
        }

        if let Some(err) = &self.error {
            return self.pane(
                scroller.child(
                    empty_state::centered_fill().child(
                        div()
                            .max_w(px(420.0))
                            .p_4()
                            .rounded_lg()
                            .bg(rgb(palette().danger_bg))
                            .border_1()
                            .border_color(rgb(palette().danger_border))
                            .text_sm()
                            .text_color(rgb(palette().danger_fg))
                            .text_center()
                            .child(crate::i18n::tf(
                                "settings.connectionFailed",
                                &[("error", err)],
                            )),
                    ),
                ),
                cx,
                row,
            );
        }

        // Scope switching: Remote / Channels / Local
        match self.scope {
            MySkillsScope::Remote => {
                return self.pane(scroller.child(render_remote_scope()), cx, row);
            }
            MySkillsScope::Channels => {
                return self.pane(
                    scroller.child(render_channels_scope(view, self.github.as_ref())),
                    cx,
                    row,
                );
            }
            MySkillsScope::Local => {}
        }

        // Local scope rendering
        if self.skills.is_empty() {
            return self.pane(scroller.child(render_empty_installed(view)), cx, row);
        }

        let filtered = self.filtered_skills();
        if filtered.is_empty() {
            if self.only_updates {
                return self.pane(scroller.child(render_empty_updates(view)), cx, row);
            } else {
                return self.pane(
                    scroller.child(render_empty_search(&self.search_query, view)),
                    cx,
                    row,
                );
            }
        }

        // The canvas paints installed cards through `render_virtual_pane`.
        // Landing here means the filter and the canvas slots disagree.
        self.pane(scroller, cx, row)
    }

    /// Card list. Only the canvas builds the rows; this pins the scroller,
    /// the selection pill, the detail column, and the back-to-top control.
    pub(super) fn render_virtual_pane(
        &self,
        list: impl IntoElement,
        on_back_to_top: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut scroll_col = div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .h_full()
            .px_5()
            .pt(px(crate::layout::SKILL_SCROLL_PT))
            .pb(px(crate::layout::SKILL_SCROLL_PB))
            .debug_selector(|| "skills-scroll".into())
            .child(list);
        // The selection pill overlays the last row. Extra bottom inset keeps
        // that row above the pill; without a selection the page pad is enough.
        if !self.selected_batch.is_empty() {
            scroll_col = scroll_col.pb_16();
        }

        let row = div()
            .flex()
            .size_full()
            .min_h_0()
            .min_w_0()
            .relative()
            .debug_selector(|| "skills-grid".into());
        let mut pane = self.pane(scroll_col, cx, row);
        if self.selected_skill.is_some() {
            pane = pane.child(self.render_detail_drawer(cx));
        }
        if is_list_scrolled_down(&self.skills_scroll) {
            let mut button = back_to_top_button(
                "skills-back-top",
                crate::i18n::t("mySkills.backToTop"),
                on_back_to_top,
            )
            .debug_selector(|| "skills-back-top".into());
            // The pane includes the floating sheet. Keep the control on the
            // card side, 32px from the sheet's left edge.
            if self.selected_skill.is_some() {
                button = button.right(px(super::detail_drawer::DRAWER_W + 32.0));
            }
            pane = pane.child(button);
        }
        pane
    }

    /// Card pane = scrolling region + the floating selection pill overlay.
    /// The row stays `.relative()` so the pill pins to the pane's bottom
    /// edge regardless of scroll offset.
    fn pane(&self, scroller: impl IntoElement, cx: &mut Context<Self>, row: Div) -> Div {
        row.child(scroller).child(self.render_selection_bar(cx))
    }

    /// Skeleton cards on the same tracks the loaded grid uses.
    fn render_loading_grid(&self, columns: usize) -> impl IntoElement {
        let mut tiles = Vec::with_capacity(6);
        for index in 0..6 {
            tiles.push(
                card_placeholder(
                    ElementId::Name(format!("skills-load-{index}").into()),
                    CardFace::Skill,
                )
                .p_3()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    pulse(format!("skills-load-{index}-title"))
                        .h_4()
                        .w(px(140.0)),
                )
                .child(pulse(format!("skills-load-{index}-line")).h_3().secondary())
                .child(
                    pulse(format!("skills-load-{index}-meta"))
                        .h_3()
                        .w(px(200.0))
                        .secondary(),
                )
                .into_any_element(),
            );
        }
        let grid = if self.view_list {
            full_width_rows(tiles, LIST_GAP)
        } else {
            card_rows(columns, CARD_GAP, tiles)
        };
        grid.p_5()
    }
}

/// One card per row, each row the width of the pane. The card itself is
/// `CardWidth::Fill`, so it grows with the row and does not stay on a
/// `CARD_W` track.
fn full_width_rows(tiles: Vec<AnyElement>, gap: f32) -> Div {
    let mut column = div().w_full().flex().flex_col().gap(px(gap));
    for tile in tiles {
        column = column.child(div().w_full().min_w_0().child(tile));
    }
    column
}

#[cfg(test)]
mod tests {
    use gpui_kit::component::Root;
    use gpui_kit::{
        AppContext as _, IntoElement, ParentElement as _, Render, Styled as _, div, px, size,
    };

    use super::super::MySkillsPage;
    use crate::layout::{SKILL_SCROLL_PB, SKILL_SCROLL_PT};

    struct Frame {
        page: gpui_kit::Entity<MySkillsPage>,
    }

    impl Render for Frame {
        fn render(
            &mut self,
            _: &mut gpui_kit::Window,
            _: &mut gpui_kit::Context<Self>,
        ) -> impl IntoElement {
            div()
                .size_full()
                .relative()
                .child(crate::chrome::replay_view(self.page.clone().into()))
        }
    }

    /// Four card strides fit the default window's track without scrolling:
    /// `WINDOW_H` minus the shell's vertical chrome is the page's track, and
    /// [`SKILL_SCROLL_PT`] / [`SKILL_SCROLL_PB`] are the remainder, so the
    /// last row lands fully inside the viewport (see `layout.rs`'s lock).
    /// The window is shortened by the shell chrome (insets 16 + borders 2)
    /// because this frame mounts the page without the panel.
    #[gpui_kit::test]
    fn four_rows_fit_the_default_track(cx: &mut gpui_kit::TestAppContext) {
        use ss_core::types::skill::{Skill, SkillType};

        crate::init_test(cx);
        let page = cx.new(MySkillsPage::new);
        cx.update(|cx| {
            page.update(cx, |page, cx| {
                page.loading = false;
                page.error = None;
                page.skills = (0..12)
                    .map(|index| {
                        let mut skill = Skill::from_skills_sh(
                            format!("skill-{index}"),
                            String::new(),
                            0,
                            "local".into(),
                            String::new(),
                        );
                        skill.skill_type = SkillType::Local;
                        skill
                    })
                    .collect();
                page.revise(cx);
            });
        });
        let shown = page.clone();
        let (_root, cx) = cx.add_window_view(move |window, cx| {
            let frame = cx.new(|_| Frame { page: shown });
            Root::new(frame, window, cx)
        });
        let shell_chrome = 18.0;
        cx.simulate_resize(size(
            px(crate::layout::WINDOW_W),
            px(crate::layout::WINDOW_H - shell_chrome),
        ));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let scroll = cx.debug_bounds("skills-scroll").expect("scroller missing");
        let row0 = cx.debug_bounds("skills-row-0").expect("first row missing");
        let row3 = cx.debug_bounds("skills-row-3").expect("fourth row missing");
        let top = f32::from(scroll.origin.y) + SKILL_SCROLL_PT;
        assert_eq!(f32::from(row0.origin.y), top, "first row off its track");
        let viewport_bottom = f32::from(scroll.origin.y) + f32::from(scroll.size.height);
        assert_eq!(
            f32::from(row3.origin.y) + f32::from(row3.size.height) + SKILL_SCROLL_PB,
            viewport_bottom,
            "four rows must fill the track exactly"
        );
    }
}
