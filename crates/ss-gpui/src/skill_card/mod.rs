//! Shared card pitch, and the skill / market / deck frame.
//!
//! Pages paint their own bodies. This module owns the track pitch (skill
//! cards and quota cards), the skill-card frame, the footer agent carousel,
//! and the source chip. Quota cards keep their legend face in `accounts`.

mod agent_rail;
mod grid;
mod shell;
mod size;
mod source_chip;

pub use agent_rail::{
    AgentRailClick, AgentRailSlot, agent_footer_bar, agent_rail, targetable_agent_profiles,
};
pub use grid::{
    card_content_width, card_row, card_rows, columns_for, grid_columns, pane_width, tracks,
};
pub use shell::{CardFace, CardShell, CardWidth, card_placeholder, card_shell};
pub use size::{CARD_GAP, CARD_H, CARD_W};
pub(crate) use source_chip::skill_source_chip;
