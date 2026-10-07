//! Quota-card face. The track width lives in `skill_card::tracks`.
//!
//! Family name on the top border, body inside. Account cells and the add
//! cell are the same face. This module does not choose a width or a row.

use gpui_kit::*;

use super::theme::palette;

/// Legend on the top border. It has to be a later sibling of the bordered
/// box: GPUI paints a parent's border over its children, so a legend inside
/// the card gets struck through.
pub(super) fn legend_frame(title: &str, body: impl IntoElement) -> Div {
    div()
        .relative()
        .w_full()
        .child(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap_3()
                .px_4()
                .pt(px(20.0))
                .pb(px(20.0))
                .border_1()
                .border_color(rgb(palette().os_line))
                .rounded(px(14.0))
                .child(body),
        )
        .child(
            div()
                .absolute()
                .top(px(-9.0))
                .left(px(16.0))
                .px_2()
                .bg(rgb(palette().panel))
                .text_size(px(15.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(palette().fg))
                .child(title.to_string()),
        )
}
