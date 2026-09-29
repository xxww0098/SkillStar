//! One smart-routing decision: who still has room goes first.
//!
//! A candidate whose snapshot says 98 or more of the window is used goes
//! last. A candidate with no snapshot is unknown: after the ones with room,
//! and not with the ones that are used up. Order inside a group stays the
//! order the caller gave.

/// Share of a window at which a candidate is treated as used up.
pub const USED_SHARE: f64 = 98.0;

/// What an injected usage snapshot says about one candidate.
///
/// `used` is the percent of the window already spent, from 0 to 100.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AllowanceSnapshot {
    pub used: f64,
}

/// One routable candidate. `allowance` is `None` when no snapshot was injected.
#[derive(Clone, Copy, Debug)]
pub struct RouteCandidate<'a> {
    pub id: &'a str,
    pub allowance: Option<AllowanceSnapshot>,
}

/// Smart order for this slice: room, then unknown, then used up.
pub fn route_smart(candidates: &[RouteCandidate<'_>]) -> Vec<String> {
    let mut room = Vec::new();
    let mut unknown = Vec::new();
    let mut spent = Vec::new();
    for candidate in candidates {
        let id = candidate.id.to_string();
        match candidate.allowance {
            Some(snapshot) if snapshot.used >= USED_SHARE => spent.push(id),
            None => unknown.push(id),
            Some(_) => room.push(id),
        }
    }
    room.extend(unknown);
    room.extend(spent);
    room
}
