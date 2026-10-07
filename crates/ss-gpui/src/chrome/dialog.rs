//! Kit dialogs shared by every page.
//!
//! gpui-component 0.7.1 keeps Dialog::button_props as callbacks only.
//! A regular Dialog never paints them, so a confirm opened that way has
//! a title, a body, and a close button, and no cancel or commit button.
//! It also pins the card's top edge at one tenth of the viewport.
//!
//! AlertDialog would paint the footer, but it hides the close button and
//! does not expose margin_top, so it cannot be placed in the window
//! center. These helpers draw the footer on a normal Dialog (danger for
//! an irreversible commit, primary otherwise) and set margin_top from
//! the card's measured height. The builder runs before layout, so the
//! first frame uses the fallback and the next frame uses the measured height.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::component::WindowExt;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::dialog::{Dialog, DialogFooter, DialogTitle};
use gpui_kit::component::v_flex;
use gpui_kit::*;

use crate::theme::palette;

/// Minimum distance from the window edge. Matches the kit's overflow clamp
/// so a centered card that fits is not pulled back toward the top.
const EDGE_MARGIN: f32 = 24.0;

/// Default dialog padding (16 + 16) plus the 1px border on each edge.
/// The measured child sits inside that padding.
const PADDED_CHROME: f32 = 34.0;

/// p_0 dialogs (the import modal) only have the border outside the child.
const FLUSH_CHROME: f32 = 2.0;

/// How much of the kit surface sits outside the measured child.
#[derive(Clone, Copy)]
pub(crate) enum DialogChrome {
    Padded,
    Flush,
}

impl DialogChrome {
    fn extra(self) -> f32 {
        match self {
            Self::Padded => PADDED_CHROME,
            Self::Flush => FLUSH_CHROME,
        }
    }
}

/// Screen-space top of a card, then the margin_top the kit expects.
///
/// The kit adds window_paddings.top to margin_top before placing the
/// card, so the inset has to be removed here or the card sits low.
pub(crate) fn centered_margin_top(viewport: f32, card: f32, top_inset: f32) -> f32 {
    let screen_top = ((viewport - card) * 0.5).max(EDGE_MARGIN);
    (screen_top - top_inset).max(EDGE_MARGIN)
}

/// Remembers the last laid-out body height. open_dialog's builder runs
/// on every dialog-layer paint, before layout, so this is what the next
/// paint reads.
#[derive(Clone)]
pub(crate) struct DialogFrame {
    height: Rc<Cell<Option<f32>>>,
    fallback: f32,
    chrome: DialogChrome,
}

impl DialogFrame {
    fn new(fallback: f32, chrome: DialogChrome) -> Self {
        Self {
            height: Rc::new(Cell::new(None)),
            fallback,
            chrome,
        }
    }

    fn margin_top(&self, window: &Window) -> Pixels {
        let viewport = window.viewport_size().height.as_f32();
        let card = self.height.get().unwrap_or(self.fallback) + self.chrome.extra();
        let inset = gpui_kit::component::window_paddings(window).top.as_f32();
        px(centered_margin_top(viewport, card, inset))
    }

    /// Known child height for the first frame. Layout still replaces it when
    /// the measured child differs by more than a pixel.
    pub(crate) fn seed(&self, height: f32) {
        if self.height.get().is_none() && height >= 8.0 {
            self.height.set(Some(height));
        }
    }

    /// Wrap the dialog body. The child's laid-out height is the next
    /// frame's centering input. Heights under 8px are ignored so a
    /// not-yet-laid-out child cannot yank the card to the middle.
    pub(crate) fn measure(&self, child: impl IntoElement) -> Div {
        let slot = self.height.clone();
        div()
            .w_full()
            .child(child)
            .on_children_prepainted(move |bounds, window, _cx| {
                let Some(bounds) = bounds.first() else {
                    return;
                };
                let next = bounds.size.height.as_f32();
                if next < 8.0 {
                    return;
                }
                let prev = slot.get();
                if prev.is_none_or(|prev| (prev - next).abs() > 1.0) {
                    slot.set(Some(next));
                    window.request_animation_frame();
                }
            })
    }
}

