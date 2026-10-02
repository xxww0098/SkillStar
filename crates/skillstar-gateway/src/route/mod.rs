//! Routing decisions over candidate lists: order, affinity, classification,
//! rules, rest, the stream hold, and group expansion. No file opens here.

pub(crate) mod affinity;
pub(crate) mod classify;
pub(crate) mod groups;
pub(crate) mod hold;
pub(crate) mod order;
pub(crate) mod rest;
pub(crate) mod rules;
