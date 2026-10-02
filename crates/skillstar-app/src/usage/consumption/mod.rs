//! The merged consumption view: gateway ledger records plus session-file
//! calls, one row per real call (slice 07).
//!
//! The gateway ledger knows every turn that went through it, with
//! attribution (catalog, account, status); the agents' own session files
//! know every call, bypass traffic included. Merging them naively counts a
//! through-proxied turn twice — once as a record, once as a file line. This
//! module ports magpie `internal/usage` `gatewayMatches` (ledger.go) to
//! correlate the two sides and swallow the file line a record already
//! stands for.
//!
//! Matching, two stages, first one that answers wins:
//!
//! 1. **request id** — the primary key. The gateway `Record` has no
//!    `request_id` field yet (slice 03 landed without it), so
//!    [`record_request_id`] currently answers `None` for every record and
//!    this stage consumes nothing: pairing runs entirely on stage 2. When
//!    the gateway lane adds the field (and the extraction pipeline fills
//!    it), replacing that one accessor body activates stage 1 with no other
//!    change here.
//! 2. **session + tokens + time + failure** — same `(agent, session)`, the
//!    four token counts equal, the file call's timestamp inside the
//!    record's *end* (`at + latency_ms`) ± 2s, both sides agreeing on
//!    success or failure, zero-token calls only pairing failed-with-failed
//!    (an empty success carries too little evidence), and the pairing
//!    unique on both sides — one record up against two equal candidates
//!    abandons both and every row stays visible.
//!
//! Deviations from magpie, deliberate:
//!
//! - magpie skips records it knows the gateway itself rejected before an
//!   upstream was involved. SkillStar's `Record` has no rejection marker
//!   yet (routing/attribution is still landing), so failed records
//!   participate here and can only pair with calls that also failed.
//!   Revisit when the gateway can mark self-generated refusals.
//! - matching never compares model ids, exactly like magpie: the tuple
//!   above is already tight enough, and a wrong normalization would split
//!   real pairs. [`same_model`] normalizes model ids for the slices that
//!   need equivalence judgments; it never rewrites the ids a row keeps.
//!
//! Caller contract for [`consumption_view`]:
//!
//! - Pass the gateway pool read with a 24h buffer *before* the window floor
//!   (a turn begun before midnight can still consume a file call written
//!   after it). Matching always runs over everything given; the window then
//!   filters rows by `at`. A cross-boundary turn therefore shows nowhere in
//!   the narrower window — its record falls below the floor and its file
//!   line is consumed. That is magpie's behavior too; the buffer exists so
//!   the *pairing* is computed, not so the record is displayed.
//! - Pure functions throughout: the clock reaches [`Window`] as a
//!   constructor argument, and nothing here reads the wall clock, the
//!   filesystem, or the network.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use chrono::{Local, LocalResult, TimeZone};
use skillstar_gateway::{ErrorKind, Record, TokenCounts};
use skillstar_usage::sessions::SessionCall;

mod summarize;

pub use summarize::{
    Dimension, Group, Period, SeriesPoint, SummarizeInput, Summary, Totals, groups,
    period_floor_ms, summarize,
};

mod crossview;

pub use crossview::{CandidateFact, route_comparison, today_from_rows, today_consumption};

/// Catalog label for a call only the session file knows (magpie's
/// `session-unknown`: no gateway attribution exists for bypass traffic).
pub const SESSION_UNKNOWN_CATALOG: &str = "session-unknown";

/// How far a file call's timestamp may sit from the record's end time and
/// still be the same turn (magpie: 2s).
const MATCH_WINDOW_MS: i64 = 2_000;

/// One day in milliseconds; the unit of the window spans.
const DAY_MS: i64 = 86_400_000;

#[cfg(test)]
#[path = "consumption_tests.rs"]
mod tests;

/// Which world a consumption row came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallSource {
    /// The gateway saw the turn: full attribution, status included.
    Gateway,
    /// Only the agent's session file knows the call (agent bypassed the
    /// gateway): no status, no account; catalog is
    /// [`SESSION_UNKNOWN_CATALOG`].
    SessionFile,
}

