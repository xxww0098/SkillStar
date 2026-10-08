//! Page chrome shared by every capability: the toolbar row, icon helpers,
//! the floating back-to-top control, kit-dialog framing, and pointer paint.
//! No page state lives here.

mod dialog;
mod motion;
mod scroll_top;
mod segmented;
mod sheet;
mod toolbar;

use std::time::Duration;

use gpui_kit::base::StyledExt;
use gpui_kit::component::Icon;
use gpui_kit::component::Sizable;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::*;

use crate::theme::palette;

pub(crate) use dialog::{DialogChrome, open_centered, open_confirm, open_form_dialog};
pub(crate) use motion::{InteractionSpring, MotionDiv, MotionPaint, motion_spring};
pub(crate) use scroll_top::{back_to_top_button, is_list_scrolled_down};
pub(crate) use segmented::{SliderGeometry, SliderSegment, slider_segmented, slider_segmented_at};
pub(crate) use sheet::{SHEET_GAP, sheet};
pub(crate) use toolbar::{
    PageBar, bar_count, bar_icon_button, bar_primary, bar_refresh_button, bar_secondary,
    bar_text_chip, page_toolbar, segment_tab_compact, segment_track, toolbar_search, window_drag,
};

/// Hover tip. Black surface, white text, in both modes.
///
/// The kit paints `tokens.popover` and then applies this style, so the black
/// stays on the tip. Menus, notifications, and search keep the card color.
/// `Button::tooltip` builds its own unstyled tip; hang this helper on the
/// element instead.
pub(crate) fn tooltip(text: impl Into<SharedString>) -> gpui_kit::component::tooltip::Tooltip {
    let p = palette();
    gpui_kit::component::tooltip::Tooltip::new(text.into())
        .bg(rgb(p.tip_bg))
        .text_color(rgb(p.tip_fg))
        .border_color(rgb(p.tip_fg).alpha(0.16))
}

/// Wraps a page body in the standard shell chrome — toolbar on top, body
/// below. Pages stay transparent: the shell panel owns the background, so
/// a square page fill cannot bleed past the panel's rounded corners.
pub(crate) fn page_chrome(toolbar: impl IntoElement, body: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .size_full()
        .child(toolbar)
        .child(body)
}

/// Helper to construct a sized, colored icon from Lucide `IconName`.
pub(crate) fn icon(
    name: gpui_kit::assets::IconName,
    size: f32,
    color: u32,
) -> gpui_kit::component::Icon {
    use gpui_kit::component::Sizable;
    gpui_kit::component::Icon::new(name)
        .with_size(px(size))
        .text_color(rgb(color))
}

/// Opacity pulses (skeletons, reset blink) read clearly well below 60.
/// They stay stepped: a throb does not need a sample on every refresh.
pub(crate) const PULSE_FPS: f32 = 20.0;

/// Same glyph as [`icon`]. While `spin` is set the glyph is a kit
/// [`Spinner`] and turns on the kit cadence; it stops on the next frame
/// after the flag drops. Rotation stays a paint transform, so the heavy
/// page behind it can stay a replayed view.
pub(crate) fn icon_spin(
    name: gpui_kit::assets::IconName,
    size: f32,
    color: u32,
    spin: bool,
) -> AnyElement {
    if !spin {
        return icon(name, size, color).into_any_element();
    }
    Spinner::new()
        .icon(Icon::new(name))
        .with_size(px(size))
        .color(rgb(color).into())
        .into_any_element()
}

/// Beam width for [`icon_sweep`]: a fading tail plus [`SWEEP_CORE_W`] pixels
/// of bright core, crossing the glyph in one pass.
const SWEEP_BEAM_W: f32 = 10.0;

/// The bright leading edge of a sweep beam. The tail fades out behind it.
const SWEEP_CORE_W: f32 = 2.0;

/// Left offset of a sweep beam: fully left of the glyph at 0, fully past its
/// right edge at 1, so one pass crosses the whole glyph. The animator applies
/// it as a style offset, so a repeating pass also settles out of sight under
/// `reduce_motion` instead of freezing mid-glyph.
fn sweep_left(delta: f32, size: f32) -> Pixels {
    px((size + SWEEP_BEAM_W) * delta.clamp(0.0, 1.0) - SWEEP_BEAM_W)
}

/// Same glyph as [`icon`]. While `sweep` is set, a laser beam crosses the
/// glyph left to right about once per second, sampled on the display link,
/// and stops on the next frame after the flag drops. The glyph dims while a
/// pass runs, so the beam reads as the moving part. Like [`icon_spin`] a
/// stable `anim_id` keeps the phase across redraws, and the pass is a style
/// offset on a clipped one-glyph frame, so those frames do not lay the page
/// out again either.
pub(crate) fn icon_sweep(
    anim_id: impl Into<ElementId>,
    name: gpui_kit::assets::IconName,
    size: f32,
    color: u32,
    sweep: bool,
) -> AnyElement {
    let glyph = icon(name, size, color);
    if !sweep {
        return glyph.into_any_element();
    }
    let laser = rgb(palette().accent);
    div()
        .relative()
        .w(px(size))
        .h(px(size))
        .overflow_hidden()
        .child(div().size_full().opacity(0.35).child(glyph))
        .child(
            div()
                .absolute()
                .top_0()
                .w(px(SWEEP_BEAM_W))
                .h_full()
                .flex()
                .flex_row()
                .child(
                    div()
                        .w(px(SWEEP_BEAM_W - SWEEP_CORE_W))
                        .h_full()
                        .bg(linear_gradient(
                            90.,
                            linear_color_stop(laser.alpha(0.0), 0.),
                            linear_color_stop(laser.alpha(0.65), 1.),
                        )),
                )
                .child(div().w(px(SWEEP_CORE_W)).h_full().bg(laser.alpha(0.95)))
                .with_animation(
                    anim_id,
                    Animation::new(Duration::from_secs(1)).repeat(),
                    move |beam, delta| beam.left(sweep_left(delta, size)),
                ),
        )
        .into_any_element()
}

