//! Skills-mode destinations in the sidebar.
//!
//! One selection box is painted from the rows' measured bounds and springs
//! between them. The rows stay transparent, so the box can travel without
//! each item painting its own highlight. `reduce_motion` snaps the spring.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::Shell;
use crate::chrome::{InteractionSpring, MotionPaint, icon, motion_spring};
use crate::nav::NavPage;
use crate::theme::{self, palette};

/// `rounded_md` is `0.375rem`. The painted box uses the same radius.
const NAV_RADIUS: Rems = rems(0.375);
/// Same gap the row column lays out with, so the box travels through it.
const NAV_GAP_PX: f32 = 2.0;

pub(super) fn render_skills_nav(
    active: NavPage,
    collapsed: bool,
    nav_slot: u8,
    shell: WeakEntity<Shell>,
) -> impl IntoElement {
    let shown = skills_nav_slot(active).is_some();
    let target = f32::from(nav_slot);
    let presence = if shown { 1.0 } else { 0.0 };
    div().w_full().with_spring(
        ElementId::Name("skills-nav-presence".into()),
        motion_spring(presence),
        move |host, opacity| {
            let shell = shell.clone();
            host.child(div().w_full().with_spring(
                ElementId::Name("skills-nav-slot".into()),
                motion_spring(target),
                move |_, slot| skills_nav_column(active, collapsed, slot, opacity, shell),
            ))
        },
    )
}

/// Index of a skills-nav page. Settings and publisher detail are absent.
pub(super) fn skills_nav_slot(page: NavPage) -> Option<u8> {
    let index = NavPage::SKILLS_NAV.iter().position(|item| *item == page)?;
    u8::try_from(index).ok()
}

/// How strongly row `index` should wear the selected color.
/// `slot` is the spring position; `opacity` fades the whole selection out.
pub(super) fn nav_emphasis(index: usize, slot: f32, opacity: f32) -> f32 {
    let near = (1.0 - (slot - index as f32).abs()).clamp(0.0, 1.0);
    near * opacity.clamp(0.0, 1.0)
}

/// Box covering the row at `slot`, blending neighbors while the spring travels.
pub(super) fn nav_thumb_bounds(slots: &[Bounds<Pixels>], slot: f32) -> Option<Bounds<Pixels>> {
    let last = slots.len().checked_sub(1)?;
    let slot = slot.clamp(0.0, last as f32);
    let start = slot.floor() as usize;
    let end = slot.ceil() as usize;
    let mix = slot - start as f32;
    Some(lerp_bounds(slots[start], slots[end], mix))
}

fn skills_nav_column(
    active: NavPage,
    collapsed: bool,
    slot: f32,
    opacity: f32,
    shell: WeakEntity<Shell>,
) -> Div {
    // Filled during the column's prepaint, read when the canvas behind the
    // rows paints. Prepaint of the whole tree finishes before any paint, so
    // this frame's row bounds are already stored.
    let measured = Rc::new(RefCell::new(Vec::new()));
    let measured_paint = measured.clone();
    div()
        .relative()
        .w_full()
        .child(
            canvas(
                |_, _, _| (),
                move |_, _, window, _| {
                    let slots = measured_paint.borrow();
                    let Some(frame) = nav_thumb_bounds(&slots, slot) else {
                        return;
                    };
                    paint_nav_thumb(frame, opacity, window);
                },
            )
            .absolute()
            .inset_0(),
        )
        .child(skills_nav_rows(
            active, collapsed, slot, opacity, shell, measured,
        ))
}

fn skills_nav_rows(
    active: NavPage,
    collapsed: bool,
    slot: f32,
    opacity: f32,
    shell: WeakEntity<Shell>,
    measured: Rc<RefCell<Vec<Bounds<Pixels>>>>,
) -> Div {
    let mut column = div().flex().flex_col().w_full().gap(px(NAV_GAP_PX));
    for (index, page) in NavPage::SKILLS_NAV.iter().copied().enumerate() {
        column = column.child(skills_nav_row(
            page,
            page == active,
            collapsed,
            nav_emphasis(index, slot, opacity),
            shell.clone(),
        ));
    }
    column.on_children_prepainted(move |bounds, _, _| {
        *measured.borrow_mut() = bounds;
    })
}