/// One call as the merged view lists it: the projection of a gateway
/// [`Record`] or a session-file [`SessionCall`] onto one shape, plus the
/// request id. Fields one side cannot know arrive empty or `None` rather
/// than being invented; ids keep their original spelling (normalization is
/// for matching only, never for display).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnifiedCall {
    /// Unix milliseconds. Gateway: dispatch entry (`Record.at`); session
    /// file: the call's own timestamp.
    pub at: i64,
    /// Owning agent id (the AGENT_SPECS vocabulary both sides share).
    pub agent: String,
    /// The session id the agent presented to the gateway; empty when
    /// unknown.
    pub session: String,
    /// The model identity the agent ran as / asked for.
    pub model_asked: String,
    /// The model the reply named.
    pub model_answered: String,
    /// Token counts. The session vocabulary's four counts plus the
    /// gateway-only reasoning count (files never name it: 0 there).
    pub tokens: TokenCounts,
    /// Effort hint, session files only.
    pub effort: Option<String>,
    /// Request id, session files only — until [`record_request_id`] stops
    /// answering `None`.
    pub request_id: Option<String>,
    /// Failure category (`None` = the call succeeded). Gateway records the
    /// ledger's snake_case kind; files their free-form word.
    pub error_kind: Option<String>,
    /// Turn duration as the source measured it.
    pub latency_ms: Option<u64>,
    /// Gateway attribution slot in the catalog; `SESSION_UNKNOWN_CATALOG`
    /// on session-file rows.
    pub catalog: String,
    /// Account the gateway charged; empty on session-file rows.
    pub account: String,
    /// HTTP status the gateway saw; `None` on session-file rows.
    pub status: Option<u16>,
    /// Inbound endpoint path; empty on session-file rows.
    pub endpoint: String,
    /// Locate-and-reread triple (session files only): where the original
    /// conversation text lives.
    pub file: Option<PathBuf>,
    /// Byte offset the call's interval starts at (see `file`).
    pub from: Option<u64>,
    /// Byte offset the call's interval ends at (see `file`).
    pub to: Option<u64>,
}

impl UnifiedCall {
    /// Project a gateway record; the call is attributed and complete.
    fn from_record(record: &Record) -> Self {
        Self {
            at: record.at,
            agent: record.agent.clone(),
            session: record.session.clone(),
            model_asked: record.model_asked.clone(),
            model_answered: record.model_answered.clone(),
            tokens: record.tokens,
            effort: None,
            request_id: record_request_id(record).map(str::to_string),
            error_kind: record.error_kind.as_ref().map(error_kind_name),
            latency_ms: Some(record.latency_ms),
            catalog: record.catalog.clone(),
            account: record.account.clone(),
            status: Some(record.status),
            endpoint: record.endpoint.clone(),
            file: None,
            from: None,
            to: None,
        }
    }

    /// Project a session-file call; what the gateway would know stays
    /// empty, and the catalog marks the missing attribution.
    fn from_session_call(call: &SessionCall) -> Self {
        Self {
            at: call.at,
            agent: call.agent.clone(),
            session: call.session.clone(),
            model_asked: call.model_asked.clone(),
            model_answered: call.model_answered.clone(),
            tokens: TokenCounts {
                input: call.tokens.input,
                output: call.tokens.output,
                cache_read: call.tokens.cache_read,
                cache_write: call.tokens.cache_write,
                reasoning: 0,
            },
            effort: call.effort.clone(),
            request_id: call
                .request_id
                .clone()
                .filter(|id| !id.is_empty()),
            error_kind: call
                .error_kind
                .clone()
                .filter(|kind| !kind.is_empty()),
            latency_ms: call.latency_ms,
            catalog: SESSION_UNKNOWN_CATALOG.to_string(),
            account: String::new(),
            status: None,
            endpoint: String::new(),
            file: Some(call.file.clone()),
            from: Some(call.from),
            to: Some(call.to),
        }
    }
}

/// One row of the merged view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsumptionRow {
    pub call: UnifiedCall,
    pub source: CallSource,
}

/// The merged, deduplicated view over one window, newest first (ties keep
/// the gateway row ahead of the session-file row; both passes run in input
/// order and the sort is stable).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConsumptionView {
    pub rows: Vec<ConsumptionRow>,
}

/// Which calls a view keeps: the period containing `now_ms` (the injected
/// clock — nothing here reads the wall clock).
///
/// - `today` — the local day of `now_ms`, from local midnight.
/// - `week` / `month` — the trailing 7 / 30 calendar days, the spans
///   magpie's periods use.
/// - `all` — everything.
///
/// Rows are kept by when they *began* (`at`), floor inclusive. The gateway
/// pool the caller passes should still be read with a 24h buffer before
/// [`Window::floor_ms`] so cross-boundary turns pair correctly — see the
/// module docs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    kind: WindowKind,
    now_ms: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowKind {
    Today,
    Week,
    Month,
    All,
}

