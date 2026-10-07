//! Read-time-priced summaries over the consumption view (slice 09).
//!
//! [`summarize`] is pure: the clock arrives as `now_ms` and the price table
//! as a closure, so a summary is a function of the rows it was handed and
//! nothing else. Pricing at read time is the accepted semantics — the price
//! table changes, and a change restates history (docs/features/usage, D8);
//! every cost here reads as an estimate of "what those tokens would cost
//! under today's table".
//!
//! Day boundaries are **UTC** — a summary is a wire contract shared by
//! every host and CI runner, so it pins one calendar.
//!
//! Series buckets switch with the period: hours inside a day, days inside a
//! week or month, ISO weeks over all time, each aligned to the Unix epoch
//! grid — pure arithmetic, no zone lookups.

use std::collections::BTreeMap;

use crate::pricing::ModelCost;

use super::UnifiedCall;

/// One hour in milliseconds.
const HOUR_MS: i64 = 3_600_000;
/// One day in milliseconds.
const DAY_MS: i64 = 86_400_000;
/// One week in milliseconds.
const WEEK_MS: i64 = 7 * DAY_MS;

/// Which slice of the view a summary covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    /// The UTC day containing `now_ms`.
    Today,
    /// The trailing 7 UTC calendar days ending today.
    Week,
    /// The trailing 30 UTC calendar days ending today.
    Month,
    /// Everything.
    All,
}

/// The period start in Unix milliseconds (inclusive, UTC), or `None` for
/// all time. Callers read the session files from here.
pub fn period_floor_ms(period: Period, now_ms: i64) -> Option<i64> {
    let today = utc_midnight_ms(now_ms);
    match period {
        Period::Today => Some(today),
        Period::Week => Some(today - 6 * DAY_MS),
        Period::Month => Some(today - 29 * DAY_MS),
        Period::All => None,
    }
}

/// 00:00:00 UTC of the day containing `now_ms`.
fn utc_midnight_ms(now_ms: i64) -> i64 {
    now_ms.div_euclid(DAY_MS) * DAY_MS
}

/// A grouping axis of a summary's `by` breakdowns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Dimension {
    Agent,
    Model,
    Session,
}

/// Aggregated counts and read-time-priced cost of a set of calls.
///
/// `cost_usd` is what the injected price table says *now*; `unpriced`
/// counts the calls no table entry priced (unknown, not free).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Totals {
    pub calls: u64,
    pub errors: u64,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub reasoning: u64,
    /// Estimated USD cost under the injected price table.
    pub cost_usd: f64,
    /// Calls the price table could not price.
    pub unpriced: u64,
    /// Mean turn duration over the calls that measured one.
    pub mean_latency_ms: f64,
    /// Sample count behind `mean_latency_ms` — bookkeeping, not a fact.
    latency_rows: u64,
}

impl Totals {
    /// Fold one call in, billing through `price` at read time. The price is
    /// looked up by served model alone — a session row does not name the
    /// provider that served it — so a lookup that answers `None` bumps
    /// [`Totals::unpriced`] instead of billing zero.
    pub fn add_call(&mut self, call: &UnifiedCall, price: &dyn Fn(&str) -> Option<ModelCost>) {
        self.calls += 1;
        if call.error_kind.is_some() {
            self.errors += 1;
        }
        self.input += call.tokens.input;
        self.output += call.tokens.output;
        self.cache_read += call.tokens.cache_read;
        self.cache_write += call.tokens.cache_write;
        self.reasoning += call.tokens.reasoning;
        match price(served_model(call)) {
            Some(cost) => self.cost_usd += cost.cost(&call.tokens),
            None => self.unpriced += 1,
        }
        if let Some(latency) = call.latency_ms {
            let samples = self.latency_rows;
            self.mean_latency_ms =
                (self.mean_latency_ms * samples as f64 + latency as f64) / (samples + 1) as f64;
            self.latency_rows = samples + 1;
        }
    }
}

/// The model id a row is billed and grouped under: the reply's model when it
/// named one, else what was asked for.
pub(crate) fn served_model(call: &UnifiedCall) -> &str {
    if call.model_answered.is_empty() {
        &call.model_asked
    } else {
        &call.model_answered
    }
}

