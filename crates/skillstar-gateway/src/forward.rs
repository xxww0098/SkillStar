//! The turn state machine: one forward, from candidates to a ledger line.
//!
//! Dispatch hands every forwarding outcome here. The injected
//! [`UpstreamEnv`] supplies the pieces the gateway must not own (README D7):
//! the candidate upstreams for one model ref, the signing accounts, and the
//! ledger attribution of a winning candidate. This module owns the loop
//! around them: smart order, per-candidate signing, the upstream send, rest
//! seats for the failures, and the ledger hand-off.
//!
//! When no env is injected the listener keeps its old shape: a single
//! static origin from [`ServeOptions`](crate::ServeOptions) — the legacy
//! test path — or the plain 502 `no upstream`. A 401 with an env consumes
//! the turn's single self-heal (spec D9): the book's `reauthorize` hook may
//! renew the credentials, and the same candidate is then re-signed and
//! re-sent exactly once. A 401 that survives the heal is passed through,
//! lands as `auth` in the ledger, and parks the candidate in the auth rest.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::SystemTime;

use bytes::Bytes;
use http_body_util::Full;
use hyper::{Response, StatusCode};

use crate::ledger::ErrorKind;
use crate::route::order::RouteCandidate;
use crate::route::rest::{self, Rest, RestSeat, UpstreamFailure};
use crate::sign::{AccountBook, ProviderSnapshot, SignInput, sign_upstream};
use crate::trace::TurnFacts;
use crate::translate::Protocol;

/// One candidate upstream, as the injected resolver spelled it. The
/// endpoint is the origin without a path; the gateway appends the route's
/// own `url_path` to it, exactly as it does for the static origin.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Upstream {
    /// Seat key: unique within one resolve and stable across turns, because
    /// the rest table is keyed by it.
    pub id: String,
    /// The signing shape `sign_upstream` matches on (`codex`, `gemini-cli`,
    /// …), and the allowance key the account book answers for.
    pub catalog_id: String,
    /// Origin without a path, `http://host:port`.
    pub endpoint: String,
    /// Provider-row secret for the api-key signing shapes. `None` for the
    /// account catalogs, whose credentials the book supplies.
    pub provider: Option<ProviderSnapshot>,
}

/// Candidate upstreams for one model ref, per turn.
pub type Resolve = Box<dyn Fn(&str) -> Vec<Upstream> + Send + Sync>;
/// Ledger attribution of one candidate id: `(catalog, account)`.
pub type Attribute = Box<dyn Fn(&str) -> (String, String) + Send + Sync>;

/// The production upstream, injected by the app at assembly. Everything the
/// gateway must not read itself rides in here; the closures are called on
/// the turn path, so they must be cheap and side-effect free.
pub struct UpstreamEnv {
    /// Candidate upstreams for one model ref. An empty vector leaves the
    /// turn on the plain 502 `no upstream` path.
    pub resolve: Resolve,
    /// Signing accounts and stored allowances. The app implements this over
    /// Usage's stored rows (live-first, slice 02).
    pub book: Box<dyn AccountBook + Send + Sync>,
    /// Ledger attribution of one candidate id: the catalog column and the
    /// account label (a subscription id, or `key:` plus a fingerprint).
    /// Only the winning id is ever asked.
    pub attribute: Attribute,
}

impl std::fmt::Debug for UpstreamEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpstreamEnv").finish_non_exhaustive()
    }
}

/// Rest seats of earlier turns, keyed by candidate id. Process state: a
/// restart forgets every spell, which is the safe direction.
static RESTS: LazyLock<Mutex<HashMap<String, Rest>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// One finished candidate send: the reply as it left the upstream, or why
/// it never became one.
enum Sent {
    /// Connect, send, or body read failed. No status exists to pass through.
    Transport,
    /// Vision transcription refused the request before any send.
    Reject { status: u16, message: String },
    Reply {
        status: u16,
        headers: Vec<(String, String)>,
        bytes: Bytes,
    },
}

/// The 401 self-heal turn state (spec D9): whether this turn already spent
/// its single re-sign-and-resend. A plain stack flag the candidate loop
/// hands to [`attempt`]; single-shot by construction, because every path
/// that spends it either returns the turn's answer or re-sends exactly once.
struct TurnAuth {
    retried: bool,
}