impl Window {
    /// The local day containing `now_ms`.
    pub fn today(now_ms: i64) -> Self {
        Self::new(WindowKind::Today, now_ms)
    }

    /// The trailing 7 calendar days ending today.
    pub fn week(now_ms: i64) -> Self {
        Self::new(WindowKind::Week, now_ms)
    }

    /// The trailing 30 calendar days ending today.
    pub fn month(now_ms: i64) -> Self {
        Self::new(WindowKind::Month, now_ms)
    }

    /// No time filter.
    pub fn all() -> Self {
        Self::new(WindowKind::All, 0)
    }

    fn new(kind: WindowKind, now_ms: i64) -> Self {
        Self { kind, now_ms }
    }

    /// The period start in Unix milliseconds (inclusive), or `None` for
    /// all time. Callers use this to compute the buffered gateway read
    /// (`floor − 24h`).
    pub fn floor_ms(&self) -> Option<i64> {
        match self.kind {
            WindowKind::All => None,
            WindowKind::Today => Some(local_midnight_ms(self.now_ms)),
            WindowKind::Week => Some(local_midnight_ms(self.now_ms) - 6 * DAY_MS),
            WindowKind::Month => Some(local_midnight_ms(self.now_ms) - 29 * DAY_MS),
        }
    }
}

/// Local midnight (00:00:00 of the local day containing `now_ms`), as Unix
/// milliseconds. Falls back to `now_ms` itself only for clock values no
/// calendar can hold.
fn local_midnight_ms(now_ms: i64) -> i64 {
    let Some(now) = Local.timestamp_millis_opt(now_ms).single() else {
        return now_ms;
    };
    let midnight = match now.date_naive().and_hms_opt(0, 0, 0) {
        Some(midnight) => midnight,
        // 00:00:00 is always a constructible time of day; unreachable.
        None => return now_ms,
    };
    match Local.from_local_datetime(&midnight) {
        LocalResult::Single(time) => time.timestamp_millis(),
        // A zone whose offset changed across midnight picked one of the two
        // instants already; take the earlier edge.
        LocalResult::Ambiguous(earlier, _) => earlier.timestamp_millis(),
        // A DST jump that swallowed this day's midnight (no shipping zone
        // does this): the instant now_ms less the wall-clock time of day,
        // which is that date's 00:00 under the offset in force now.
        LocalResult::None => now_ms - ms_since_local_midnight(&now),
    }
}

/// Milliseconds between `now`'s wall clock and its own date's 00:00 read
/// as a naive instant.
fn ms_since_local_midnight(now: &chrono::DateTime<Local>) -> i64 {
    now.naive_local()
        .and_utc()
        .timestamp_millis()
        .rem_euclid(DAY_MS)
}

/// The record↔call correlation [`consumption_view`] deduplicates by:
/// call index (into the `calls` slice) → record index (into `records`).
/// At most one record per call and at most one call per record — ambiguous
/// pairs are abandoned on purpose and both rows stay visible.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MatchMap {
    pairs: BTreeMap<usize, usize>,
}

impl MatchMap {
    fn new(pairs: impl IntoIterator<Item = (usize, usize)>) -> Self {
        Self {
            pairs: pairs.into_iter().collect(),
        }
    }

    /// The record index this call was consumed by, if any.
    pub fn record_of(&self, call_index: usize) -> Option<usize> {
        self.pairs.get(&call_index).copied()
    }

    /// Whether the record already stands for this call (the file line is
    /// swallowed).
    pub fn contains_call(&self, call_index: usize) -> bool {
        self.pairs.contains_key(&call_index)
    }

    /// The pairs, ordered by call index.
    pub fn iter(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.pairs.iter().map(|(call, record)| (*call, *record))
    }

    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }
}

/// Correlate gateway records with session-file calls: which calls a record
/// already stands for. Pure; see the module docs for the two-stage
/// semantics and the [`record_request_id`] degradation.
pub fn gateway_matches(records: &[Record], calls: &[SessionCall]) -> MatchMap {
    let record_sides: Vec<RecordSide> = records.iter().map(record_side).collect();
    let call_sides: Vec<CallSide> = calls.iter().map(call_side).collect();
    MatchMap::new(pair_sides(&record_sides, &call_sides))
}

