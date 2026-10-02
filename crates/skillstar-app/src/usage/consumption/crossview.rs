//! Cross-view projections over already-assembled data (spec slice 13).
//!
//! Two derived views, no new truth:
//!
//! - [`today_consumption`] — the merged view's today (UTC day, the same
//!   contract [`summarize`](super::summarize) pins) with one chip per
//!   session: the Usage page's drill-down from a card into the agent that
//!   ran it.
//! - [`route_comparison`] — one model ref's routable candidates compared
//!   over the **same** ledger records, so the numbers a user compares
//!   always share a scope. The consumption columns (`calls`, `error_rate`,
//!   the latency percentiles, `tokens`, `cost_usd`) aggregate only those
//!   records; the two process facts beside them — `resting` and the
//!   allowance percent — arrive as [`CandidateFact`] inputs because they
//!   live in the running gateway, not in the ledger.
//!
//! Everything here is pure: rows, records, candidates, and the price table
//! arrive as arguments, and the clock reaches `today_consumption` as
//! `now_ms`. The service layer (`service/summary.rs`) owns the reads.

use std::collections::BTreeMap;

use skillstar_gateway::{ModelCost, Record, TokenCounts, route_smart, RouteCandidate};

use super::{ConsumptionRow, Window, consumption_view, groups, period_floor_ms, same_model};
use super::summarize::{Period, Totals, served_model};
use crate::usage::dto::{
    ConsumptionTokensDto, RouteComparisonDto, RouteCostDto, SessionChipDto, TodayConsumptionDto,
};

/// One routable candidate as the comparison needs it: the ledger
/// attribution label plus the two live process facts the app layer reads
/// (allowance from the account book, rest from the gateway's seat table).
#[derive(Clone, Debug, PartialEq)]
pub struct CandidateFact {
    /// The provider row id (the resolve seam's candidate id).
    pub id: String,
    /// The ledger attribution catalog of that row — the column the ledger
    /// lines already carry, so records join on it.
    pub catalog: String,
    /// Share of the candidate's own usage window already spent, when the
    /// account book reported one.
    pub percent: Option<f64>,
    /// When that window renews, Unix milliseconds, when known.
    pub renews_at_ms: Option<i64>,
    /// Whether the gateway currently parks this candidate.
    pub resting: bool,
}

/// Today's consumption with session chips. Rows are the merged,
/// deduplicated view (the caller passes raw sources; matching runs here);
/// the day boundary is UTC, floor inclusive — the same contract as
/// [`summarize`](super::summarize::summarize).
pub fn today_consumption(
    now_ms: i64,
    records: &[Record],
    calls: &[skillstar_usage::sessions::SessionCall],
    price: &dyn Fn(&str, &str) -> Option<ModelCost>,
) -> TodayConsumptionDto {
    let rows = consumption_view(records, calls, Window::all()).rows;
    today_from_rows(now_ms, &rows, price)
}

/// The chips-and-totals projection over already-merged rows (the pure seam
/// tests drive; `today_consumption` is its read-side wrapper).
pub fn today_from_rows(
    now_ms: i64,
    rows: &[ConsumptionRow],
    price: &dyn Fn(&str, &str) -> Option<ModelCost>,
) -> TodayConsumptionDto {
    let floor = period_floor_ms(Period::Today, now_ms);
    let kept: Vec<&ConsumptionRow> = rows
        .iter()
        .filter(|row| floor.is_none_or(|floor| row.call.at >= floor))
        .collect();

    let mut totals = Totals::default();
    for row in &kept {
        totals.add_call(&row.call, price);
    }
    let by_agent = groups(kept.iter().copied(), |call| Some(call.agent.clone()), price)
        .into_iter()
        .map(crate::usage::dto::ConsumptionGroupDto::from)
        .collect();

    TodayConsumptionDto {
        totals: totals.into(),
        by_agent,
        chips: session_chips(&kept, price),
    }
}

