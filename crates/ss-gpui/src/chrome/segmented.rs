//! Sliding-thumb segmented control.
//!
//! One absolutely positioned thumb springs between equal slots; the segments
//! stay transparent and crossfade their ink on the same spring, so the
//! highlight glides instead of each segment repainting its own fill. The
//! sidebar mode switcher, the skills scope switch, and the settings
//! translation-language row share this code. `reduce_motion` snaps the slide
//! (`with_spring` handles that).

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::chrome::{InteractionSpring, MotionPaint, icon, motion_spring};
use crate::theme::{self, palette};

const GAP_PX: f32 = 2.0;

/// Click sink shared by every pill: the slot index plus the gpui context.
pub(crate) type SegmentClick = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// One slot of the track. `id` keys the clickable pill; icon and label are
/// each optional (icon-only scopes omit the label, the text-only language
/// row omits the icon).
#[derive(Clone)]
pub(crate) struct SliderSegment {
    pub(crate) id: &'static str,
    pub(crate) icon: Option<IconName>,
    pub(crate) label: Option<SharedString>,
}

/// Track padding per side. The thumb inset mirrors `pad` so the thumb and the
/// outline stay concentric (`rounded_md` inside `rounded_lg`).
#[derive(Clone, Copy)]
pub(crate) struct SliderGeometry {
    /// Slot width, excluding the gap.
    pub(crate) slot_w: f32,
    /// Slot height, excluding the gap.
    pub(crate) slot_h: f32,
    /// Track inner padding per side; the border adds 1px outside it.
    pub(crate) pad: f32,
    /// Segment icon size; the sidebar's collapsed form runs larger.
    pub(crate) icon_size: f32,
}

/// Equal-slot segmented control whose thumb springs to `selected`.
pub(crate) fn slider_segmented(
    motion_id: &'static str,
    segments: &[SliderSegment],
    selected: usize,
    geo: SliderGeometry,
    vertical: bool,
    on_click: SegmentClick,
) -> AnyElement {
    let target = selected as f32;
    let segments = segments.to_vec();
    div()
        .with_spring(
            ElementId::Name(motion_id.into()),
            motion_spring(target),
            move |_, t| slider_segmented_at(t, &segments, selected, geo, vertical, &on_click),
        )
        .into_any_element()
}

/// The track rendered at spring position `t`. The mode switcher calls this
/// inside its own spring closure, where the same `t` also animates its
/// collapsed-form geometry.
pub(crate) fn slider_segmented_at(
    t: f32,
    segments: &[SliderSegment],
    selected: usize,
    geo: SliderGeometry,
    vertical: bool,
    on_click: &SegmentClick,
) -> Div {
    let stride = if vertical {
        geo.slot_h + GAP_PX
    } else {
        geo.slot_w + GAP_PX
    };
    // Out-of-flow thumb. Children after it paint on top, so the pills keep
    // receiving clicks while the thumb slides underneath them.
    let mut thumb = div()
        .absolute()
        .w(px(geo.slot_w))
        .h(px(geo.slot_h))
        .rounded_md()
        .bg(rgb(palette().card))
        .border_1()
        .border_color(rgb(palette().border))
        .shadow_sm();
    thumb = if vertical {
        thumb.left(px(geo.pad)).top(px(geo.pad + t * stride))
    } else {
        thumb.top(px(geo.pad)).left(px(geo.pad + t * stride))
    };

    let mut track = div()
        .relative()
        .flex()
        .gap(px(GAP_PX))
        .p(px(geo.pad))
        .rounded_lg()
        .bg(rgb(palette().bg))
        .border_1()
        .border_color(rgb(palette().border))
        .when(vertical, |d| d.flex_col())
        // Keep presses off the page bar's window-drag layer, the same way
        // `segment_track` and the bar icon buttons occlude.
        .occlude()
        .child(thumb);
    for (ix, seg) in segments.iter().enumerate() {
        // Spring overshoot may push t past the slot range; the thumb enjoys
        // it, the color mix must not extrapolate.
        let near = (1.0 - (t - ix as f32).abs()).clamp(0.0, 1.0);
        let active = ix == selected;
        let pill = div()
            .id(ElementId::Name(seg.id.into()))
            .debug_selector(move || seg.id.into())
            .flex()
            .items_center()
            .justify_center()
            .w(px(geo.slot_w))
            .h(px(geo.slot_h))
            .rounded_md()
            .cursor_pointer()
            .text_color(rgb(theme::mix_hsl(palette().fg_muted, palette().fg, near)))
            .when_some(seg.icon, |d, icon_name| {
                d.child(icon(
                    icon_name,
                    geo.icon_size,
                    theme::mix_hsl(palette().fg_muted, palette().accent_fg, near),
                ))
            })
            .when_some(seg.label.clone(), |d, label| {
                d.gap(px(6.0)).child(div().text_xs().child(label))
            })
            .when(active, |d| d.font_weight(FontWeight::SEMIBOLD))
            .interaction_spring(
                seg.id,
                true,
                MotionPaint::new().opacity(1.0),
                MotionPaint::new().opacity(1.0),
            );
        let on_click = on_click.clone();
        track = track.child(pill.on_click(move |_, window, cx| {
            on_click(ix, window, cx);
        }));
    }
    track
}
