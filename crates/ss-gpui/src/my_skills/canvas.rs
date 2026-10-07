//! Installed-skill grid, kept off the animation frame.
//!
//! A refresh spin and the kit dialog notify the page or the window root.
//! Those frames must not lay every card out again. Each card is its own
//! view, so the updating ellipsis notifies that card and the others replay.
//!
//! Scroll and hover also notify this view. The rows go through `uniform_list`
//! so only the visible ones are built; laying out the whole library here is
//! what made the page stutter.

use std::ops::Range;
use std::rc::Rc;

use gpui_kit::*;

use super::detail_drawer;
use super::skill_card::{SkillCardEmit, SkillCardEvent, prefetch_skill_avatar, render_skill_card};
use super::types::MySkillsScope;
use super::view::LIST_GAP;
use super::{MySkillsPage, description_source};
use crate::skill_card::{CARD_GAP, CARD_H, CARD_W, card_row, grid_columns};

struct SkillCardView {
    props: super::skill_card::SkillCardProps,
    emit: SkillCardEmit,
}

impl Render for SkillCardView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        prefetch_skill_avatar(&self.props.skill, &cx.entity(), cx);
        render_skill_card(self.props.clone(), self.emit.clone(), cx)
    }
}

pub(super) struct SkillsCanvas {
    page: Entity<MySkillsPage>,
    content_epoch: u64,
    translation_epoch: u64,
    primed: bool,
    slots: Vec<(String, Entity<SkillCardView>)>,
    _observe: Subscription,
}

impl SkillsCanvas {
    fn new(
        page: Entity<MySkillsPage>,
        content_epoch: u64,
        translation_epoch: u64,
        cx: &mut Context<Self>,
    ) -> Self {
        // `page` is leased by the render that constructs this view.
        let observe = cx.observe(&page, |this, page, cx| {
            let (content, translation) = page.read(cx).replay_epochs();
            if content != this.content_epoch || translation != this.translation_epoch {
                cx.notify();
            }
        });
        Self {
            page,
            content_epoch,
            translation_epoch,
            primed: false,
            slots: Vec::new(),
            _observe: observe,
        }
    }

    fn resync(&mut self, cx: &mut Context<Self>) {
        let page = self.page.clone();
        let specs = {
            let page = page.read(cx);
            page.filtered_skills()
                .into_iter()
                .map(|skill| {
                    let name = skill.name.clone();
                    let props = page.skill_card_props(&skill);
                    (name, props)
                })
                .collect::<Vec<_>>()
        };
        let sources: Vec<String> = specs
            .iter()
            .filter_map(|(_, props)| description_source(&props.skill).map(str::to_string))
            .collect();
        let mut next = Vec::with_capacity(specs.len());
        for (name, props) in specs {
            let emit = card_emit(page.downgrade(), name.clone());
            if let Some((_, card)) = self.slots.iter().find(|(existing, _)| existing == &name) {
                let card = card.clone();
                card.update(cx, |slot, cx| {
                    slot.props = props;
                    slot.emit = emit;
                    cx.notify();
                });
                next.push((name, card));
            } else {
                let card = cx.new(|_| SkillCardView { props, emit });
                next.push((name, card));
            }
        }
        self.slots = next;
        page.update(cx, |_, cx| {
            crate::translation::schedule(
                &cx.entity(),
                sources,
                crate::translation::Surface::Description,
                cx,
            );
        });
    }

    /// Visible rows only. The first row's height is reused for every row, so
    /// each line sets `CARD_H` plus the gap and nothing else.
    fn skill_list(&self, columns: usize, list_mode: bool, cx: &mut Context<Self>) -> UniformList {
        let columns = columns.max(1);
        let gap = if list_mode { LIST_GAP } else { CARD_GAP };
        let rows = self.slots.len().div_ceil(columns);
        let scroll = self.page.read(cx).skills_scroll.clone();
        uniform_list(
            SharedString::from(if list_mode {
                "skills-cards-list"
            } else {
                "skills-cards-grid"
            }),
            rows,
            cx.processor(move |this, range: Range<usize>, _window, _cx| {
                let mut lines = Vec::with_capacity(range.end.saturating_sub(range.start));
                for row in range {
                    let start = row.saturating_mul(columns);
                    if start >= this.slots.len() {
                        break;
                    }
                    let end = (start + columns).min(this.slots.len());
                    let tiles = this.slots[start..end]
                        .iter()
                        .map(|(_, card)| {
                            let style = if list_mode {
                                StyleRefinement::default().h(px(CARD_H)).w_full()
                            } else {
                                StyleRefinement::default().h(px(CARD_H)).w(px(CARD_W))
                            };
                            card.clone().cached(style).into_any_element()
                        })
                        .collect::<Vec<_>>();
                    lines.push(skill_line(list_mode, columns, gap, tiles, row));
                }
                lines
            }),
        )
        .track_scroll(&scroll)
        .flex_1()
        .min_h_0()
        .w_full()
    }
}