/// The D9 gate, kept alone so its three limits stay testable: only a 401
/// reply, only once per turn, only before anything was committed to the
/// agent. 403/429/5xx never reach the hook (`is_auth_error` is 401-only).
fn may_self_heal(sent: &Sent, auth: &TurnAuth, committed: bool) -> bool {
    matches!(sent, Sent::Reply { status: 401, .. }) && !auth.retried && !committed
}

/// The turn-static pieces of one send: what leaves the gateway and how it
/// is translated, fixed for every candidate of one turn.
struct Outbound<'a> {
    protocol: Option<Protocol>,
    url_path: &'a str,
    body: &'a [u8],
    public_responses: bool,
}

/// Sign and send one candidate. A 401 spends the turn's single self-heal
/// (spec D9): the hook is asked at most once per turn, and when it answers
/// with fresh material — already written back where the book reads it — the
/// same candidate is re-signed and re-sent exactly once, then the result is
/// final whatever it is. `None` marks the process-bridge skip, which is not
/// a failure.
async fn attempt(
    env: &UpstreamEnv,
    candidate: &Upstream,
    outbound: &Outbound<'_>,
    auth: &mut TurnAuth,
    committed: bool,
) -> Option<(crate::sign::SignedUpstream, Sent)> {
    loop {
        let signed = sign_upstream(
            &*env.book,
            &SignInput {
                catalog_id: &candidate.catalog_id,
                provider: candidate.provider.as_ref(),
                body: outbound.body,
            },
            &mut |_url| {},
        );
        if signed.bridge {
            return None;
        }
        let sent = send_upstream(
            &candidate.endpoint,
            outbound.protocol,
            outbound.url_path,
            outbound.body,
            outbound.public_responses,
            &signed.headers,
        )
        .await;
        if may_self_heal(&sent, auth, committed)
            && env.book.reauthorize(&candidate.catalog_id).is_some()
        {
            // Spend the flag before the resend, so a second heal is
            // structurally impossible even if the resent reply 401s again.
            auth.retried = true;
            continue;
        }
        return Some((signed, sent));
    }
}

/// The reply the agent receives for one turn, plus what the ledger adds.
struct Answer {
    turn: Turn,
    /// `(catalog, account)` of the winning candidate, when routing ran and
    /// a candidate answered at all.
    won: Option<(String, String)>,
    /// The rest word lists' verdict, refining what status alone settles.
    refined: Option<ErrorKind>,
}

/// The reply as the agent sees it, plus the raw upstream bytes the ledger
/// reads usage from.
pub(crate) struct Turn {
    status: StatusCode,
    body: Vec<u8>,
    json: bool,
    /// The reply as the upstream sent it, kept from before the protocol
    /// translation back to the agent. Usage and the answered model are read
    /// from it when the agent-side body has neither — tokens were spent even
    /// when the rebuild failed. `None` marks a refusal generated here.
    upstream: Option<Bytes>,
}

impl Turn {
    fn text(status: StatusCode, message: &str) -> Self {
        Self {
            status,
            body: message.as_bytes().to_vec(),
            json: false,
            upstream: None,
        }
    }

    fn upstream(status: StatusCode, body: Vec<u8>, raw: Bytes) -> Self {
        Self {
            status,
            body,
            json: true,
            upstream: Some(raw),
        }
    }
}

/// Forward one turn and hand it to the ledger. `env` wins over `base`; the
/// static `base` is the legacy single-origin path the serve tests use.
pub(crate) async fn turn(
    env: Option<&UpstreamEnv>,
    base: Option<&str>,
    protocol: Option<Protocol>,
    url_path: &str,
    facts: &TurnFacts<'_>,
    public_responses: bool,
) -> Response<Full<Bytes>> {
    // After translation, before redaction. Raw image routes have no protocol
    // and stay byte-for-byte; a missing vision id leaves the body alone.
    let translated = match protocol {
        Some(protocol) => match crate::translate::upstream_body(protocol, facts.inbound) {
            Ok(body) => crate::effort::apply_upstream_effort(&body, ""),
            Err(_) => return respond(facts, Turn::text(StatusCode::BAD_REQUEST, "bad request"), None, None),
        },
        None => facts.inbound.to_vec(),
    };
    let translated = if public_responses {
        crate::chatgpt::shape_responses(&translated)
    } else {
        translated
    };
    let answer = match env {
        Some(env) => {
            let model = model_ref(facts.inbound, url_path);
            routed(env, &model, protocol, url_path, &translated, public_responses).await
        }
        None => match base {
            Some(base) => {
                let turn = static_forward(base, protocol, url_path, &translated, public_responses)
                    .await;
                Answer {
                    turn,
                    won: None,
                    refined: None,
                }
            }
            None => Answer {
                turn: Turn::text(StatusCode::BAD_GATEWAY, "no upstream"),
                won: None,
                refined: None,
            },
        },
    };
    respond(facts, answer.turn, answer.won, answer.refined)
}

