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

/// Read every agent's live state against an already-read index.
pub(crate) fn live_all_with(entries: &[IndexEntry]) -> Vec<AgentLive> {
    let roots = PathRoots::from_env();
    Agent::supported()
        .iter()
        .map(|agent| live_one(&roots, entries, agent.id()))
        .collect()
}

fn live_one(roots: &PathRoots, entries: &[IndexEntry], agent: &str) -> AgentLive {
    match switch::live_with(roots, agent) {
        Ok(live) => AgentLive {
            agent: agent.to_string(),
            account_id: match_live(entries, agent, &live),
            identity: live.identity,
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
    let by_fingerprint = |fingerprint: Option<&str>,
                          of: fn(&Account) -> Option<String>|
     -> Option<String> {
        let fingerprint = fingerprint?;
        candidates.iter().find_map(|entry| {
            let account = crate::store::get(&entry.id).ok()?;
            (of(&account).as_deref() == Some(fingerprint)).then(|| entry.id.clone())
        })
    };
    by_fingerprint(
        live.refresh_fingerprint.as_deref(),
        Account::refresh_fingerprint,
    )
    .or_else(|| {
        by_fingerprint(
            live.access_fingerprint.as_deref(),
            Account::access_fingerprint,
        )
    })
}