/// Open a kit dialog centered in the window.
///
/// The builder must pass its visible body through DialogFrame::measure.
/// Flush is for a dialog that already sets p_0 (the import modal).
pub(crate) fn open_centered<F>(
    window: &mut Window,
    cx: &mut App,
    fallback_height: f32,
    chrome: DialogChrome,
    build: F,
) where
    F: Fn(Dialog, &DialogFrame, &mut Window, &mut App) -> Dialog + 'static,
{
    let frame = DialogFrame::new(fallback_height, chrome);
    window.open_dialog(cx, move |dialog, window, cx| {
        let frame = frame.clone();
        let built = build(dialog, &frame, window, cx);
        built.margin_top(frame.margin_top(window))
    });
}

/// Centered confirm with a cancel button and a commit button.
///
/// danger is for an irreversible commit (uninstall, delete, remove).
/// Other commits use primary. Enter runs the same callback as the button;
/// returning false leaves the dialog open.
pub(crate) fn open_confirm<Ok>(
    window: &mut Window,
    cx: &mut App,
    title: impl Into<SharedString>,
    body: impl Into<SharedString>,
    ok_text: impl Into<SharedString>,
    danger: bool,
    on_ok: Ok,
) where
    Ok: Fn(&mut Window, &mut App) -> bool + 'static,
{
    let body = body.into();
    open_form_dialog(
        window,
        cx,
        title,
        ok_text,
        danger,
        140.0,
        move |_, _| muted_copy(body.clone()).into_any_element(),
        on_ok,
    );
}

/// Centered short form: title, caller-built fields, cancel, and commit.
///
/// body is called on every dialog paint. Create input entities before
/// calling this, and only clone them inside body.
pub(crate) fn open_form_dialog<B, Ok>(
    window: &mut Window,
    cx: &mut App,
    title: impl Into<SharedString>,
    ok_text: impl Into<SharedString>,
    danger: bool,
    fallback_height: f32,
    body: B,
    on_ok: Ok,
) where
    B: Fn(&mut Window, &mut App) -> AnyElement + 'static,
    Ok: Fn(&mut Window, &mut App) -> bool + 'static,
{
    let title = title.into();
    let ok_text = ok_text.into();
    let on_ok: Rc<dyn Fn(&mut Window, &mut App) -> bool> = Rc::new(on_ok);
    open_centered(
        window,
        cx,
        fallback_height,
        DialogChrome::Padded,
        move |dialog, frame, window, cx| {
            let on_ok = on_ok.clone();
            dialog
                .on_ok({
                    let on_ok = on_ok.clone();
                    move |_, window, cx| on_ok(window, cx)
                })
                .child(frame.measure(confirm_card(
                    title.clone(),
                    body(window, cx),
                    ok_text.clone(),
                    danger,
                    on_ok,
                )))
        },
    );
}

fn muted_copy(text: SharedString) -> Div {
    div()
        .w_full()
        .text_sm()
        .text_color(rgb(palette().fg_muted))
        .child(text)
}

fn confirm_card(
    title: SharedString,
    body: AnyElement,
    ok_text: SharedString,
    danger: bool,
    on_ok: Rc<dyn Fn(&mut Window, &mut App) -> bool>,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap_3()
        .child(DialogTitle::new().pr_6().child(title))
        .child(body)
        .child(action_row(ok_text, danger, on_ok))
}

fn action_row(
    ok_text: SharedString,
    danger: bool,
    on_ok: Rc<dyn Fn(&mut Window, &mut App) -> bool>,
) -> DialogFooter {
    let commit = Button::new("dialog-ok").label(ok_text);
    let commit = if danger {
        commit.danger()
    } else {
        commit.primary()
    };
    DialogFooter::new()
        .w_full()
        .child(
            Button::new("dialog-cancel")
                .label(crate::i18n::t("common.cancel"))
                .on_click(|_, window, cx| {
                    window.close_dialog(cx);
                }),
        )
        .child(commit.on_click(move |_, window, cx| {
            if on_ok(window, cx) {
                window.close_dialog(cx);
            }
        }))
}

#[cfg(test)]
mod tests {
    use super::centered_margin_top;

    #[test]
    fn centers_a_short_card_in_the_viewport() {
        assert_eq!(centered_margin_top(800.0, 200.0, 0.0), 300.0);
    }

    #[test]
    fn removes_the_kit_top_inset_so_the_card_stays_centered() {
        assert_eq!(centered_margin_top(800.0, 200.0, 28.0), 272.0);
    }

    #[test]
    fn keeps_a_tall_card_inside_the_margin() {
        assert_eq!(centered_margin_top(200.0, 400.0, 0.0), 24.0);
        assert_eq!(centered_margin_top(80.0, 40.0, 40.0), 24.0);
    }
}