/// The legacy single-origin forward: one send, no signing, no rest. This is
/// the behavior every serve test predating routing pins; production always
/// injects an env instead.
async fn static_forward(
    base: &str,
    protocol: Option<Protocol>,
    url_path: &str,
    translated: &[u8],
    public_responses: bool,
) -> Turn {
    let sent = send_upstream(base, protocol, url_path, translated, public_responses, &[]).await;
    finish_local(sent, protocol, public_responses)
}

/// The candidate loop: smart order, sign, send, rest, rotate.
async fn routed(
    env: &UpstreamEnv,
    model: &str,
    protocol: Option<Protocol>,
    url_path: &str,
    translated: &[u8],
    public_responses: bool,
) -> Answer {
    let candidates = dedupe((env.resolve)(model));
    if candidates.is_empty() {
        return Answer {
            turn: Turn::text(StatusCode::BAD_GATEWAY, "no upstream"),
            won: None,
            refined: None,
        };
    }
    // Smart order: whoever has room first, then unknown, then used up. The
    // allowances come from the book — the same place signing reads them.
    let ranked: Vec<RouteCandidate<'_>> = candidates
        .iter()
        .map(|candidate| RouteCandidate {
            id: candidate.id.as_str(),
            allowance: env.book.allowance(&candidate.catalog_id),
        })
        .collect();
    let order = crate::route::order::route_smart(&ranked);
    let ordered: Vec<&Upstream> = order
        .iter()
        .filter_map(|id| candidates.iter().find(|candidate| candidate.id == *id))
        .collect();
    let mut seats: Vec<RestSeat<'_>> = ordered
        .iter()
        .map(|candidate| RestSeat {
            id: candidate.id.as_str(),
            until: rest_until(&candidate.id),
        })
        .collect();
    let mut last: Option<Answer> = None;
    // `committed` is the word the rest table's next_candidate gates on: this
    // loop returns replies whole, so nothing is ever committed mid-turn and
    // the self-heal gate below shares that same fact.
    let committed = false;
    let mut auth = TurnAuth { retried: false };
    let outbound = Outbound {
        protocol,
        url_path,
        body: translated,
        public_responses,
    };
    loop {
        let now = SystemTime::now();
        let Some(seat) = rest::next_candidate(committed, &seats, now) else {
            break;
        };
        let id = seat.to_string();
        seats.retain(|left| left.id != id.as_str());
        let Some(candidate) = ordered.iter().find(|candidate| candidate.id == id) else {
            continue;
        };
        let Some((signed, sent)) = attempt(env, candidate, &outbound, &mut auth, committed).await
        else {
            // `anthropic` stays on the process bridge: no HTTP from this
            // path, and no rest either — skipping is not a failure. The
            // seat is already dropped, so the loop cannot pick it again.
            continue;
        };
        match sent {
            Sent::Reject { status, message } => {
                let status = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
                return Answer {
                    turn: Turn::text(status, &message),
                    won: None,
                    refined: None,
                };
            }
            Sent::Transport => {
                let failure = UpstreamFailure {
                    status: 0,
                    body: &[],
                    headers: &[],
                    now,
                    failures: failures_next(&id),
                    snapshot: signed.allowance,
                };
                let decided = rest::rest_after(&failure);
                note_rest(&id, &decided);
                last = Some(Answer {
                    turn: Turn::text(StatusCode::BAD_GATEWAY, "upstream request"),
                    won: attribution(env, &id),
                    refined: None,
                });
                continue;
            }
            Sent::Reply {
                status,
                headers,
                bytes,
            } => {
                if (100..400).contains(&status) {
                    // A success forgets the seat's earlier failures.
                    clear_rest(&id);
                    return Answer {
                        turn: finish_reply(protocol, public_responses, status, bytes),
                        won: attribution(env, &id),
                        refined: None,
                    };
                }
                let header_pairs: Vec<(&str, &str)> = headers
                    .iter()
                    .map(|(name, value)| (name.as_str(), value.as_str()))
                    .collect();
                let failure = UpstreamFailure {
                    status,
                    body: &bytes,
                    headers: &header_pairs,
                    now,
                    failures: failures_next(&id),
                    snapshot: signed.allowance,
                };
                let decided = rest::rest_after(&failure);
                if !rotates(status) {
                    if status == 401 {
                        // The heal was refused, not attempted, or resent
                        // into another 401: the credentials are dead for
                        // this process. Park the seat so later turns do not
                        // hammer a dead login — a re-login or a restart is
                        // what can change the answer.
                        note_rest(&id, &decided);
                    }
                    // A 401 the turn could not heal, or a 4xx the request
                    // itself earned, would fail the same way on the next
                    // candidate. Pass it through, classified for the ledger.
                    return Answer {
                        turn: finish_reply(protocol, public_responses, status, bytes),
                        won: attribution(env, &id),
                        refined: refined_kind(&decided),
                    };
                }
                note_rest(&id, &decided);
                last = Some(Answer {
                    turn: finish_reply(protocol, public_responses, status, bytes),
                    won: attribution(env, &id),
                    refined: refined_kind(&decided),
                });
                continue;
            }
        }
    }
    last.unwrap_or(Answer {
        turn: Turn::text(StatusCode::BAD_GATEWAY, "no upstream"),
        won: None,
        refined: None,
    })
}

