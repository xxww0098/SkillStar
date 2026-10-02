//! Candidate order for one provider or group.
//!
//! Smart puts whoever has room first, then unknown, then used up.
//! Listed order, a caller-owned turn, and least-used sit beside that.
//! Least-used reads the snapshot it was given and does not ask for quota.
//! The stored `routing` field is read by `store::routing`.

use std::time::SystemTime;

/// Share of a window at which a candidate is treated as used up.
pub const USED_SHARE: f64 = 98.0;

/// What an injected usage snapshot says about one candidate.
///
/// `percent` is the share of one provider's own window already spent,
/// from 0 to 100. Vendors count their windows differently, so the number
/// is only comparable between candidates of the same provider:
/// `route_smart` and `usage_order` use it to rank within one resolve,
/// never to compare across providers. `renews_at` is when that window
/// resets, when the stored window knew; `None` means the reset time is
/// unknown and only the percent can act.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AllowanceSnapshot {
    pub percent: f64,
    pub renews_at: Option<SystemTime>,
}

/// One routable candidate. `allowance` is `None` when no snapshot was injected.
#[derive(Clone, Copy, Debug)]
pub struct RouteCandidate<'a> {
    pub id: &'a str,
    pub allowance: Option<AllowanceSnapshot>,
}

/// Smart order: room, then unknown, then used up.
pub fn route_smart(candidates: &[RouteCandidate<'_>]) -> Vec<String> {
    let mut room = Vec::new();
    let mut unknown = Vec::new();
    let mut spent = Vec::new();
    for candidate in candidates {
        let id = candidate.id.to_string();
        match candidate.allowance {
            Some(snapshot) if snapshot.percent >= USED_SHARE => spent.push(id),
            None => unknown.push(id),
            Some(_) => room.push(id),
        }
    }
    room.extend(unknown);
    room.extend(spent);
    room
}

/// How requests spread over one provider's or group's candidates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteMode {
    Smart,
    Order,
    Rotate,
    Usage,
}

impl RouteMode {
    /// `order`, `rotate`, and `usage` stay themselves. Every other string,
    /// including empty, is smart.
    pub fn parse(raw: &str) -> Self {
        match raw {
            "order" => Self::Order,
            "rotate" => Self::Rotate,
            "usage" => Self::Usage,
            _ => Self::Smart,
        }
    }

    /// File and control spelling. Smart is the word the control sends; the
    /// file omits that word.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Smart => "smart",
            Self::Order => "order",
            Self::Rotate => "rotate",
            Self::Usage => "usage",
        }
    }

    /// The four control values. Empty and any other word are rejected here.
    /// [`parse`](Self::parse) still treats those as smart when reading a file.
    pub fn from_control(raw: &str) -> Option<Self> {
        match raw {
            "smart" => Some(Self::Smart),
            "order" => Some(Self::Order),
            "rotate" => Some(Self::Rotate),
            "usage" => Some(Self::Usage),
            _ => None,
        }
    }
}

/// Order `candidates` and report the rotate counter to store next.
///
/// Fewer than two candidates leave both the list and `turn` alone. Only
/// rotate advances `turn`. The input slice is not reordered.
pub fn route_mode(
    mode: RouteMode,
    candidates: &[RouteCandidate<'_>],
    turn: u64,
) -> (Vec<String>, u64) {
    if candidates.len() < 2 {
        return (ids(candidates), turn);
    }
    match mode {
        RouteMode::Smart => (route_smart(candidates), turn),
        RouteMode::Order => (ids(candidates), turn),
        RouteMode::Rotate => rotate(candidates, turn),
        RouteMode::Usage => (usage_order(candidates), turn),
    }
}

fn ids(candidates: &[RouteCandidate<'_>]) -> Vec<String> {
    candidates
        .iter()
        .map(|candidate| candidate.id.to_string())
        .collect()
}

fn rotate(candidates: &[RouteCandidate<'_>], turn: u64) -> (Vec<String>, u64) {
    let len = candidates.len() as u64;
    let start = (turn % len) as usize;
    let mut order = ids(&candidates[start..]);
    order.extend(ids(&candidates[..start]));
    (order, turn + 1)
}

/// Least used first. A missing snapshot counts as nothing used. Equal
/// shares keep the caller's order.
fn usage_order(candidates: &[RouteCandidate<'_>]) -> Vec<String> {
    let mut ranked: Vec<usize> = (0..candidates.len()).collect();
    ranked.sort_by(|&left, &right| {
        share(&candidates[left])
            .total_cmp(&share(&candidates[right]))
            .then(left.cmp(&right))
    });
    ranked
        .into_iter()
        .map(|index| candidates[index].id.to_string())
        .collect()
}

fn share(candidate: &RouteCandidate<'_>) -> f64 {
    candidate
        .allowance
        .map(|snapshot| snapshot.percent)
        .unwrap_or(0.0)
}