fn skills_nav_row(
    page: NavPage,
    selected: bool,
    collapsed: bool,
    emphasis: f32,
    shell: WeakEntity<Shell>,
) -> impl IntoElement {
    let color = theme::mix_hsl(palette().fg_muted, palette().accent_fg, emphasis);
    let rest_fg = rgb(color);
    // The traveling box owns the selected fill. A hover fill on the selected
    // row would cover that box.
    let rest = MotionPaint::new().fg(rest_fg);
    let mut hover = MotionPaint::new().fg(rgb(if selected {
        palette().accent_fg
    } else {
        palette().fg
    }));
    if !selected {
        hover = hover.bg(rgb(palette().panel_hover));
    }
    div()
        .id(ElementId::Name(page.dom_id().into()))
        .w_full()
        .flex()
        .items_center()
        .rounded_md()
        .cursor_pointer()
        .text_sm()
        .text_color(rest_fg)
        .child(icon(
            page.icon(),
            if collapsed { 18.0 } else { 16.0 },
            color,
        ))
        .when(collapsed, |row| row.justify_center().px_0().py_2())
        .when(!collapsed, |row| {
            row.gap(px(10.0))
                .px(px(10.0))
                .py(px(7.0))
                .child(page.label())
        })
        .when(selected, |row| row.font_weight(FontWeight::SEMIBOLD))
        .interaction_spring(page.dom_id(), true, rest, hover)
        .on_click(move |_, _, cx| {
            let _ = shell.update(cx, |this, cx| this.set_page(page, cx));
        })
}

fn paint_nav_thumb(bounds: Bounds<Pixels>, opacity: f32, window: &mut Window) {
    if opacity <= 0.0 {
        return;
    }
    let radius = NAV_RADIUS.to_pixels(window.rem_size());
    let fill = rgb(palette().panel_active).opacity(opacity);
    let edge = rgb(palette().accent).opacity(opacity);
    window.paint_quad(quad(
        bounds,
        Corners::all(radius),
        fill,
        Edges::all(px(1.0)),
        edge,
        BorderStyle::Solid,
    ));
}

fn lerp_bounds(from: Bounds<Pixels>, to: Bounds<Pixels>, t: f32) -> Bounds<Pixels> {
    Bounds {
        origin: point(
            lerp_px(from.origin.x, to.origin.x, t),
            lerp_px(from.origin.y, to.origin.y, t),
        ),
        size: size(
            lerp_px(from.size.width, to.size.width, t),
            lerp_px(from.size.height, to.size.height, t),
        ),
    }
}

fn lerp_px(from: Pixels, to: Pixels, t: f32) -> Pixels {
    from + (to - from) * t
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Bounds, Pixels, point, px, size};

    use super::{nav_emphasis, nav_thumb_bounds, skills_nav_slot};
    use crate::nav::NavPage;

    fn box_at(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
        Bounds {
            origin: point(px(x), px(y)),
            size: size(px(width), px(height)),
        }
    }

    fn near(value: Pixels, expected: f32) -> bool {
        (value.as_f32() - expected).abs() < 0.001
    }

    #[test]
    fn skills_nav_slot_matches_the_four_destinations() {
        assert_eq!(skills_nav_slot(NavPage::MySkills), Some(0));
        assert_eq!(skills_nav_slot(NavPage::Marketplace), Some(1));
        assert_eq!(skills_nav_slot(NavPage::SkillCards), Some(2));
        assert_eq!(skills_nav_slot(NavPage::Projects), Some(3));
        assert_eq!(skills_nav_slot(NavPage::Settings), None);
        assert_eq!(skills_nav_slot(NavPage::PublisherDetail), None);
    }

    #[test]
    fn emphasis_follows_the_spring_and_fades_with_the_box() {
        assert!((nav_emphasis(0, 0.0, 1.0) - 1.0).abs() < 1e-5);
        assert!((nav_emphasis(1, 0.0, 1.0) - 0.0).abs() < 1e-5);
        assert!((nav_emphasis(0, 0.25, 1.0) - 0.75).abs() < 1e-5);
        assert!((nav_emphasis(1, 0.25, 1.0) - 0.25).abs() < 1e-5);
        assert!((nav_emphasis(0, 0.5, 1.0) - 0.5).abs() < 1e-5);
        assert!((nav_emphasis(1, 0.5, 1.0) - 0.5).abs() < 1e-5);
        assert!((nav_emphasis(0, 0.0, 0.5) - 0.5).abs() < 1e-5);
        assert!((nav_emphasis(2, 5.0, 1.0) - 0.0).abs() < 1e-5);
    }

    #[test]
    fn thumb_bounds_blend_between_measured_rows() {
        let slots = [
            box_at(8.0, 10.0, 100.0, 34.0),
            box_at(8.0, 46.0, 100.0, 40.0),
        ];
        let start = nav_thumb_bounds(&slots, 0.0).unwrap();
        assert!(near(start.origin.y, 10.0));
        assert!(near(start.size.height, 34.0));

        let mid = nav_thumb_bounds(&slots, 0.5).unwrap();
        assert!(near(mid.origin.y, 28.0));
        assert!(near(mid.size.height, 37.0));
        assert!(near(mid.origin.x, 8.0));
        assert!(near(mid.size.width, 100.0));

        let end = nav_thumb_bounds(&slots, 1.0).unwrap();
        assert!(near(end.origin.y, 46.0));
        assert!(near(end.size.height, 40.0));

        let clamped = nav_thumb_bounds(&slots, 4.0).unwrap();
        assert!(near(clamped.origin.y, 46.0));
        assert!(nav_thumb_bounds(&[], 0.0).is_none());
    }
}