/// The gateway record's request id, when the ledger line carries one.
///
/// **Degraded seam:** gateway `Record` has no `request_id` field yet
/// (slice 03 landed without it; magpie's primary-key stage matches on it).
/// Until the gateway lane adds the field and the extraction pipeline fills
/// it, this answers `None` for every record: stage-1 matching consumes
/// nothing and pairing degrades to the session/tokens/time stage — which
/// carries all of today's correlation on its own. Swap this body for the
/// field read and stage 1 activates with no other change.
fn record_request_id(_record: &Record) -> Option<&str> {
    None
}

/// A gateway record reduced to its matching identity.
#[derive(Clone, Debug)]
struct RecordSide {
    request_id: Option<String>,
    agent: String,
    session: String,
    /// input, output, cache_read, cache_write.
    tokens: [u64; 4],
    failed: bool,
    /// `at + latency_ms`: the moment the turn ended — what a file call's
    /// timestamp is compared against.
    end_ms: i64,
}

/// A session-file call reduced to its matching identity.
#[derive(Clone, Debug)]
struct CallSide {
    request_id: Option<String>,
    agent: String,
    session: String,
    tokens: [u64; 4],
    failed: bool,
    at_ms: i64,
}

fn record_side(record: &Record) -> RecordSide {
    RecordSide {
        request_id: record_request_id(record)
            .map(str::to_string)
            .filter(|id| !id.is_empty()),
        agent: record.agent.clone(),
        session: record.session.clone(),
        tokens: tokens4(record.tokens),
        // magpie's Failed(): an error status or an error the reply named.
        failed: record.status >= 400 || record.error_kind.is_some(),
        end_ms: record
            .at
            .saturating_add(i64::try_from(record.latency_ms).unwrap_or(i64::MAX)),
    }
}

fn call_side(call: &SessionCall) -> CallSide {
    CallSide {
        request_id: call
            .request_id
            .clone()
            .filter(|id| !id.is_empty()),
        agent: call.agent.clone(),
        session: call.session.clone(),
        tokens: [
            call.tokens.input,
            call.tokens.output,
            call.tokens.cache_read,
            call.tokens.cache_write,
        ],
        failed: call
            .error_kind
            .as_deref()
            .is_some_and(|kind| !kind.is_empty()),
        at_ms: call.at,
    }
}

fn tokens4(tokens: TokenCounts) -> [u64; 4] {
    [tokens.input, tokens.output, tokens.cache_read, tokens.cache_write]
}

/// The two-stage correlation over the projected sides; `(call index,
/// record index)` pairs, ordered by call index. The projection seam is
/// what makes the request-id stage testable today: [`record_side`] feeds
/// it `None`s, direct [`RecordSide`] construction need not.
fn pair_sides(records: &[RecordSide], calls: &[CallSide]) -> Vec<(usize, usize)> {
    let mut by_id: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut by_session: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, record) in records.iter().enumerate() {
        if let Some(id) = record.request_id.as_deref() {
            by_id.entry(id).or_default().push(index);
        }
        if !record.session.is_empty() {
            by_session
                .entry(record.session.as_str())
                .or_default()
                .push(index);
        }
    }

    // Stage 1 — request id, the primary key: the first unused record with
    // the same id consumes the call, whatever else differs.
    let mut pair_of_call: Vec<Option<usize>> = vec![None; calls.len()];
    let mut used = vec![false; records.len()];
    for (index, call) in calls.iter().enumerate() {
        let Some(id) = call.request_id.as_deref() else {
            continue;
        };
        if let Some(record_index) = by_id
            .get(id)
            .and_then(|indices| indices.iter().copied().find(|i| !used[*i]))
        {
            pair_of_call[index] = Some(record_index);
            used[record_index] = true;
        }
    }

    // Stage 2 — session, tokens, time and failure agreement; kept only
    // when the pairing is unique on both sides.
    let mut candidates: Vec<Vec<usize>> = vec![Vec::new(); calls.len()];
    let mut counts = vec![0usize; records.len()];
    for (index, call) in calls.iter().enumerate() {
        if pair_of_call[index].is_some() || call.session.is_empty() {
            continue;
        }
        let Some(session_records) = by_session.get(call.session.as_str()) else {
            continue;
        };
        for &record_index in session_records {
            if used[record_index] {
                continue;
            }
            let record = &records[record_index];
            // Both sides carry request ids and stage 1 did not pair them:
            // different calls, and fallback must not second-guess the key.
            if call.request_id.is_some() && record.request_id.is_some() {
                continue;
            }
            // An empty success carries too little evidence to pair. Failed
            // calls may have zero tokens, but only with a record that
            // failed the same way.
            if call.tokens.iter().all(|count| *count == 0)
                && (!call.failed || !record.failed)
            {
                continue;
            }
            if call.failed != record.failed {
                continue;
            }
            if call.agent != record.agent || call.tokens != record.tokens {
                continue;
            }
            if call.at_ms < record.end_ms.saturating_sub(MATCH_WINDOW_MS)
                || call.at_ms > record.end_ms.saturating_add(MATCH_WINDOW_MS)
            {
                continue;
            }
            candidates[index].push(record_index);
            counts[record_index] += 1;
        }
    }
    for (index, candidate) in candidates.iter().enumerate() {
        if candidate.len() == 1 && counts[candidate[0]] == 1 {
            pair_of_call[index] = Some(candidate[0]);
        }
    }

    pair_of_call
        .into_iter()
        .enumerate()
        .filter_map(|(call_index, record_index)| {
            record_index.map(|record_index| (call_index, record_index))
        })
        .collect()
}

