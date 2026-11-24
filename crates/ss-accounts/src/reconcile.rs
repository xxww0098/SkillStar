//! What each client is actually serving, read from the client.
//!
//! The card's current mark is disk truth, never a cached pin: reconcile reads
//! the client's own credential and matches it against the stored rows. A client
//! that cannot be read reports an empty identity and no error - the page still
//! has to render.

use serde::Serialize;

use crate::record::Account;
use crate::store::IndexEntry;
use crate::switch::{self, LiveCredential, PathRoots};
use crate::Agent;

/// One agent's live state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLive {
    /// The ss-skills agent id.
    pub agent: String,
    /// The identity the client serves, when it can be read.
    pub identity: Option<String>,
    /// User-visible name the client carries locally, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// The account id, when the live identity matches a stored account.
    pub account_id: Option<String>,
    /// Where the client keeps its credential.
    pub source: String,
}

/// Read every supported agent's live state. An unreadable client reports an
/// empty identity, never an error: the page must still render.
pub fn live_all() -> crate::AccountResult<Vec<AgentLive>> {
    let entries = match crate::store::index() {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(
                target: "accounts",
                error = %error,
                "accounts index is unreadable; reconcile cannot name accounts"
            );
            Vec::new()
        }
    };
    Ok(live_all_with(&entries))
}

/// Read every agent's live state against an already-read index in parallel,
/// preserving `Agent::supported()` order.
pub(crate) fn live_all_with(entries: &[IndexEntry]) -> Vec<AgentLive> {
    let roots = PathRoots::from_env();
    let agents = Agent::supported();
    std::thread::scope(|scope| {
        let handles: Vec<_> = agents
            .iter()
            .map(|agent| scope.spawn(|| live_one(&roots, entries, agent.id())))
            .collect();
        handles
            .into_iter()
            .zip(agents.iter())
            .map(|(handle, agent)| {
                handle.join().unwrap_or_else(|_| AgentLive {
                    agent: agent.id().to_string(),
                    identity: None,
                    display_name: None,
                    account_id: None,
                    source: switch::credential_path_with(&roots, agent.id())
                        .map(|path| path.display().to_string())
                        .unwrap_or_default(),
                })
            })
            .collect()
    })
}

/// Read a single agent's live state without probing the other agents.
pub(crate) fn live_for_agent(agent: &str) -> Option<AgentLive> {
    let entries = crate::store::index().ok()?;
    let roots = PathRoots::from_env();
    Some(live_one(&roots, &entries, agent))
}

fn live_one(roots: &PathRoots, entries: &[IndexEntry], agent: &str) -> AgentLive {
    match switch::live_with(roots, agent) {
        Ok(live) => AgentLive {
            agent: agent.to_string(),
            account_id: match_live(entries, agent, &live),
            identity: live.identity,
            display_name: live.display_name,
            source: live.source,
        },
        Err(error) => {
            tracing::debug!(
                target: "accounts",
                agent,
                error = %error,
                "no readable client credential for this agent"
            );
            AgentLive {
                agent: agent.to_string(),
                identity: None,
                display_name: None,
                account_id: None,
                source: switch::credential_path_with(roots, agent)
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
            }
        }
    }
}

#[cfg(test)]
#[path = "reconcile_tests.rs"]
mod reconcile_tests;

/// Which stored account a live client credential belongs to.
///
/// Identity first (an email or the provider's account id). When the client shows
/// neither - an old Codex credential with no account_id, say - the tokens it
/// holds are the only remaining tie, and their fingerprints decide.
pub(crate) fn match_live(
    entries: &[IndexEntry],
    agent: &str,
    live: &LiveCredential,
) -> Option<String> {
    let candidates: Vec<&IndexEntry> = entries
        .iter()
        .filter(|entry| entry.agent == agent)
        .collect();
    if let Some(identity) = live.identity.as_deref()
        && let Some(entry) = candidates
            .iter()
            .find(|entry| entry.matches_identity(identity))
    {
        return Some(entry.id.clone());
    }
    let by_fingerprint = |target: Option<&str>,
                          of_entry: fn(&IndexEntry) -> Option<&str>,
                          of_account: fn(&Account) -> Option<String>|
     -> Option<String> {
        let target = target?;
        candidates.iter().find_map(|entry| {
            if let Some(fp) = of_entry(entry) {
                return (fp == target).then(|| entry.id.clone());
            }
            if entry.access_fingerprint.is_some() {
                // Fingerprints are already indexed; missing refresh_fingerprint means no refresh token.
                return None;
            }
            let account = crate::store::get(&entry.id).ok()?;
            (of_account(&account).as_deref() == Some(target)).then(|| entry.id.clone())
        })
    };
    by_fingerprint(
        live.refresh_fingerprint.as_deref(),
        |entry| entry.refresh_fingerprint.as_deref(),
        Account::refresh_fingerprint,
    )
    .or_else(|| {
        by_fingerprint(
            live.access_fingerprint.as_deref(),
            |entry| entry.access_fingerprint.as_deref(),
            Account::access_fingerprint,
        )
    })
}
