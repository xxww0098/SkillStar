//! Page toolbar — the `h-14` row in `PageToolbar.tsx`, plus the controls
//! `Toolbar.tsx` drops into its search, filter, and action slots.
//!
//! On macOS the overlay titlebar puts the traffic lights in the sidebar
//! lane. This row is the rest of that band: title, search, filters, then
//! the slack, then actions. A drag layer sits behind the whole row, so
//! every pixel that is not a control moves the window. Controls call
//! `occlude()` — a press otherwise hits that layer too, because a normal
//! hitbox does not block the one behind it.

use gpui_kit::assets::IconName;
use gpui_kit::component::InteractiveElementExt as _;
use gpui_kit::component::input::{Escape, Input, InputState};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::icon;
use super::{InteractionSpring, MotionDiv, MotionPaint};
use crate::theme::palette;

/// Drag moves the window, double-click zooms. `id` must be unique in the
/// live tree. The element must be empty: this does not look at which child
/// was pressed, and a normal hitbox does not hide the layer behind it.
pub(crate) fn window_drag(id: impl Into<ElementId>, el: Div) -> Stateful<Div> {
    el.id(id)
        .window_control_area(WindowControlArea::Drag)
        .on_mouse_down(MouseButton::Left, |_, window, _| {
            window.start_window_move();
        })
        .on_double_click(|_, window, _| {
            window.titlebar_double_click();
        })
}

/// Three-zone toolbar. Build with [`page_toolbar`], then `search` / `filter`
/// / `action`, then [`PageBar::build`].
pub(crate) struct PageBar {
    title: AnyElement,
    extras: Vec<AnyElement>,
    search: Option<AnyElement>,
    filters: Vec<AnyElement>,
    actions: Vec<AnyElement>,
    drag_id: &'static str,
}

impl PageBar {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self::title_row(
            div()
                .text_sm()
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(palette().fg))
                .child(title.into()),
        )
    }

    /// Title cluster that already includes its own leading controls
    /// (back button, scope switch). The zone divider still follows it.
    pub fn title_row(row: impl IntoElement) -> Self {
        Self {
            title: row.into_any_element(),
            extras: Vec::new(),
            search: None,
            filters: Vec::new(),
            actions: Vec::new(),
            drag_id: "page-toolbar-drag",
        }
    }

    pub fn drag_id(mut self, id: &'static str) -> Self {
        self.drag_id = id;
        self
    }

    pub fn extra(mut self, el: impl IntoElement) -> Self {
        self.extras.push(el.into_any_element());
        self
    }

    pub fn search(mut self, el: impl IntoElement) -> Self {
        self.search = Some(el.into_any_element());
        self
    }

    pub fn filter(mut self, el: impl IntoElement) -> Self {
        self.filters.push(el.into_any_element());
        self
    }

    pub fn action(mut self, el: impl IntoElement) -> Self {
        self.actions.push(el.into_any_element());
        self
    }

    pub fn build(self) -> Div {
        let mut cluster = div().flex().items_center().gap_3().child(self.title);
        for extra in self.extras {
            cluster = cluster.child(extra);
        }
        let title_zone = div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .h(px(32.0))
            .child(cluster)
            .child(
                div()
                    .w(px(1.0))
                    .h(px(20.0))
                    .ml(px(16.0))
                    .mr(px(4.0))
                    .bg(rgb(palette().border)),
            );

        // Filters keep their content width. `overflow_x_scrollbar` wraps
        // them in a `size_full` root, so without an explicit auto width that
        // root would claim the whole zone and the slack in the middle
        // would disappear.
        let mut filters = div()
            .flex()
            .items_center()
            .gap_2()
            .min_w_0()
            .w_auto()
            .overflow_x_scrollbar();
        for filter in self.filters {
            filters = filters.child(filter);
        }
        let center = div()
            .flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .items_center()
            .gap(px(8.0))
            .child(filters)
            .child(div().flex_1().min_w_0().h_full());

        let mut actions = div().flex().items_center().gap_2().flex_shrink_0();
        for action in self.actions {
            actions = actions.child(action);
        }

        // 24px gutters, 12px between zones. Those pixels belong to this row,
        // which has no hitbox, so the layer behind receives the press.
        let mut row = div()
            .flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .items_center()
            .px(px(24.0))
            .gap(px(12.0))
            .child(title_zone);
        if let Some(search) = self.search {
            row = row.child(search);
        }
        row = row.child(center.overflow_hidden()).child(actions);

        div()
            .relative()
            .flex()
            .h(px(56.0))
            .w_full()
            .flex_shrink_0()
            .border_b_1()
            .border_color(rgb(palette().border))
            .child(window_drag(self.drag_id, div().absolute().inset_0()))
            .child(row)
    }
}

