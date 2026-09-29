//! Candidate order for one provider or group.
//!
//! Smart puts whoever has room first, then unknown, then used up.
//! Listed order, a caller-owned turn, and least-used sit beside that.
//! Least-used reads the snapshot it was given and does not ask for quota.
//!
//! The mode is the `routing` field on a provider or group row in
//! `config_dir()/model_gateway.json`. A missing file is smart. This module
//! does not create or rewrite that file.

use serde::Deserialize;

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

/// Smart order: room, then unknown, then used up.
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
}

/// Whose `routing` field to read in `model_gateway.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteOwner {
    Provider,
    Group,
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

/// Routing stored for one provider or group.
///
/// A missing file, a missing row, a missing or empty `routing` field, an
/// unknown string, or a file that is not JSON is smart. This does not
/// create or rewrite the file.
pub fn stored_route_mode(owner: RouteOwner, id: &str) -> RouteMode {
    let path = skillstar_core::infra::paths::config_dir().join("model_gateway.json");
    let Ok(bytes) = std::fs::read(&path) else {
        return RouteMode::Smart;
    };
    let Ok(file) = serde_json::from_slice::<GatewayFile>(&bytes) else {
        return RouteMode::Smart;
    };
    let rows = match owner {
        RouteOwner::Provider => &file.providers,
        RouteOwner::Group => &file.groups,
    };
    rows.iter()
        .find(|row| row.id == id)
        .map(|row| RouteMode::parse(row.routing.as_deref().unwrap_or("")))
        .unwrap_or(RouteMode::Smart)
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
        .map(|snapshot| snapshot.used)
        .unwrap_or(0.0)
}

#[derive(Deserialize)]
struct GatewayFile {
    #[serde(default)]
    providers: Vec<GatewayRow>,
    #[serde(default)]
    groups: Vec<GatewayRow>,
}

#[derive(Deserialize)]
struct GatewayRow {
    #[serde(default)]
    id: String,
    routing: Option<String>,
}