/// The merged, deduplicated consumption view over `window`.
///
/// Gateway records surface as [`CallSource::Gateway`] rows; session-file
/// calls a record already stands for are swallowed; the rest surface as
/// [`CallSource::SessionFile`] rows with [`SESSION_UNKNOWN_CATALOG`]. Rows
/// enter newest-first; the window floor (inclusive) filters by `at` after
/// matching, so cross-boundary turns pair on the buffered pool first.
pub fn consumption_view(
    records: &[Record],
    calls: &[SessionCall],
    window: Window,
) -> ConsumptionView {
    let matches = gateway_matches(records, calls);
    let floor = window.floor_ms();
    let keeps = |at: i64| floor.is_none_or(|floor| at >= floor);
    let mut rows = Vec::with_capacity(records.len() + calls.len());
    for record in records {
        if keeps(record.at) {
            rows.push(ConsumptionRow {
                call: UnifiedCall::from_record(record),
                source: CallSource::Gateway,
            });
        }
    }
    for (index, call) in calls.iter().enumerate() {
        if matches.contains_call(index) {
            continue;
        }
        if keeps(call.at) {
            rows.push(ConsumptionRow {
                call: UnifiedCall::from_session_call(call),
                source: CallSource::SessionFile,
            });
        }
    }
    rows.sort_by_key(|row| std::cmp::Reverse(row.call.at));
    ConsumptionView { rows }
}

/// The ledger's snake_case name for an [`ErrorKind`], read through the
/// same serde renaming the JSONL line uses, so the row can never disagree
/// with the stored line.
fn error_kind_name(kind: &ErrorKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Whether two model ids name the same model for matching: the same bare
/// id after folding away vendor prefixes, paths, version tails and
/// context-size suffixes — `gpt-5` and `gpt-5-2025-08-07` are one model,
/// `deepseek-v3` and `deepseek-v2` are not. Empty ids name nothing and
/// never equate. Matching decisions only: a row keeps the id it was given.
///
/// Delivered ahead of the wiring slices that need equivalence judgments;
/// [`gateway_matches`] itself does not compare model ids (magpie does not
/// either — see the module docs).
pub fn same_model(a: &str, b: &str) -> bool {
    let (bare_a, bare_b) = (bare_model(a), bare_model(b));
    !bare_a.is_empty() && bare_a == bare_b
}

/// A model id with only what identifies the model: lowercase, no path, no
/// vendor prefix, no version tail, no context-size suffix (magpie
/// `internal/usage/served.go` `bareModel`, hand-rolled to keep this crate
/// regex-free).
fn bare_model(id: &str) -> String {
    let lowered = id.trim().to_lowercase();
    let without_context = strip_context_suffix(&lowered);
    let last_segment = match without_context.rfind('/') {
        Some(pos) => &without_context[pos + 1..],
        None => without_context,
    };
    strip_version_tail(strip_vendor_prefix(last_segment)).to_string()
}

/// A trailing bracketed group holding no bracket of its own — Claude
/// Code's context-size marker (`claude-opus-5[1m]`).
fn strip_context_suffix(id: &str) -> &str {
    if !id.ends_with(']') {
        return id;
    }
    match id.rfind('[') {
        Some(open) if !id[open + 1..id.len() - 1].contains(']') => &id[..open],
        _ => id,
    }
}

/// Bedrock-style vendor prefixes: `vendor.` and `region.vendor.` — the
/// maker names a path can carry (magpie `vendorDot`).
const VENDOR_NAMES: &[&str] = &[
    "anthropic",
    "amazon",
    "meta",
    "mistral",
    "cohere",
    "ai21",
    "deepseek",
    "qwen",
    "openai",
    "google",
    "moonshotai",
    "minimax",
    "zai",
];

fn strip_vendor_prefix(id: &str) -> &str {
    for vendor in VENDOR_NAMES {
        if let Some(rest) = after_vendor_dot(id, vendor) {
            return rest;
        }
    }
    id
}

/// What follows `vendor.` when `id` is `vendor.…` or `region.vendor.…`.
fn after_vendor_dot<'a>(id: &'a str, vendor: &str) -> Option<&'a str> {
    if let Some(rest) = id.strip_prefix(vendor).and_then(|after| after.strip_prefix('.')) {
        return Some(rest);
    }
    let bytes = id.as_bytes();
    let letters = bytes.iter().take_while(|byte| byte.is_ascii_lowercase()).count();
    if (2..=4).contains(&letters) && bytes.get(letters) == Some(&b'.') {
        let rest = &id[letters + 1..];
        if let Some(after) = rest.strip_prefix(vendor) {
            return after.strip_prefix('.');
        }
    }
    None
}