/// Resolve a send into the agent-facing turn, for the paths that do not
/// rotate: a reject is a local refusal, a transport failure the plain 502.
fn finish_local(sent: Sent, protocol: Option<Protocol>, public_responses: bool) -> Turn {
    match sent {
        Sent::Reject { status, message } => {
            let status = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
            Turn::text(status, &message)
        }
        Sent::Transport => Turn::text(StatusCode::BAD_GATEWAY, "upstream request"),
        Sent::Reply {
            status,
            headers: _,
            bytes,
        } => finish_reply(protocol, public_responses, status, bytes),
    }
}

/// Translate one upstream reply back to the agent, keeping the raw bytes
/// for the ledger even when the rebuild fails.
fn finish_reply(
    protocol: Option<Protocol>,
    public_responses: bool,
    status: u16,
    raw: Bytes,
) -> Turn {
    let status_code = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    let unmasked = crate::redact::unmask_response(&raw);
    let outbound = match protocol {
        Some(protocol) => match crate::translate::outbound_body(protocol, &unmasked) {
            Ok(body) => body,
            // The agent-side rebuild failed, but the upstream reply is kept:
            // what it spent still belongs in the ledger.
            Err(_) => {
                return Turn::upstream(
                    StatusCode::BAD_GATEWAY,
                    b"upstream body".to_vec(),
                    Bytes::from(unmasked),
                );
            }
        },
        None => unmasked.clone(),
    };
    if public_responses
        && let Some(replaced) = crate::chatgpt::quota_reply(status, &outbound)
    {
        return Turn::upstream(status_code, replaced, Bytes::from(unmasked));
    }
    Turn::upstream(status_code, outbound, Bytes::from(unmasked))
}

