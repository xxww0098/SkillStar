//! The floating detail sheet the skill and marketplace detail columns wear.
//!
//! Both columns stay direct flex children of their page row — the track
//! keeps the width, height, and shrink the grid math has always subtracted
//! — but the track is no longer the surface: it paints a [`SHEET_GAP`]
//! ring as padding on every side, and the sheet inside that ring gets the
//! card tier's radius, a full hairline, and the elevation the kit's own
//! dialogs rest on. That geometry is what makes the column read as a
//! popup. The surface color stays `panel` — the color the kit theme calls
//! `background`, i.e. what every dialog in the app paints.
//!
//! The ring lives inside the reserved track, not outside it: a
//! [`DETAIL_COLUMN_W`](crate::layout::DETAIL_COLUMN_W) track shows the
//! sheet one [`SHEET_GAP`] narrower on every side. The sheet fills the
//! track by flex (`flex_1` inside the track's column), never by percent
//! height — percent fill inside the replayed canvas is what made
//! transient zero-height frames clip the whole drawer away.

use gpui_kit::*;

use crate::theme::palette;

/// Inset between the sheet and the edges of its reserved track, on all
/// four sides. The track paints this as padding.
pub(crate) const SHEET_GAP: f32 = 12.0;

/// The floating surface inside a padded detail track: card-tier radius
/// (the 16px the skill and market cards use), a full hairline, the kit
/// Dialog's resting shadow, and a column layout for the drawer's own
/// header/body/actions stack. Fills the track minus its ring.
pub(crate) fn sheet(base: Div) -> Div {
    base.flex_1()
        .min_w_0()
        .min_h_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(px(16.0))
        .bg(rgb(palette().panel))
        .border_1()
        .border_color(rgb(palette().border))
        .shadow(sheet_shadow())
}

/// The kit Dialog's resting elevation, verbatim: two soft black layers.
/// Repeated here because the dialog keeps it private; changing one surface
/// means changing the other, or the sheet stops reading as a dialog.
fn sheet_shadow() -> Vec<BoxShadow> {
    vec![
        BoxShadow::new(px(0.), px(20.), hsla(0., 0., 0., 0.1))
            .blur_radius(px(25.))
            .spread_radius(px(-5.)),
        BoxShadow::new(px(0.), px(8.), hsla(0., 0., 0., 0.1))
            .blur_radius(px(10.))
            .spread_radius(px(-6.)),
    ]
}

#[cfg(test)]
mod tests {
    use gpui_kit::component::Root;
    use gpui_kit::{
        AppContext, Context, InteractiveElement, IntoElement, ParentElement, Render, Styled,
        Window, div, px, size,
    };

    use super::{SHEET_GAP, sheet};

    /// The page-row shape both detail columns mount with: a fixed-height
    /// row, the track at full height with the ring as padding, the sheet
    /// filling what is left.
    struct SheetHost;

    impl Render for SheetHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().flex().size_full().child(
                div()
                    .id("sheet-track")
                    .w(px(crate::layout::DETAIL_COLUMN_W))
                    .h_full()
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .min_h_0()
                    .p(px(SHEET_GAP))
                    .child(sheet(div().debug_selector(|| "sheet-surface".into()))),
            )
        }
    }

    /// A detail track — what both grids reserve — shows the sheet one ring
    /// narrower on every side. The ring can never eat grid width, and the
    /// sheet cannot fall out of the track's column.
    #[gpui_kit::test]
    fn the_sheet_fills_the_track_minus_the_ring(cx: &mut gpui_kit::TestAppContext) {
        crate::init_test(cx);
        let (_root, cx) = cx.add_window_view(|window, cx| {
            let host = cx.new(|_| SheetHost);
            Root::new(host, window, cx)
        });
        cx.simulate_resize(size(px(1200.), px(800.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let sheet = cx.debug_bounds("sheet-surface").expect("sheet laid out");
        assert_eq!(
            sheet.size.width.as_f32() as i32,
            (crate::layout::DETAIL_COLUMN_W - SHEET_GAP * 2.0) as i32,
            "the sheet must stay inside the track the grid reserves"
        );
    }
}