/// One labeled group of a `by` breakdown.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub label: String,
    pub totals: Totals,
}

/// One non-empty series bucket.
#[derive(Clone, Debug, PartialEq)]
pub struct SeriesPoint {
    /// UTC bucket start in Unix milliseconds.
    pub bucket_start_ms: i64,
    pub totals: Totals,
}

/// The summary of one period: whole-period totals, a time series, and a
/// per-dimension breakdown.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub totals: Totals,
    pub series: Vec<SeriesPoint>,
    pub by: BTreeMap<Dimension, Vec<Group>>,
}

/// Everything [`summarize`] needs, with the clock and the price table
/// injected — nothing here reads the wall clock, the filesystem, or the
/// network.
pub struct SummarizeInput<'a> {
    pub now_ms: i64,
    pub rows: &'a [UnifiedCall],
    pub price: &'a dyn Fn(&str) -> Option<ModelCost>,
}

/// Summarize `input.rows` over `period`: totals, an auto-bucketed series,
/// and the three `by` breakdowns. Rows are kept by when they *began* (`at`),
/// floor inclusive, on the UTC calendar.
pub fn summarize(input: &SummarizeInput<'_>, period: Period) -> Summary {
    let floor = period_floor_ms(period, input.now_ms);
    let kept: Vec<&UnifiedCall> = input
        .rows
        .iter()
        .filter(|call| floor.is_none_or(|floor| call.at >= floor))
        .collect();

    let mut totals = Totals::default();
    let mut series: BTreeMap<i64, Totals> = BTreeMap::new();
    let bucket = bucket_ms(period);
    for call in &kept {
        totals.add_call(call, input.price);
        series
            .entry(bucket_start_ms(call.at, bucket))
            .or_default()
            .add_call(call, input.price);
    }

    let by = BTreeMap::from([
        (
            Dimension::Agent,
            groups(
                kept.iter().copied(),
                |call| Some(call.agent.clone()),
                input.price,
            ),
        ),
        (
            Dimension::Model,
            groups(
                kept.iter().copied(),
                |call| Some(served_model(call).to_string()),
                input.price,
            ),
        ),
        (
            Dimension::Session,
            groups(
                kept.iter().copied(),
                |call| non_empty(&call.session),
                input.price,
            ),
        ),
    ]);

    Summary {
        totals,
        series: series
            .into_iter()
            .map(|(bucket_start_ms, totals)| SeriesPoint {
                bucket_start_ms,
                totals,
            })
            .collect(),
        by,
    }
}

/// Group `rows` by a caller-chosen label and total each group; rows the
/// label function declines (`None`) stay out of that breakdown (an idless
/// call has no session). This is the engine behind [`summarize`]'s
/// dimensions.
pub fn groups<'a>(
    rows: impl IntoIterator<Item = &'a UnifiedCall>,
    label: impl Fn(&UnifiedCall) -> Option<String>,
    price: &dyn Fn(&str) -> Option<ModelCost>,
) -> Vec<Group> {
    let mut by: BTreeMap<String, Totals> = BTreeMap::new();
    for call in rows {
        if let Some(label) = label(call) {
            by.entry(label).or_default().add_call(call, price);
        }
    }
    by.into_iter()
        .map(|(label, totals)| Group { label, totals })
        .collect()
}

/// An owned copy of `value` unless it is empty — the "this side cannot
/// know" spelling that keeps unknown attributions out of a breakdown.
fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

/// The series bucket width of a period: hours inside a day, days inside a
/// week or month, weeks over all time.
fn bucket_ms(period: Period) -> i64 {
    match period {
        Period::Today => HOUR_MS,
        Period::Week | Period::Month => DAY_MS,
        Period::All => WEEK_MS,
    }
}

/// The UTC start of the bucket `at` falls into. Hours and days align to the
/// epoch grid (which is UTC midnight by definition); weeks align to Monday
/// 00:00 UTC — the epoch itself is a Thursday, so the week start shifts
/// back by three days.
fn bucket_start_ms(at: i64, bucket: i64) -> i64 {
    if bucket == WEEK_MS {
        let day = at.div_euclid(DAY_MS);
        return ((day + 3).div_euclid(7) * 7 - 3) * DAY_MS;
    }
    at.div_euclid(bucket) * bucket
}