/// Replay a retained view until that view itself is notified.
///
/// The parent has to be `relative`. The view is absolutely positioned and
/// sized by this style, not by its contents. GPUI copies the previous
/// scene for a cache hit, which is what lets a 120Hz animation move a
/// sibling without laying this subtree out again.
pub(crate) fn replay_view(view: AnyView) -> impl IntoElement {
    view.cached(StyleRefinement::default().absolute().size_full())
}

/// Loading bar. The pulse is synced across bars and sampled at [`PULSE_FPS`],
/// not once per display refresh. `id` must stay stable across frames.
pub(crate) struct Pulse {
    id: ElementId,
    secondary: bool,
    style: StyleRefinement,
}

pub(crate) fn pulse(id: impl Into<SharedString>) -> Pulse {
    Pulse {
        id: ElementId::Name(id.into()),
        secondary: false,
        style: StyleRefinement::default(),
    }
}

impl Pulse {
    pub(crate) fn secondary(mut self) -> Self {
        self.secondary = true;
        self
    }
}

impl Styled for Pulse {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl IntoElement for Pulse {
    type Element = AnyElement;

    fn into_element(self) -> Self::Element {
        let mut fill = rgb(palette().well);
        if self.secondary {
            fill = fill.alpha(0.5);
        }
        let bar = div().w_full().h_4().bg(fill).refine_style(&self.style);
        bar.with_animation(
            self.id,
            Animation::new(Duration::from_secs(2))
                .repeat_synced()
                .with_max_fps(PULSE_FPS)
                .with_easing(bounce(ease_in_out)),
            |bar, delta| bar.opacity(1.0 - delta * 0.5),
        )
        .into_any_element()
    }
}

/// `Button variant="ghost" size="sm"` — label-only row action. Generic over
/// the owning view so dialogs outside `import_modal` can reuse the pair.
pub(crate) fn ghost_button<T: 'static>(
    id: &'static str,
    label: SharedString,
    view: gpui::WeakEntity<T>,
    action: impl Fn(&mut T, &mut Window, &mut gpui::Context<T>) + 'static,
) -> MotionDiv {
    div()
        .id(id)
        .h(px(28.0))
        .px_3()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(palette().fg))
        .interaction_spring(
            id,
            true,
            MotionPaint::new(),
            MotionPaint::new().bg(rgb(palette().panel_hover)),
        )
        .child(label)
        .on_click(move |_, window, cx| {
            let _ = view.update(cx, |this, cx| action(this, window, cx));
        })
}

/// `Button size="sm"` primary fill — footer commits and Done.
pub(crate) fn primary_button<T: 'static>(
    id: &'static str,
    label: SharedString,
    view: gpui::WeakEntity<T>,
    action: impl Fn(&mut T, &mut Window, &mut gpui::Context<T>) + 'static,
) -> MotionDiv {
    div()
        .id(id)
        .h(px(28.0))
        .px(px(20.0))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .rounded_md()
        .bg(rgb(palette().accent))
        .cursor_pointer()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(palette().on_accent))
        .child(label)
        .on_click(move |_, window, cx| {
            let _ = view.update(cx, |this, cx| action(this, window, cx));
        })
        .interaction_spring(
            id,
            true,
            MotionPaint::new().bg(rgb(palette().accent)),
            MotionPaint::new().bg(rgb(palette().accent_hover)),
        )
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Pixels, Styled as _, px};

    #[test]
    fn tooltip_surface_is_black_with_white_text() {
        let mut tip = super::tooltip("name");
        let style = tip.style();
        assert_eq!(style.background, Some(gpui_kit::rgb(0x000000).into()));
        assert_eq!(style.text.color, Some(gpui_kit::rgb(0xffffff).into()));
    }

    /// One pass has to start left of the glyph and end past its right edge,
    /// whatever the animation feeds the closure.
    #[test]
    fn a_sweep_pass_covers_the_whole_glyph() {
        let size = 14.0;
        assert_eq!(super::sweep_left(0.0, size), px(-super::SWEEP_BEAM_W));
        assert_eq!(super::sweep_left(1.0, size), px(size));
        assert_eq!(super::sweep_left(-0.3, size), px(-super::SWEEP_BEAM_W));
        assert_eq!(super::sweep_left(1.7, size), px(size));
        let _mid: Pixels = super::sweep_left(0.5, size);
    }
}