/// One chip per `(agent, session)` with an id, newest activity first.
/// Sessions the agents never named stay out — they count in the totals but
/// have no session to enter — and `via_gateway` marks a session whose
/// gateway share is above zero (magpie's chip scoping).
fn session_chips(
    kept: &[&ConsumptionRow],
    price: &dyn Fn(&str, &str) -> Option<ModelCost>,
) -> Vec<SessionChipDto> {
    struct Chip {
        last_active: i64,
        tokens: TokenCounts,
        cost_usd: f64,
        priced: u64,
        via_gateway: bool,
        models: BTreeMap<String, u64>,
    }
    let mut chips: BTreeMap<(String, String), Chip> = BTreeMap::new();
    for row in kept {
        let call = &row.call;
        if call.session.is_empty() {
            continue;
        }
        let chip = chips
            .entry((call.agent.clone(), call.session.clone()))
            .or_insert_with(|| Chip {
                last_active: call.at,
                tokens: TokenCounts::default(),
                cost_usd: 0.0,
                priced: 0,
                via_gateway: false,
                models: BTreeMap::new(),
            });
        chip.last_active = chip.last_active.max(call.at);
        chip.tokens.input += call.tokens.input;
        chip.tokens.output += call.tokens.output;
        chip.tokens.cache_read += call.tokens.cache_read;
        chip.tokens.cache_write += call.tokens.cache_write;
        chip.tokens.reasoning += call.tokens.reasoning;
        if let Some(cost) = price(&call.catalog, served_model(call)) {
            chip.cost_usd += cost.cost(&call.tokens);
            chip.priced += 1;
        }
        chip.via_gateway |= row.source == super::CallSource::Gateway;
        if let Some(model) = non_empty(served_model(call)) {
            *chip.models.entry(model).or_default() += 1;
        }
    }

    let mut chips: Vec<SessionChipDto> = chips
        .into_iter()
        .map(|((agent, session), chip)| SessionChipDto {
            agent,
            session,
            // The dominant served model, ties to the lexicographically last
            // (BTreeMap order) so the label is a function of the rows alone.
            title: chip
                .models
                .iter()
                .max_by_key(|(model, calls)| (**calls, *model))
                .map(|(model, _)| model.clone()),
            last_active: chip.last_active,
            tokens: ConsumptionTokensDto::from(chip.tokens),
            cost_usd: (chip.priced > 0).then_some(chip.cost_usd),
            via_gateway: chip.via_gateway,
        })
        .collect();
    // Newest activity first; the agent/session pair breaks ties so the
    // order is a function of the rows alone.
    chips.sort_by(|left, right| {
        right
            .last_active
            .cmp(&left.last_active)
            .then_with(|| (&left.agent, &left.session).cmp(&(&right.agent, &right.session)))
    });
    chips
}

/// One model ref's candidates compared over `records`. Candidates with the
/// same id keep the first (the resolve seam can spell one row twice when a
/// group holds two refs it both serve); the output order is `route_smart`'s
/// — room first, then unknown, then used up — with the caller's order kept
/// inside each band.
pub fn route_comparison(
    model_ref: &str,
    records: &[Record],
    candidates: &[CandidateFact],
    price: &dyn Fn(&str, &str) -> Option<ModelCost>,
) -> RouteComparisonDto {
    // Deduplicate by candidate id, keeping first occurrence.
    let mut seen = std::collections::BTreeSet::new();
    let unique: Vec<&CandidateFact> = candidates
        .iter()
        .filter(|fact| seen.insert(fact.id.clone()))
        .collect();

    // Smart order over the deduplicated facts; allowance exists only when a
    // percent was reported.
    let ranked: Vec<RouteCandidate<'_>> = unique
        .iter()
        .map(|fact| RouteCandidate {
            id: fact.id.as_str(),
            allowance: fact.percent.map(|percent| skillstar_gateway::AllowanceSnapshot {
                percent,
                renews_at: fact.renews_at_ms.map(unix_ms),
            }),
        })
        .collect();
    let order = route_smart(&ranked);
    let mut facts = unique;
    facts.sort_by_key(|fact| order.iter().position(|id| id == &fact.id).unwrap_or(usize::MAX));

    let models = compared_models(model_ref);
    RouteComparisonDto {
        model: model_ref.to_string(),
        candidates: facts
            .into_iter()
            .map(|fact| route_cost(fact, records, &models, price))
            .collect(),
    }
}

