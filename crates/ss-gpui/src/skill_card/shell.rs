//! The shared card box. Skill, market, and group cards, and their loading
//! placeholders, all use this frame. A page paints the body and says which
//! card is selected. Radius, shadow, hover, and the selected face stay here.

use gpui_kit::*;

use super::grid::pin_width;
use super::size::{CARD_H, CARD_W};
use crate::chrome::{InteractionSpring, MotionDiv, MotionPaint};
use crate::theme::palette;

/// How wide the box is inside its parent.
pub enum CardWidth {
    /// Fill the grid track or list pane. min_w_0 lets a long title truncate.
    Fill,
    /// Pin to CARD_W. A lone tile outside a row.
    Fixed,
    /// List column that may grow up to max.
    FillMax(f32),
}

/// Which page card this box is. The face picks radius and shadow.
/// Hover and selection do not vary by face.
pub enum CardFace {
    /// 16px, shadow. skill-card.
    Skill,
    /// rounded_xl, shadow. market-card.
    Market,
    /// rounded_xl, no shadow. group-card.
    Deck,
}

enum CardRadius {
    Px16,
    Xl,
}

/// The outer box. Callers add flex direction, cursor, and children.
pub struct CardShell {
    pub id: ElementId,
    pub width: CardWidth,
    pub face: CardFace,
    /// Accent border and active fill. Open, checked, and expanded share it.
    pub selected: bool,
}

/// Loading tile on the same frame as a live card.
pub fn card_placeholder(id: impl Into<ElementId>, face: CardFace) -> MotionDiv {
    card_shell(CardShell {
        id: id.into(),
        width: CardWidth::Fill,
        face,
        selected: false,
    })
    .flex_shrink_0()
}

pub fn card_shell(shell: CardShell) -> MotionDiv {
    let CardShell {
        id,
        width,
        face,
        selected,
    } = shell;
    let (radius, shadow) = face_chrome(face);
    let key: SharedString = id.to_string().into();
    let (rest, hover) = card_face_paint(selected);
    let mut card = div()
        .id(id)
        .h(px(CARD_H))
        .overflow_hidden()
        .bg(rest.bg.expect("card face always paints a fill"))
        .border_1()
        .border_color(rest.border.expect("card face always paints a border"));
    card = match radius {
        CardRadius::Px16 => card.rounded(px(16.0)),
        CardRadius::Xl => card.rounded_xl(),
    };
    if shadow {
        card = card.shadow_sm();
    }
    let card = match width {
        CardWidth::Fill => card.w_full().min_w_0(),
        CardWidth::Fixed => pin_width(card, CARD_W),
        CardWidth::FillMax(max) => card.w_full().max_w(px(max)),
    };
    card.interaction_spring(key, true, rest, hover)
}

fn face_chrome(face: CardFace) -> (CardRadius, bool) {
    match face {
        CardFace::Skill => (CardRadius::Px16, true),
        CardFace::Market => (CardRadius::Xl, true),
        CardFace::Deck => (CardRadius::Xl, false),
    }
}

/// Marketplace hover: accent edge plus the hover fill. A selected card keeps
/// the accent edge and deepens the active fill instead of dropping back to gray.
fn card_face_paint(selected: bool) -> (MotionPaint, MotionPaint) {
    let rest_bg = rgb(if selected {
        palette().card_active
    } else {
        palette().card
    });
    let hover_bg = rgb(if selected {
        palette().card_active_hover
    } else {
        palette().card_hover
    });
    let rest_border = rgb(if selected {
        palette().accent
    } else {
        palette().border
    });
    let hover_border = rgb(palette().accent);
    (
        MotionPaint::new().bg(rest_bg).border(rest_border),
        MotionPaint::new().bg(hover_bg).border(hover_border),
    )
}

#[cfg(test)]
mod tests {
    use super::card_face_paint;
    use crate::theme::palette;

    #[test]
    fn hover_uses_the_market_blue_edge() {
        let idle = card_face_paint(false);
        assert_eq!(idle.0.bg, Some(gpui_kit::rgb(palette().card)));
        assert_eq!(idle.0.border, Some(gpui_kit::rgb(palette().border)));
        assert_eq!(idle.1.bg, Some(gpui_kit::rgb(palette().card_hover)));
        assert_eq!(idle.1.border, Some(gpui_kit::rgb(palette().accent)));

        let open = card_face_paint(true);
        assert_eq!(open.0.bg, Some(gpui_kit::rgb(palette().card_active)));
        assert_eq!(open.0.border, Some(gpui_kit::rgb(palette().accent)));
        assert_eq!(open.1.bg, Some(gpui_kit::rgb(palette().card_active_hover)));
        assert_eq!(open.1.border, Some(gpui_kit::rgb(palette().accent)));
    }
}