/// One virtual-list row. List mode is a single full-width card. Grid mode
/// uses the shared tracks; a short row is padded so its cards stay `CARD_W`.
fn skill_line(
    list_mode: bool,
    columns: usize,
    gap: f32,
    tiles: Vec<AnyElement>,
    row: usize,
) -> Div {
    let stride = CARD_H + gap;
    let line = if list_mode {
        let mut column = div().w_full().h(px(stride)).pb(px(gap));
        for tile in tiles {
            column = column.child(div().w_full().min_w_0().child(tile));
        }
        column
    } else {
        card_row(columns, gap, tiles).h(px(stride)).pb(px(gap))
    };
    line.debug_selector(|| format!("skills-row-{row}"))
}

fn card_emit(page: WeakEntity<MySkillsPage>, name: String) -> SkillCardEmit {
    Rc::new(move |event, app| {
        if let SkillCardEvent::OpenLink(url) = &event {
            app.open_url(url);
            return;
        }
        let _ = page.update(app, |this, cx| this.on_skill_card(&name, event, cx));
    })
}

impl Render for SkillsCanvas {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (content, translation) = self.page.read(cx).replay_epochs();
        if !self.primed || content != self.content_epoch || translation != self.translation_epoch {
            self.primed = true;
            self.content_epoch = content;
            self.translation_epoch = translation;
            self.resync(cx);
        }
        let (list_mode, scope, loading, failed, cards) = {
            let page = self.page.read(cx);
            (
                page.view_list,
                page.scope,
                page.loading,
                page.error.is_some(),
                !self.slots.is_empty(),
            )
        };
        let width = {
            let page = self.page.read(cx);
            let mut width =
                crate::skill_card::card_content_width(f32::from(window.viewport_size().width));
            if page.selected_skill.is_some() {
                width -= detail_drawer::DRAWER_W;
            }
            width
        };
        let page = self.page.clone();
        if !loading && !failed && scope == MySkillsScope::Local && cards {
            let columns = if list_mode {
                1
            } else {
                grid_columns(width).max(1)
            };
            let list = self.skill_list(columns, list_mode, cx);
            let scroll = self.page.read(cx).skills_scroll.clone();
            let canvas = cx.entity().downgrade();
            return page.update(cx, move |page, cx| {
                page.render_virtual_pane(
                    list,
                    move |_, _, cx| {
                        cx.stop_propagation();
                        scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
                        let _ = canvas.update(cx, |_, cx| cx.notify());
                    },
                    cx,
                )
                .into_any_element()
            });
        }
        page.update(cx, |page, cx| {
            let view = cx.entity().downgrade();
            page.render_main_content(width, cx, view).into_any_element()
        })
    }
}