/// The model ids a record must serve to join this comparison: the ref's
/// model half, plus every member's half when the ref names a group (the
/// resolve seam expands the same members into candidates).
fn compared_models(model_ref: &str) -> Vec<String> {
    let mut models = vec![model_half(model_ref)];
    if model_ref.trim().starts_with("group/") {
        for member in skillstar_gateway::expand_group(model_ref, &[]) {
            models.push(model_half(&member));
        }
    }
    models
}

/// The model half of a `provider/model` (or bare) ref.
fn model_half(model_ref: &str) -> String {
    model_ref
        .trim()
        .split_once('/')
        .map_or(model_ref.trim().to_string(), |(_, half)| half.trim().to_string())
}

/// One candidate's ledger aggregate: only records attributed to its catalog
/// that served one of the compared models, priced at read time.
fn route_cost(
    fact: &CandidateFact,
    records: &[Record],
    models: &[String],
    price: &dyn Fn(&str, &str) -> Option<ModelCost>,
) -> RouteCostDto {
    let mut calls = 0u64;
    let mut errors = 0u64;
    let mut latencies: Vec<u64> = Vec::new();
    let mut tokens = TokenCounts::default();
    let mut cost_usd = 0.0;
    let mut priced = 0u64;
    for record in records {
        if record.catalog != fact.catalog {
            continue;
        }
        let served = record.served_model();
        if !models.iter().any(|model| same_model(model, served)) {
            continue;
        }
        calls += 1;
        if record.error_kind.is_some() || record.status >= 400 {
            errors += 1;
        }
        latencies.push(record.latency_ms);
        tokens.input += record.tokens.input;
        tokens.output += record.tokens.output;
        tokens.cache_read += record.tokens.cache_read;
        tokens.cache_write += record.tokens.cache_write;
        tokens.reasoning += record.tokens.reasoning;
        if let Some(cost) = price(&record.catalog, served) {
            cost_usd += cost.cost(&record.tokens);
            priced += 1;
        }
    }
    latencies.sort_unstable();
    RouteCostDto {
        catalog: fact.catalog.clone(),
        calls,
        error_rate: if calls == 0 {
            0.0
        } else {
            errors as f64 / calls as f64
        },
        p50_latency_ms: percentile(&latencies, 50),
        p95_latency_ms: percentile(&latencies, 95),
        tokens: ConsumptionTokensDto::from(tokens),
        cost_usd: (priced > 0).then_some(cost_usd),
        resting: fact.resting,
        percent: fact.percent,
        renews_at_ms: fact.renews_at_ms,
    }
}

/// Nearest-rank percentile of the sorted `values` (`0` when empty): the
/// ceil(p·n)-th value, the vocabulary every latency dashboard agrees on.
fn percentile(sorted: &[u64], percent: u64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (sorted.len() as u64)
        .saturating_mul(percent)
        .div_ceil(100)
        .clamp(1, sorted.len() as u64) as usize;
    sorted[rank - 1]
}

/// The model a merged row is grouped and billed under (summarize's rule).
/// An owned copy of `value` unless it is empty.
fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