/// The version tail a vendor puts after a model's name: one or more
/// `separator + atom` groups at the very end. The earliest separator whose
/// suffix decomposes entirely into atoms is cut (magpie `versionTail`);
/// never the whole id.
fn strip_version_tail(id: &str) -> &str {
    let bytes = id.as_bytes();
    for start in 1..bytes.len() {
        if is_tail_separator(bytes[start]) && is_atom_chain(&bytes[start + 1..]) {
            return &id[..start];
        }
    }
    id
}

fn is_tail_separator(byte: u8) -> bool {
    matches!(byte, b'-' | b'_' | b'@' | b':')
}

/// Whether `rest` is `atom (separator atom)*` — a whole version chain.
fn is_atom_chain(rest: &[u8]) -> bool {
    for len in atom_lengths(rest) {
        if len == rest.len() {
            return true;
        }
        if rest.get(len).is_some_and(|byte| is_tail_separator(*byte))
            && is_atom_chain(&rest[len + 1..])
        {
            return true;
        }
    }
    false
}

/// Every length under which the head of `rest` is one version atom: a
/// date (`2025-08-07`), a month-day (`08-07`), a 6-8 digit build, a 3-4
/// digit build, Bedrock's `v1:0`, or `latest` / `preview` / `exp`. A lone
/// 1-2 digit number is not an atom — `deepseek-v3` stays `deepseek-v3`.
fn atom_lengths(rest: &[u8]) -> Vec<usize> {
    let mut lengths = Vec::new();
    if rest.len() >= 10
        && rest[..4].iter().all(u8::is_ascii_digit)
        && rest[4] == b'-'
        && rest[5..7].iter().all(u8::is_ascii_digit)
        && rest[7] == b'-'
        && rest[8..10].iter().all(u8::is_ascii_digit)
    {
        lengths.push(10);
    }
    if rest.len() >= 5
        && rest[..2].iter().all(u8::is_ascii_digit)
        && rest[2] == b'-'
        && rest[3..5].iter().all(u8::is_ascii_digit)
    {
        lengths.push(5);
    }
    let digit_run = rest.iter().take_while(|byte| byte.is_ascii_digit()).count();
    for len in [8, 7, 6, 4, 3] {
        if len <= digit_run {
            lengths.push(len);
        }
    }
    if rest.first() == Some(&b'v') {
        let digits = rest[1..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        if digits > 0 && rest.get(1 + digits) == Some(&b':') {
            let tail_digits = rest[2 + digits..]
                .iter()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            if tail_digits > 0 {
                lengths.push(2 + digits + tail_digits);
            }
        }
    }
    for literal in ["latest", "preview", "exp"] {
        if rest.starts_with(literal.as_bytes()) {
            lengths.push(literal.len());
        }
    }
    lengths
}
