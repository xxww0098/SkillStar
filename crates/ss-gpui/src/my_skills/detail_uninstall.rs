//! Hover motion for the detail-column uninstall control.
//!
//! The other drawer buttons swap paint on the pointer edge. This one eases,
//! because a flat opacity dip does not read as a hover. The spring is the
//! shared critically damped one (about 80ms) and stops when it settles.
//! Leaving the button eases back from the current progress. Reduced motion
//! snaps. Do not copy this onto skill cards or the other buttons: a spring
//! redraws the canvas until it settles.

use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::*;

use super::MySkillsPage;
use crate::chrome::motion_spring;
use crate::theme::palette;

/// How far the trash glyph turns, grows, and rises at full hover.
const TURN: f32 = -0.2;
const GROW: f32 = 0.08;
const RISE: f32 = -1.0;

pub(super) fn button(
    skill_name: String,
    label: String,
    hovered: bool,
    view: WeakEntity<MySkillsPage>,
) -> impl IntoElement {
    let motion_id = SharedString::from(format!("drawer-uninstall-motion-{skill_name}"));
    let hover_view = view.clone();
    div()
        .id("drawer-uninstall-btn")
        .flex()
        .items_center()
        .justify_center()
        .gap_2()
        .w_full()
        .py_2()
        .px_2()
        .rounded_lg()
        .border_1()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .cursor_pointer()
        .active(|style| style.opacity(0.88))
        .on_hover(move |hovered, _, cx| {
            let _ = hover_view.update(cx, |this, cx| this.set_uninstall_hover(*hovered, cx));
        })
        .on_click(move |_, window, cx| {
            let name_dialog = skill_name.clone();
            let v_dialog = view.clone();
            crate::chrome::open_confirm(
                window,
                cx,
                crate::i18n::tf(
                    "uninstallDialog.titleNamed",
                    &[("name", name_dialog.as_str())],
                ),
                crate::i18n::t("uninstallDialog.description"),
                crate::i18n::t("uninstallDialog.confirmUninstall"),
                true,
                move |_, cx| {
                    let n = name_dialog.clone();
                    let _ = v_dialog.update(cx, |this, cx| {
                        this.select_detail(None);
                        this.uninstall_skill(&n, cx);
                    });
                    true
                },
            );
        })
        .with_spring(
            ElementId::Name(motion_id),
            motion_spring(if hovered { 1.0 } else { 0.0 }),
            move |button, progress| paint(button, progress, label),
        )
}

fn paint<E>(button: E, progress: f32, label: String) -> E
where
    E: Styled + ParentElement,
{
    let face = hover_face(progress);
    button
        .bg(face.bg)
        .border_color(face.border)
        .text_color(face.fg)
        .child(
            Icon::new(IconName::Trash)
                .with_size(px(14.0))
                .transform(glyph(face.progress)),
        )
        .child(div().whitespace_normal().text_center().child(label))
}

struct Face {
    progress: f32,
    bg: Rgba,
    border: Rgba,
    fg: Rgba,
}

/// Rest at 0, hover at 1. Channels stay inside the danger palette.
fn hover_face(progress: f32) -> Face {
    let progress = progress.clamp(0.0, 1.0);
    let colors = palette();
    Face {
        progress,
        bg: mix_rgb(colors.danger_bg, colors.danger_hover, progress),
        border: mix_rgb(colors.danger_border, colors.danger, progress),
        fg: mix_rgb(colors.danger, colors.danger_fg, progress),
    }
}

fn glyph(progress: f32) -> Transformation {
    Transformation::translate(point(px(0.0), px(RISE * progress)))
        .with_rotation(radians(TURN * progress))
        .with_scaling(size(1.0 + GROW * progress, 1.0 + GROW * progress))
}

fn mix_rgb(from: u32, to: u32, t: f32) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    let from = rgb(from);
    let to = rgb(to);
    Rgba {
        r: from.r + (to.r - from.r) * t,
        g: from.g + (to.g - from.g) * t,
        b: from.b + (to.b - from.b) * t,
        a: from.a + (to.a - from.a) * t,
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::AppContext as _;
    use gpui_kit::Transformation;

    use super::{glyph, hover_face, mix_rgb};
    use crate::theme::palette;

    fn channel(left: f32, right: f32) -> bool {
        (left - right).abs() < 1e-4
    }

    #[test]
    fn rests_on_the_danger_fill() {
        let face = hover_face(0.0);
        let rest = mix_rgb(palette().danger_bg, palette().danger_bg, 0.0);
        assert!(channel(face.bg.r, rest.r) && channel(face.bg.g, rest.g));
        assert!(channel(face.progress, 0.0));
        assert_eq!(glyph(0.0), Transformation::default());
    }

    #[test]
    fn ends_on_the_danger_hover_fill() {
        let face = hover_face(1.0);
        let hover = mix_rgb(palette().danger_bg, palette().danger_hover, 1.0);
        let edge = mix_rgb(palette().danger_border, palette().danger, 1.0);
        assert!(channel(face.bg.r, hover.r) && channel(face.bg.b, hover.b));
        assert!(channel(face.border.r, edge.r) && channel(face.border.g, edge.g));
        assert!(face.progress > 0.99);
        assert_ne!(glyph(1.0), Transformation::default());
    }

    /// Pointer motion must not bump the grid epoch. A different open skill
    /// drops the hover so the next button does not open already eased in.
    #[gpui_kit::test]
    fn hover_does_not_revise_the_grid(cx: &mut gpui_kit::TestAppContext) {
        crate::init_test(cx);
        let page = cx.new(|cx| super::MySkillsPage::new(cx));
        cx.update(|cx| {
            page.update(cx, |page, cx| {
                let before = page.replay_epochs();
                page.set_uninstall_hover(true, cx);
                assert!(page.uninstall_hover);
                assert_eq!(page.replay_epochs(), before);
                page.set_uninstall_hover(true, cx);
                assert_eq!(page.replay_epochs(), before);

                page.select_detail(Some("wizard".into()));
                assert!(!page.uninstall_hover);
                page.uninstall_hover = true;
                page.select_detail(Some("wizard".into()));
                assert!(page.uninstall_hover);
                page.select_detail(None);
                assert!(!page.uninstall_hover);
                assert_eq!(page.replay_epochs(), before);
            });
        });
    }
}