impl IntoElement for PageBar {
    type Element = Div;

    fn into_element(self) -> Self::Element {
        self.build()
    }
}

pub(crate) fn page_toolbar(title: impl Into<SharedString>) -> PageBar {
    PageBar::new(title)
}

/// `h-8` count chip. React puts a Layers glyph and a tabular number here.
pub(crate) fn bar_count(glyph: IconName, label: impl Into<SharedString>) -> Div {
    div()
        .h(px(32.0))
        .px_3()
        .flex()
        .items_center()
        .gap(px(6.0))
        .flex_shrink_0()
        .rounded_lg()
        .border_1()
        .border_color(rgb(palette().border))
        .bg(rgb(palette().bg))
        .text_xs()
        .font_weight(FontWeight::BOLD)
        .text_color(rgb(palette().fg))
        .child(icon(glyph, 14.0, palette().accent))
        .child(label.into())
}

/// 32×32 refresh control. `spin` turns the glyph until that request ends.
pub(crate) fn bar_refresh_button(id: &'static str, spin: bool) -> MotionDiv {
    bar_icon_button(id, IconName::RefreshCw, spin)
}

/// 32×32 icon control. `spin` turns `glyph` until that request ends.
pub(crate) fn bar_icon_button(id: &'static str, glyph: IconName, spin: bool) -> MotionDiv {
    let color = if spin {
        palette().accent
    } else {
        palette().fg_muted
    };
    let rest_bg = rgb(palette().bg);
    let hover_bg = rgb(if spin {
        palette().bg
    } else {
        palette().card_hover
    });
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .size(px(32.0))
        .flex_shrink_0()
        .rounded_lg()
        .border_1()
        .border_color(rgb(palette().border))
        .bg(rest_bg)
        .when(spin, |button| button.opacity(0.55))
        .when(!spin, |button| button.cursor_pointer())
        .child(super::icon_spin(
            ElementId::Name(format!("{id}-spin").into()),
            glyph,
            14.0,
            color,
            spin,
        ))
        .occlude()
        .interaction_spring(
            id,
            !spin,
            MotionPaint::new().bg(rest_bg),
            MotionPaint::new().bg(hover_bg),
        )
}

pub(crate) fn bar_primary(
    id: &'static str,
    glyph: IconName,
    label: impl Into<SharedString>,
) -> MotionDiv {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(6.0))
        .h(px(32.0))
        .px_3()
        .flex_shrink_0()
        .rounded_lg()
        .bg(rgb(palette().accent))
        .cursor_pointer()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(palette().on_accent))
        .child(icon(glyph, 14.0, palette().on_accent))
        .child(label.into())
        .occlude()
        .interaction_spring(
            id,
            true,
            MotionPaint::new().opacity(1.0),
            MotionPaint::new().opacity(0.9),
        )
}

/// Inline search. Preferred width is `width`; the field yields down to
/// 160px before the filters beside it are clipped.
///
/// GPUI keeps a text field focused until something blurs it. A click on
/// blank page or the toolbar drag layer does not, and the field's Escape
/// only dismisses its own menu, extra cursors, or completion, then
/// propagates. This wrapper blurs on a press outside its bounds (capture,
/// so the press still starts a window drag) and on Escape after that
/// propagate. `occlude` keeps a press inside the field off the drag layer.
pub(crate) fn toolbar_search(state: &Entity<InputState>, width: f32) -> Div {
    let state = state.clone();
    let focus_state = state.clone();
    div()
        .relative()
        .w(px(width))
        .min_w(px(width.min(160.0)))
        .flex_shrink_1()
        .occlude()
        .on_action(|_: &Escape, window, cx| {
            window.blur(cx);
        })
        .child(Input::new(&state).cleanable(true).prefix(
            div().pl(px(2.0)).flex().items_center().child(icon(
                IconName::Search,
                14.0,
                palette().fg_muted,
            )),
        ))
        .child(
            canvas(
                |bounds, _, _| bounds,
                move |bounds, _, window, cx| {
                    let focus = focus_state.read(cx).focus_handle(cx);
                    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                        if phase != DispatchPhase::Capture {
                            return;
                        }
                        if !matches!(event.button, MouseButton::Left | MouseButton::Right) {
                            return;
                        }
                        if !focus.is_focused(window) || bounds.contains(&event.position) {
                            return;
                        }
                        window.blur(cx);
                    });
                },
            )
            .absolute()
            .size_full(),
        )
}

