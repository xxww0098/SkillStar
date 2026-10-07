//! Cross-view projections over already-assembled data (spec slice 13).
//!
//! [`today_consumption`] — the consumption view's today (UTC day, the same
//! contract [`summarize`](super::summarize) pins) with one chip per
//! session: the Usage page's drill-down from a card into the agent that
//! ran it.
//!
//! Everything here is pure: rows and the price table arrive as arguments,
//! and the clock reaches [`today_consumption`] as `now_ms`. The service
//! layer (`service/summary.rs`) owns the reads.

use std::collections::BTreeMap;

use crate::pricing::ModelCost;
use crate::sessions::SessionCall;

use super::summarize::{Period, Totals, served_model};
use super::{UnifiedCall, consumption_view, groups, period_floor_ms};
use crate::accounts::dto::{
    ConsumptionGroupDto, ConsumptionTokensDto, SessionChipDto, TodayConsumptionDto,
};

/// Today's consumption with session chips. Rows are the consumption view
/// over the raw calls; the day boundary is UTC, floor inclusive — the same
/// contract as [`summarize`](super::summarize::summarize).
pub fn today_consumption(
    now_ms: i64,
    calls: &[SessionCall],
    price: &dyn Fn(&str) -> Option<ModelCost>,
) -> TodayConsumptionDto {
    today_from_rows(now_ms, &consumption_view(calls), price)
}

/// The chips-and-totals projection over already-projected rows (the pure
/// seam tests drive; `today_consumption` is its read-side wrapper).
pub fn today_from_rows(
    now_ms: i64,
    rows: &[UnifiedCall],
    price: &dyn Fn(&str) -> Option<ModelCost>,
) -> TodayConsumptionDto {
    let floor = period_floor_ms(Period::Today, now_ms);
    let kept: Vec<&UnifiedCall> = rows
        .iter()
        .filter(|call| floor.is_none_or(|floor| call.at >= floor))
        .collect();

    let mut totals = Totals::default();
    for call in &kept {
        totals.add_call(call, price);
    }
    let by_agent = groups(kept.iter().copied(), |call| Some(call.agent.clone()), price)
        .into_iter()
        .map(ConsumptionGroupDto::from)
        .collect();

    TodayConsumptionDto {
        totals: totals.into(),
        by_agent,
        chips: session_chips(&kept, price),
    }
}

/// One chip per `(agent, session)` with an id, newest activity first.
/// Sessions the agents never named stay out — they count in the totals but
/// have no session to enter.
fn session_chips(
    kept: &[&UnifiedCall],
    price: &dyn Fn(&str) -> Option<ModelCost>,
) -> Vec<SessionChipDto> {
    struct Chip {
        last_active: i64,
        tokens: crate::pricing::TokenCounts,
        cost_usd: f64,
        priced: u64,
        models: BTreeMap<String, u64>,
    }
    let mut chips: BTreeMap<(String, String), Chip> = BTreeMap::new();
    for call in kept {
        if call.session.is_empty() {
            continue;
        }
        let chip = chips
            .entry((call.agent.clone(), call.session.clone()))
            .or_insert_with(|| Chip {
                last_active: call.at,
                tokens: Default::default(),
                cost_usd: 0.0,
                priced: 0,
                models: BTreeMap::new(),
            });
        chip.last_active = chip.last_active.max(call.at);
        chip.tokens.input += call.tokens.input;
        chip.tokens.output += call.tokens.output;
        chip.tokens.cache_read += call.tokens.cache_read;
        chip.tokens.cache_write += call.tokens.cache_write;
        chip.tokens.reasoning += call.tokens.reasoning;
        if let Some(cost) = price(served_model(call)) {
            chip.cost_usd += cost.cost(&call.tokens);
            chip.priced += 1;
        }
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

/// The model a row is grouped and billed under (summarize's rule).
/// An owned copy of `value` unless it is empty.
fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}
