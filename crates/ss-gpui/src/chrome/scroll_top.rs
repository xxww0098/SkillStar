//! Floating back-to-top control for long scrollable pages. React source:
//! `Marketplace.tsx` / `PublisherDetail.tsx` `showBackToTop`. No page state
//! lives here — the caller reads its own scroll handle every frame and only
//! mounts the button while its pane is scrolled down.

use gpui_kit::assets::IconName;
use gpui_kit::*;

use super::{InteractionSpring, MotionDiv, MotionPaint};
use crate::theme::palette;

/// React parity (`scrollTop > 300` in `Marketplace.tsx`): appear only after
/// this much scroll, so the button never flickers while browsing the top.
const SHOW_AFTER_PX: f32 = 300.0;

/// True when a `.track_scroll`-ed pane is scrolled far enough to offer the
/// shortcut. A pane that cannot scroll keeps its offset clamped at zero, so
/// the offset check alone also hides the button for short content.
pub(crate) fn is_scrolled_down(handle: &ScrollHandle) -> bool {
    handle.offset().y < px(-SHOW_AFTER_PX)
}

/// Same check for a `uniform_list` handle; the offset lives on its base.
pub(crate) fn is_list_scrolled_down(handle: &UniformListScrollHandle) -> bool {
    is_scrolled_down(&handle.0.borrow().base_handle)
}

/// The floating control. Mount it inside a `.relative()` page body; it pins
/// itself to the bottom-right corner (`bottom-8 right-8` in React).
pub(crate) fn back_to_top_button(
    id: &'static str,
    tooltip: SharedString,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> MotionDiv {
    let rest_bg = rgb(palette().card);
    let rest_border = rgb(palette().border);
    div()
        .id(id)
        .absolute()
        .bottom(px(32.0))
        .right(px(32.0))
        .size(px(40.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(rest_border)
        .bg(rest_bg)
        .shadow_md()
        .cursor_pointer()
        // Sits on the card grid. Block the card's hover; the wheel still scrolls.
        .block_mouse_except_scroll()
        .tooltip(move |window, cx| super::tooltip(tooltip.clone()).build(window, cx))
        .child(crate::chrome::icon(
            IconName::ArrowUp,
            16.0,
            palette().fg_muted,
        ))
        .on_click(on_click)
        .interaction_spring(
            id,
            true,
            MotionPaint::new().bg(rest_bg).border(rest_border),
            MotionPaint::new()
                .bg(rgb(palette().card_hover))
                .border(rgb(palette().accent)),
        )
}

#[cfg(test)]
mod tests {
    // Explicit imports: the crate-level `use gpui_kit::*` glob also carries
    // GPUI's own `test` macro, which would shadow Rust's `#[test]` here.
    use super::{SHOW_AFTER_PX, is_list_scrolled_down, is_scrolled_down};
    use gpui_kit::{ScrollHandle, UniformListScrollHandle, point, px};

    #[test]
    fn appears_only_past_the_threshold() {
        let handle = ScrollHandle::new();
        assert!(!is_scrolled_down(&handle));

        // Exactly at the threshold is still "near the top" (`scrollTop > 300`).
        handle.set_offset(point(px(0.0), px(-SHOW_AFTER_PX)));
        assert!(!is_scrolled_down(&handle));

        handle.set_offset(point(px(0.0), px(-SHOW_AFTER_PX - 1.0)));
        assert!(is_scrolled_down(&handle));
    }

    #[test]
    fn reads_the_uniform_list_base_handle() {
        let handle = UniformListScrollHandle::new();
        assert!(!is_list_scrolled_down(&handle));
        handle
            .0
            .borrow_mut()
            .base_handle
            .set_offset(point(px(0.0), px(-400.0)));
        assert!(is_list_scrolled_down(&handle));
    }
}
