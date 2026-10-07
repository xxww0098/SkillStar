//! Shared box for skill-card, market-card, and group-card.
//!
//! The numbers live in `crate::layout`. Marketplace virtual rows copy
//! [`CARD_H`] onto every row, so the other surfaces must use the same height.
//! A shorter box clips the body; a taller one opens a gap between the virtual
//! row and the card.

pub use crate::layout::{CARD_GAP, SKILL_CARD_H as CARD_H, SKILL_CARD_W as CARD_W};