/// Unix milliseconds as the `SystemTime` the gateway compares against.
fn unix_ms(ms: i64) -> std::time::SystemTime {
    let duration = std::time::Duration::from_millis(ms.unsigned_abs());
    if ms >= 0 {
        std::time::UNIX_EPOCH + duration
    } else {
        std::time::UNIX_EPOCH - duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skillstar_gateway::TokenCounts;
    use skillstar_usage::sessions::SessionCall;

    const NOW: i64 = 1_790_000_000_000;
    const DAY: i64 = 86_400_000;

    fn record(at: i64, catalog: &str, asked: &str, answered: &str, status: u16, latency: u64) -> Record {
        Record {
            at,
            agent: "claude-code".to_string(),
            session: String::new(),
            model_asked: asked.to_string(),
            model_answered: answered.to_string(),
            catalog: catalog.to_string(),
            account: String::new(),
            tokens: TokenCounts {
                input: 100,
                output: 20,
                cache_read: 5,
                cache_write: 0,
                reasoning: 0,
            },
            status,
            latency_ms: latency,
            error_kind: None,
            endpoint: "/v1/chat/completions".to_string(),
        }
    }

    fn session_call(at: i64, agent: &str, session: &str, model: &str) -> SessionCall {
        SessionCall {
            at,
            agent: agent.to_string(),
            session: session.to_string(),
            model_asked: model.to_string(),
            model_answered: String::new(),
            tokens: skillstar_usage::sessions::SessionTokens {
                input: 30,
                output: 7,
                cache_read: 0,
                cache_write: 0,
            },
            effort: None,
            request_id: None,
            error_kind: None,
            latency_ms: Some(900),
            file: std::path::PathBuf::from("/tmp/nowhere.jsonl"),
            from: 0,
            to: 10,
        }
    }

    fn fact(id: &str, catalog: &str, percent: Option<f64>) -> CandidateFact {
        CandidateFact { id: id.to_string(), catalog: catalog.to_string(), percent, renews_at_ms: None, resting: false }
    }

    fn priced(catalog: &str, model: &str) -> Option<ModelCost> {
        (catalog == "deepseek" && model == "deepseek-chat").then_some(ModelCost {
            input: 0.27,
            output: 1.10,
            cache_read: 0.07,
            cache_write: 0.0,
        })
    }

    #[test]
    fn chips_group_by_session_and_mark_the_gateway_share() {
        let rows = consumption_view(
            &[
                Record { session: "s1".to_string(), at: NOW, ..record(NOW, "deepseek", "deepseek-chat", "", 200, 1_000) },
                Record { session: "s1".to_string(), at: NOW - 1_000, ..record(NOW, "deepseek", "deepseek-chat", "", 200, 1_000) },
            ],
            &[session_call(NOW + 2_000, "opencode", "s2", "glm-4.7")],
            Window::all(),
        )
        .rows;
        let dto = today_from_rows(NOW, &rows, &priced);
        assert_eq!(dto.totals.calls, 3, "everything counts: {dto:?}");
        // Newest activity first; the gateway session leads the bypass one.
        assert_eq!(dto.chips.len(), 2);
        assert_eq!(dto.chips[0].session, "s2", "newest first: {dto:?}");
        assert!(!dto.chips[0].via_gateway, "a bypass-only session");
        assert_eq!(dto.chips[0].title.as_deref(), Some("glm-4.7"));
        assert_eq!(dto.chips[0].cost_usd, None, "glm-4.7 is unpriced here");
        assert_eq!(dto.chips[1].session, "s1");
        assert!(dto.chips[1].via_gateway);
        assert_eq!(dto.chips[1].tokens.input, 200, "two records folded: {dto:?}");
        assert!((dto.chips[1].cost_usd.unwrap() - (200.0 * 0.27 + 40.0 * 1.10 + 10.0 * 0.07) / 1e6).abs() < 1e-9);
        assert_eq!(dto.chips[1].last_active, NOW);
        assert_eq!(dto.chips[1].title.as_deref(), Some("deepseek-chat"));
    }

    #[test]
    fn chips_keep_the_utc_today_boundary_and_drop_idless_calls() {
        let day = NOW.div_euclid(DAY) * DAY;
        let mut idless = record(day + 100, "deepseek", "deepseek-chat", "", 200, 1);
        idless.session = String::new();
        let rows = consumption_view(
            &[record(day, "deepseek", "deepseek-chat", "", 200, 1), idless],
            &[session_call(NOW, "opencode", "s9", "glm-4.7"), session_call(NOW - 2 * DAY, "opencode", "s8", "glm-4.7")],
            Window::all(),
        )
        .rows;
        let dto = today_from_rows(NOW, &rows, &|_, _| None);
        assert_eq!(dto.totals.calls, 3, "yesterday's call is out, idless is in");
        assert_eq!(dto.chips.len(), 1, "idless calls have no chip: {dto:?}");
        assert_eq!(dto.chips[0].session, "s9");
    }

    #[test]
    fn route_costs_share_one_ledger_scope_and_order_by_room() {
        let records = [
            record(NOW, "deepseek", "deepseek-chat", "", 200, 100),
            record(NOW, "deepseek", "deepseek-chat", "deepseek-chat-2025-08-07", 200, 200),
            record(NOW, "deepseek", "deepseek-chat", "", 429, 300),
            record(NOW, "deepseek", "glm-4.7", "", 200, 999), // other model: out
            record(NOW, "relay", "deepseek-chat", "", 200, 400),
            record(NOW, "relay", "deepseek-chat", "", 500, 500),
        ];
        let mut quota = record(NOW, "deepseek", "deepseek-chat", "", 200, 1);
        quota.error_kind = Some(skillstar_gateway::ErrorKind::Quota);
        let records = [records.as_slice(), &[quota]].concat();

        // deepseek reported 99% (used up), relay 10% (room), fresh unknown.
        let candidates = [fact("deepseek", "deepseek", Some(99.0)), fact("relay", "relay", Some(10.0)), fact("fresh", "fresh", None)];
        let dto = route_comparison("deepseek/deepseek-chat", &records, &candidates, &priced);

        assert_eq!(dto.model, "deepseek/deepseek-chat");
        assert_eq!(
            dto.candidates.iter().map(|c| c.catalog.as_str()).collect::<Vec<_>>(),
            vec!["relay", "fresh", "deepseek"],
            "route_smart: room, unknown, spent: {dto:?}"
        );

        let deepseek = dto.candidates.iter().find(|c| c.catalog == "deepseek").unwrap();
        assert_eq!(deepseek.calls, 4, "the glm call stayed out: {deepseek:?}");
        assert!((deepseek.error_rate - 0.5).abs() < 1e-9, "429 + quota error of 4");
        assert_eq!(deepseek.p50_latency_ms, 100, "nearest rank of [1,100,200,300]: {deepseek:?}");
        assert_eq!(deepseek.p95_latency_ms, 300);
        assert_eq!(deepseek.tokens.input, 400);
        // 3 priced calls at 100/100/100 input, 20/20/20 output, 5/5/5 cache:
        assert!((deepseek.cost_usd.unwrap() - 3.0 * (100.0 * 0.27 + 20.0 * 1.10 + 5.0 * 0.07) / 1e6).abs() < 1e-9);

        let relay = dto.candidates.iter().find(|c| c.catalog == "relay").unwrap();
        assert_eq!(relay.calls, 2);
        assert_eq!(relay.cost_usd, None, "no price table row: unknown, not free");
        assert_eq!(relay.p50_latency_ms, 400);
        assert_eq!(relay.p95_latency_ms, 500);

        let fresh = dto.candidates.iter().find(|c| c.catalog == "fresh").unwrap();
        assert_eq!(fresh.calls, 0);
        assert_eq!(fresh.error_rate, 0.0);
        assert_eq!(fresh.p50_latency_ms, 0);
    }

    #[test]
    fn resting_and_renews_ride_the_facts_and_dupes_collapse() {
        let mut spent = fact("relay", "relay", Some(100.0));
        spent.resting = true;
        spent.renews_at_ms = Some(NOW + 3_600_000);
        let dup = fact("relay", "relay", None);
        let dto = route_comparison("m", &[], &[spent.clone(), dup], &|_, _| None);
        assert_eq!(dto.candidates.len(), 1, "same id spelled twice: one seat");
        assert!(dto.candidates[0].resting);
        assert_eq!(dto.candidates[0].percent, Some(100.0));
        assert_eq!(dto.candidates[0].renews_at_ms, Some(NOW + 3_600_000));
    }
}