/// Plain count chip. Marketplace passes translated "N skills" text here,
/// without the Layers glyph `bar_count` paints for projects and decks.
pub(crate) fn bar_text_chip(label: impl Into<SharedString>) -> Div {
    div()
        .h(px(32.0))
        .px_3()
        .flex()
        .items_center()
        .flex_shrink_0()
        .rounded_lg()
        .border_1()
        .border_color(rgb(palette().border))
        .bg(rgb(palette().bg))
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(palette().fg))
        .child(label.into())
}

/// h-8 segmented track. Sort, source tabs, and the view toggle share it.
/// Active fill is `bg-accent` (`accent_soft`), ink is `text-accent-foreground`.
pub(crate) fn segment_track() -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(2.0))
        .h(px(32.0))
        .p(px(2.0))
        .flex_shrink_0()
        .rounded_lg()
        .border_1()
        .border_color(rgb(palette().border))
        .bg(rgb(palette().well))
        .occlude()
}

pub(crate) fn segment_tab(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    active: bool,
) -> MotionDiv {
    segment_tab_with(id, label, active, px(12.0))
}

/// Same tab, with less horizontal padding. Skills filters use this so the
/// source and agent tracks stay beside search instead of painting over it.
pub(crate) fn segment_tab_compact(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    active: bool,
) -> MotionDiv {
    segment_tab_with(id, label, active, px(6.0))
}

fn segment_tab_with(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    active: bool,
    pad: Pixels,
) -> MotionDiv {
    let id = id.into();
    let key: SharedString = id.to_string().into();
    let rest_fg = rgb(if active {
        palette().accent_fg
    } else {
        palette().fg_muted
    });
    let hover_fg = rgb(if active {
        palette().accent_fg
    } else {
        palette().fg
    });
    let mut rest = MotionPaint::new().fg(rest_fg);
    let mut hover = MotionPaint::new().fg(hover_fg);
    if active {
        let fill = rgb(palette().accent_soft);
        rest = rest.bg(fill);
        hover = hover.bg(fill);
    } else {
        hover = hover.bg(rgb(palette().panel_hover));
    }
    div()
        .id(id)
        .h_full()
        .px(pad)
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .text_xs()
        .font_weight(if active {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::MEDIUM
        })
        .text_color(rest_fg)
        .when(active, |d| d.bg(rgb(palette().accent_soft)))
        .child(label.into())
        .interaction_spring(key, true, rest, hover)
}

/// Grid / list switch. `list` selects the list glyph. Callers attach clicks.
/// The glyph is the only label, so the tooltip names the mode.
pub(crate) fn view_toggle_button(id: &'static str, glyph: IconName, active: bool) -> MotionDiv {
    let tip_key = if glyph == IconName::List {
        "toolbar.viewList"
    } else {
        "toolbar.viewGrid"
    };
    let mut rest = MotionPaint::new();
    let mut hover = MotionPaint::new();
    if active {
        let fill = rgb(palette().accent_soft);
        rest = rest.bg(fill);
        hover = hover.bg(fill);
    } else {
        hover = hover.bg(rgb(palette().panel_hover));
    }
    div()
        .id(id)
        .w(px(32.0))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .when(active, |d| d.bg(rgb(palette().accent_soft)))
        .child(icon(
            glyph,
            14.0,
            if active {
                palette().accent_fg
            } else {
                palette().fg_muted
            },
        ))
        .tooltip(move |window, cx| super::tooltip(crate::i18n::t(tip_key)).build(window, cx))
        .interaction_spring(id, true, rest, hover)
}

pub(crate) fn bar_secondary(
    id: &'static str,
    glyph: IconName,
    label: impl Into<SharedString>,
) -> MotionDiv {
    let rest_bg = rgb(palette().bg);
    let rest_fg = rgb(palette().fg_muted);
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(6.0))
        .h(px(32.0))
        .px_3()
        .flex_shrink_0()
        .rounded_lg()
        .border_1()
        .border_color(rgb(palette().border))
        .bg(rest_bg)
        .cursor_pointer()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(rest_fg)
        .child(icon(glyph, 14.0, palette().fg_muted))
        .child(label.into())
        .occlude()
        .interaction_spring(
            id,
            true,
            MotionPaint::new().bg(rest_bg).fg(rest_fg),
            MotionPaint::new()
                .bg(rgb(palette().card_hover))
                .fg(rgb(palette().fg)),
        )
}