/// POST one candidate: vision rewrite, redaction, the send, and the raw
/// reply bytes. Signing headers ride along as given. The shared stream
/// client is a cached clone, so building it per send costs a lock.
async fn send_upstream(
    base: &str,
    protocol: Option<Protocol>,
    url_path: &str,
    translated: &[u8],
    public_responses: bool,
    signed: &[(String, String)],
) -> Sent {
    // After translation, before redaction. Raw image routes have no protocol
    // and stay byte-for-byte; a missing vision id leaves the body alone.
    let upstream_bytes = if protocol.is_some() {
        match crate::vision::rewrite_forward(translated, base).await {
            Ok(bytes) => bytes,
            Err(reject) => {
                return Sent::Reject {
                    status: reject.status,
                    message: reject.message,
                };
            }
        }
    } else {
        translated.to_vec()
    };
    let upstream_bytes = crate::redact::mask_outbound(&upstream_bytes);
    let url = format!("{}{url_path}", base.trim_end_matches('/'));
    crate::outbound::note_outbound(&url);
    let client = match skillstar_core::infra::http_client::stream_http_client() {
        Ok(client) => client,
        Err(_) => return Sent::Transport,
    };
    let mut pending = client
        .post(url)
        .header(hyper::header::CONTENT_TYPE, "application/json");
    if public_responses {
        pending = pending.header(hyper::header::ACCEPT, "application/json");
    }
    for (name, value) in signed {
        // Signed names are fixed literals; a malformed one (a hostile env
        // closure) is dropped rather than failing the turn.
        if hyper::header::HeaderName::try_from(name.as_str()).is_ok() {
            pending = pending.header(name.as_str(), value.as_str());
        }
    }
    let pending = pending.body(upstream_bytes);
    let response = match skillstar_core::infra::http_client::send_stream(pending).await {
        Ok(response) => response,
        Err(_) => return Sent::Transport,
    };
    let status = response.status().as_u16();
    let headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|text| (name.as_str().to_string(), text.to_string()))
        })
        .collect();
    let bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(_) => return Sent::Transport,
    };
    Sent::Reply {
        status,
        headers,
        bytes,
    }
}

/// The ledger tail: one ring entry, one ledger line, one response.
fn respond(
    facts: &TurnFacts<'_>,
    turned: Turn,
    won: Option<(String, String)>,
    refined: Option<ErrorKind>,
) -> Response<Full<Bytes>> {
    let status = turned.status;
    crate::trace::note_turn(
        facts,
        &crate::trace::TurnEnd {
            status: status.as_u16(),
            body: &turned.body,
            upstream_raw: turned.upstream.as_deref(),
            won: won.as_ref().map(|(catalog, account)| (catalog.as_str(), account.as_str())),
            error_kind: refined,
        },
    );
    let content_type = if turned.json {
        "application/json"
    } else {
        "text/plain; charset=utf-8"
    };
    Response::builder()
        .status(status)
        .header(hyper::header::CONTENT_TYPE, content_type)
        .body(Full::new(Bytes::from(turned.body)))
        .unwrap_or_else(|_| plain(StatusCode::INTERNAL_SERVER_ERROR, "response"))
}

fn plain(status: StatusCode, message: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header(hyper::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Full::new(Bytes::from(message.to_string())))
        .expect("plain response uses a valid status")
}

/// First candidate occurrence wins; a resolver that repeats an id cannot
/// spin the seat table.
fn dedupe(candidates: Vec<Upstream>) -> Vec<Upstream> {
    let mut seen: Vec<String> = Vec::new();
    candidates
        .into_iter()
        .filter(|candidate| {
            if seen.iter().any(|id| id == &candidate.id) {
                return false;
            }
            seen.push(candidate.id.clone());
            true
        })
        .collect()
}

/// The model ref one turn routes for: the body's `model` first, then a
/// Gemini path's `{model}:method` segment. Empty means no ref to route.
fn model_ref(inbound: &[u8], url_path: &str) -> String {
    #[derive(serde::Deserialize)]
    struct Envelope {
        model: Option<String>,
    }
    if let Ok(Envelope {
        model: Some(model),
    }) = serde_json::from_slice(inbound)
    {
        return model;
    }
    url_path
        .strip_prefix("/v1beta/models/")
        .and_then(|rest| rest.split(':').next())
        .unwrap_or("")
        .to_string()
}

/// Whether a failure status rotates to the next candidate: the shapes that
/// say "this candidate cannot serve right now". 401 is deliberately absent
/// (spec D9); a 400-class the request earned would repeat on every seat.
fn rotates(status: u16) -> bool {
    matches!(status, 402 | 408 | 429 | 500..=599)
}

/// The rest word lists' verdict, where status alone settles less: a quota
/// refusal and a verification refusal both name themselves in the body.
fn refined_kind(rest: &Rest) -> Option<ErrorKind> {
    match rest.why {
        "quota" => Some(ErrorKind::Quota),
        "verify" => Some(ErrorKind::Verify),
        _ => None,
    }
}

fn rest_until(id: &str) -> Option<SystemTime> {
    RESTS.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(id)
        .map(|rest| rest.until)
}

fn note_rest(id: &str, rest: &Rest) {
    RESTS.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(id.to_string(), rest.clone());
}