impl MySkillsPage {
    pub(super) fn ensure_canvas(&mut self, cx: &mut Context<Self>) -> Entity<SkillsCanvas> {
        if let Some(canvas) = &self.canvas {
            return canvas.clone();
        }
        let page = cx.entity();
        let (content_epoch, translation_epoch) = self.replay_epochs();
        let canvas = cx.new(|cx| SkillsCanvas::new(page, content_epoch, translation_epoch, cx));
        self.canvas = Some(canvas.clone());
        canvas
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::component::Root;
    use gpui_kit::{
        AppContext as _, IntoElement, ParentElement as _, Render, Styled as _, div, px, size,
    };

    use super::super::MySkillsPage;

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

    /// The replayed grid has to keep the pane's height. A flex root with no
    /// definite height collapses, and the scroller clips every card.
    ///
    /// The page is built on the app context before the window opens, like the
    /// sibling tests: constructing it inside `add_window_view` binds the
    /// construction-time domain refresh to the window executor, and its real
    /// tokio wakeup then trips the test scheduler's determinism guard on CI.
    #[gpui_kit::test]
    fn replayed_grid_fills_the_page(cx: &mut gpui_kit::TestAppContext) {
        use ss_core::types::skill::{Skill, SkillType};

        cx.update(|cx| gpui_kit::init(cx));
        let page = cx.new(MySkillsPage::new);
        cx.update(|cx| {
            page.update(cx, |page, cx| {
                page.loading = false;
                page.error = None;
                page.skills = (0..3)
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
        cx.simulate_resize(size(px(1400.), px(800.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let body = cx.debug_bounds("skills-body").expect("skills body missing");
        let grid = cx.debug_bounds("skills-grid").expect("skills grid missing");
        let scroll = cx
            .debug_bounds("skills-scroll")
            .expect("skills scroller missing");
        assert!(
            body.size.height > px(400.),
            "skills body collapsed: {body:?}"
        );
        assert!(
            grid.size.height > px(400.),
            "skills grid collapsed inside {body:?}: {grid:?}"
        );
        assert!(
            scroll.size.height > px(400.),
            "skills scroller collapsed inside {grid:?}: {scroll:?}"
        );
    }

    /// `ensure_canvas` runs inside the page render, which already holds the lease.
    #[gpui_kit::test]
    fn canvas_can_be_created_while_the_page_is_updating(cx: &mut gpui_kit::TestAppContext) {
        cx.update(|cx| gpui_kit::init(cx));
        let page = cx.new(|cx| MySkillsPage::new(cx));
        cx.update(|cx| {
            page.update(cx, |page, cx| {
                let _canvas = page.ensure_canvas(cx);
            });
        });
    }

    /// A long library must not lay out every row. The back-to-top control
    /// stays hidden until the list is actually scrolled.
    #[gpui_kit::test]
    fn visible_rows_only_and_back_to_top(cx: &mut gpui_kit::TestAppContext) {
        use gpui_kit::point;
        use ss_core::types::skill::{Skill, SkillType};

        cx.update(|cx| gpui_kit::init(cx));
        let page = cx.new(|cx| MySkillsPage::new(cx));
        cx.update(|cx| {
            page.update(cx, |page, cx| {
                page.loading = false;
                page.error = None;
                page.skills = (0..30)
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
            let frame = cx.new(|_| Frame {
                page: shown.clone(),
            });
            Root::new(frame, window, cx)
        });
        cx.simulate_resize(size(px(1400.), px(800.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let count = cx.update(|_window, app| page.read(app).skills.len());
        assert_eq!(count, 30, "the fixture list was replaced before layout");
        assert!(
            cx.debug_bounds("skills-row-0").is_some(),
            "the first row should be on screen"
        );
        assert!(
            cx.debug_bounds("skills-row-9").is_none(),
            "a row below the viewport was laid out"
        );
        assert!(
            cx.debug_bounds("skills-back-top").is_none(),
            "back to top showed at the top of the list"
        );

        cx.update(|_window, cx| {
            page.update(cx, |page, cx| {
                page.skills_scroll
                    .0
                    .borrow_mut()
                    .base_handle
                    .set_offset(point(px(0.0), px(-400.0)));
                page.revise(cx);
            });
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(
            cx.debug_bounds("skills-back-top").is_some(),
            "back to top stayed hidden after scrolling"
        );
    }

    /// The link menu paints over the grid. The card under the pointer must
    /// not take the hover face; a card the menu does not cover still does.
    #[gpui_kit::test]
    fn link_menu_keeps_the_pointer_off_the_card_under_it(cx: &mut gpui_kit::TestAppContext) {
        use gpui_kit::px;
        use ss_core::types::skill::{Skill, SkillType};

        use super::super::skill_card::{reset_skill_card_hover, skill_card_is_hovered};

        cx.update(|cx| gpui_kit::init(cx));
        let page = cx.new(|cx| MySkillsPage::new(cx));
        cx.update(|cx| {
            page.update(cx, |page, cx| {
                page.loading = false;
                page.error = None;
                page.view_list = true;
                page.skills = (0..8)
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
                page.selected_batch.insert("skill-0".into());
                page.link_menu_open = true;
                page.revise(cx);
            });
        });
        let shown = page.clone();
        let (_root, mut cx) = cx.add_window_view(move |window, cx| {
            let frame = cx.new(|_| Frame {
                page: shown.clone(),
            });
            Root::new(frame, window, cx)
        });
        cx.simulate_resize(size(px(1100.), px(900.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let menu = cx
            .debug_bounds("selection-link-menu")
            .expect("link menu should be open");
        let mut covered = None;
        let mut clear = None;
        for index in 0..8 {
            let name = format!("skill-{index}");
            let selector: &'static str = Box::leak(format!("skill-card-{name}").into_boxed_str());
            let Some(bounds) = cx.debug_bounds(selector) else {
                continue;
            };
            if covered.is_none()
                && let Some(at) = overlap_center(menu, bounds)
            {
                covered = Some((name.clone(), at));
            }
            if clear.is_none() && !bounds.intersects(&menu) {
                clear = Some((name, bounds.center()));
            }
        }
        let (covered_name, covered_at) = covered.expect("the open menu should cover a skill card");
        let (clear_name, clear_at) = clear.expect("one skill card should sit clear of the menu");

        reset_skill_card_hover();
        cx.update(|window, cx| window.simulate_mouse_move(covered_at, cx));
        assert!(
            !skill_card_is_hovered(&covered_name),
            "card {covered_name} hovered while the pointer was on the link menu",
        );

        cx.update(|window, cx| window.simulate_mouse_move(clear_at, cx));
        assert!(
            skill_card_is_hovered(&clear_name),
            "card {clear_name} did not hover when the pointer was on it",
        );
    }

    fn overlap_center(
        front: gpui_kit::Bounds<gpui_kit::Pixels>,
        back: gpui_kit::Bounds<gpui_kit::Pixels>,
    ) -> Option<gpui_kit::Point<gpui_kit::Pixels>> {
        if !front.intersects(&back) {
            return None;
        }
        Some(front.intersect(&back).center())
    }
}