/// Failures in a row, counting this one: the previous seat's count plus one,
/// starting at one. A success clears the seat (see `routed`).
fn failures_next(id: &str) -> u32 {
    RESTS.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(id)
        .map(|rest| rest.failures.saturating_add(1))
        .unwrap_or(1)
}

fn clear_rest(id: &str) {
    RESTS.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(id);
}

/// The winning candidate's ledger columns, asked of the env only when a
/// candidate actually answered.
fn attribution(env: &UpstreamEnv, id: &str) -> Option<(String, String)> {
    let (catalog, account) = (env.attribute)(id);
    (!catalog.is_empty() || !account.is_empty()).then_some((catalog, account))
}

/// Test seam: drop every rest seat. The integration tests need turns that
/// start from a clean table.
#[cfg(test)]
pub(crate) fn clear_rests() {
    RESTS.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_ref_reads_the_body_then_the_gemini_path() {
        assert_eq!(
            model_ref(br#"{"model":"openai/gpt-test"}"#, "/v1/chat/completions"),
            "openai/gpt-test"
        );
        assert_eq!(
            model_ref(br#"{"messages":[]}"#, "/v1beta/models/gemini-x:generateContent"),
            "gemini-x"
        );
        assert_eq!(model_ref(b"{}", "/v1/chat/completions"), "");
    }

    #[test]
    fn rotation_covers_the_candidate_side_shapes_only() {
        assert!(rotates(402));
        assert!(rotates(408));
        assert!(rotates(429));
        assert!(rotates(500));
        assert!(rotates(503));
        assert!(!rotates(200));
        assert!(!rotates(301));
        // 401 heals in place instead of rotating; other 4xx are the request's
        // fault.
        assert!(!rotates(401));
        assert!(!rotates(400));
        assert!(!rotates(403));
        assert!(!rotates(404));
    }

    /// The D9 three limits, on the gate itself: only a 401, only once per
    /// turn, only before anything was committed. 403/429/5xx never qualify.
    #[test]
    fn the_self_heal_gate_holds_the_d9_limits() {
        let reply = |status: u16| Sent::Reply {
            status,
            headers: Vec::new(),
            bytes: Bytes::new(),
        };
        let fresh = TurnAuth { retried: false };
        let spent = TurnAuth { retried: true };
        assert!(may_self_heal(&reply(401), &fresh, false));
        assert!(!may_self_heal(&reply(401), &spent, false), "once per turn");
        assert!(
            !may_self_heal(&reply(401), &fresh, true),
            "a committed turn never retries"
        );
        for status in [200, 403, 429, 500] {
            assert!(
                !may_self_heal(&reply(status), &fresh, false),
                "{status} never reaches the hook"
            );
        }
        assert!(!may_self_heal(&Sent::Transport, &fresh, false));
        assert!(!may_self_heal(
            &Sent::Reject {
                status: 401,
                message: "vision".to_string(),
            },
            &fresh,
            false
        ));
    }

    #[test]
    fn duplicated_ids_keep_the_first_candidate() {
        let candidates = dedupe(vec![
            Upstream {
                id: "a".to_string(),
                catalog_id: "codex".to_string(),
                endpoint: "http://one".to_string(),
                provider: None,
            },
            Upstream {
                id: "a".to_string(),
                catalog_id: "kiro".to_string(),
                endpoint: "http://two".to_string(),
                provider: None,
            },
            Upstream {
                id: "b".to_string(),
                catalog_id: "codex".to_string(),
                endpoint: "http://three".to_string(),
                provider: None,
            },
        ]);
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].endpoint, "http://one");
        assert_eq!(candidates[1].id, "b");
    }

    #[test]
    fn rest_seats_round_trip_and_clear() {
        clear_rests();
        let rest = rest::rest_after(&UpstreamFailure {
            status: 429,
            body: b"Too many requests",
            headers: &[],
            now: SystemTime::UNIX_EPOCH,
            failures: 1,
            snapshot: None,
        });
        note_rest("seat-a", &rest);
        assert_eq!(rest_until("seat-a"), Some(rest.until));
        assert_eq!(failures_next("seat-a"), rest.failures.saturating_add(1));
        clear_rest("seat-a");
        assert_eq!(rest_until("seat-a"), None);
        assert_eq!(failures_next("seat-a"), 1);
    }
}
