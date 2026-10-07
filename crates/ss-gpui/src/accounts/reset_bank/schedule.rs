//! The reset window as data: which weekly credit expires when, how much is
//! left, and when the card should blink. No GPUI types here — the view and
//! the clock both consume these values.

use std::time::Duration;

use chrono::{Datelike, Local, TimeZone, Timelike};
use ss_usage::subscription::{CreditInfo, ResetWindow};

const DAY_SECS: i64 = 24 * 60 * 60;
const HOUR_SECS: i64 = 60 * 60;
const COUNTDOWN_SECS: i64 = 10 * 60;

pub(super) struct ResetBank {
    pub(super) known: bool,
    pub(super) fallback_count: i64,
    pub(super) expiries: Vec<i64>,
}

pub(super) struct ResetView {
    pub(super) known: bool,
    pub(super) count: i64,
    pub(super) expiries: Vec<i64>,
    pub(super) earliest: Option<i64>,
    pub(super) left_secs: i64,
    pub(super) urgent: bool,
    pub(super) countdown: bool,
}

pub(super) fn parse_i64(text: &str) -> Option<i64> {
    text.trim().parse::<i64>().ok()
}

pub(super) fn read_reset_bank(
    credits: &[CreditInfo],
    catalog: &str,
    window: ResetWindow,
) -> ResetBank {
    let (count_key, card_key) = window.credit_keys(catalog).unwrap_or(("", ""));
    let mut saw_count = false;
    let mut saw_card = false;
    let mut fallback_count = 0i64;
    let mut expiries = Vec::new();
    for credit in credits {
        if credit.credit_type == count_key {
            saw_count = true;
            fallback_count = credit
                .credit_amount
                .as_deref()
                .and_then(parse_i64)
                .unwrap_or(0)
                .max(0);
        } else if credit.credit_type == card_key {
            saw_card = true;
            if let Some(stamp) = credit
                .credit_amount
                .as_deref()
                .and_then(parse_i64)
                .filter(|stamp| *stamp >= 0)
            {
                expiries.push(stamp);
            }
        }
    }
    expiries.sort_unstable_by_key(|stamp| if *stamp == 0 { i64::MAX } else { *stamp });
    ResetBank {
        known: saw_count || saw_card,
        fallback_count,
        expiries,
    }
}

/// Live cards win over the aggregate count so an expiry can drop on the client.
pub(super) fn reset_view(bank: &ResetBank, now: i64) -> ResetView {
    let (count, expiries) = if bank.expiries.is_empty() {
        (bank.fallback_count.max(0), Vec::new())
    } else {
        let live: Vec<i64> = bank
            .expiries
            .iter()
            .copied()
            .filter(|stamp| *stamp == 0 || *stamp > now)
            .collect();
        (live.len() as i64, live)
    };
    let earliest = expiries.iter().copied().find(|stamp| *stamp > 0);
    let left_secs = earliest.map(|stamp| stamp - now).unwrap_or(i64::MAX);
    let urgent = earliest.is_some() && left_secs < DAY_SECS;
    let countdown = urgent && left_secs > 0 && left_secs <= COUNTDOWN_SECS;
    ResetView {
        known: bank.known,
        count,
        expiries,
        earliest,
        left_secs,
        urgent,
        countdown,
    }
}

pub(super) fn format_countdown(left_secs: i64) -> String {
    let total = left_secs.max(0);
    format!("{}:{:02}", total / 60, total % 60)
}

pub(super) fn format_stamp(epoch: i64) -> String {
    if epoch <= 0 {
        return String::new();
    }
    let Some(local) = Local.timestamp_opt(epoch, 0).single() else {
        return String::new();
    };
    let month = local.month().to_string();
    let day = local.day().to_string();
    let hour = format!("{:02}", local.hour());
    let minute = format!("{:02}", local.minute());
    crate::i18n::tf(
        "usage.resetCardStamp",
        &[
            ("month", &month),
            ("day", &day),
            ("hour", &hour),
            ("minute", &minute),
        ],
    )
    .to_string()
}

pub(super) fn expiry_line(stamp: &str) -> String {
    if stamp.is_empty() {
        crate::i18n::t("usage.resetCardNoExpiry").to_string()
    } else {
        crate::i18n::tf("usage.resetCardExpiresAt", &[("when", stamp)]).to_string()
    }
}

pub(super) fn reset_subtitle(view: &ResetView, stamp: &str) -> String {
    if !view.known {
        return String::new();
    }
    if view.count == 0 {
        return crate::i18n::t("usage.resetCardsEmpty").to_string();
    }
    if view.countdown {
        let time = format_countdown(view.left_secs);
        return crate::i18n::tf("usage.resetCardExpiresIn", &[("time", &time)]).to_string();
    }
    if stamp.is_empty() {
        return crate::i18n::t("usage.resetCardNoExpiry").to_string();
    }
    if view.count == 1 {
        crate::i18n::tf("usage.resetCardExpiresAt", &[("when", stamp)]).to_string()
    } else {
        crate::i18n::tf("usage.resetCardEarliest", &[("when", stamp)]).to_string()
    }
}

/// Blink period: ~1600ms at 60 minutes, tightening to ~400ms at zero.
pub(super) fn blink_ms(left: i64) -> Option<u64> {
    if left <= 0 || left >= HOUR_SECS {
        return None;
    }
    let fraction = 1.0 - (left as f64) / (HOUR_SECS as f64);
    Some((1600.0 - 1200.0 * fraction * fraction).round() as u64)
}

pub(super) fn clock_delay_for(left: Option<i64>) -> Option<Duration> {
    let left = left?;
    if left > DAY_SECS {
        Some(Duration::from_secs(((left - DAY_SECS) as u64).min(60)))
    } else if left <= COUNTDOWN_SECS {
        Some(Duration::from_secs(1))
    } else if left <= HOUR_SECS {
        Some(Duration::from_secs(5))
    } else {
        Some(Duration::from_secs(15))
    }
}
#[cfg(test)]
mod tests {
    use super::{
        COUNTDOWN_SECS, DAY_SECS, HOUR_SECS, blink_ms, clock_delay_for, format_countdown,
        format_stamp, reset_subtitle, reset_view,
    };
    use ss_usage::subscription::{CreditInfo, GROK_RESET_CARD, GROK_RESET_CREDITS};
    use std::time::Duration;

    fn read_reset_bank(credits: &[CreditInfo]) -> super::ResetBank {
        super::read_reset_bank(credits, "xai", ss_usage::subscription::ResetWindow::Weekly)
    }

    fn credit(kind: &str, amount: &str) -> CreditInfo {
        CreditInfo {
            credit_type: kind.to_string(),
            credit_amount: Some(amount.to_string()),
            minimum_credit_amount_for_usage: None,
        }
    }

    #[test]
    fn cards_sort_earliest_and_drop_expired() {
        let bank = read_reset_bank(&[
            credit(GROK_RESET_CARD, "200"),
            credit(GROK_RESET_CARD, "50"),
            credit(GROK_RESET_CARD, "100"),
            credit(GROK_RESET_CREDITS, "9"),
        ]);
        assert_eq!(bank.expiries, vec![50, 100, 200]);
        let view = reset_view(&bank, 50);
        assert!(view.known);
        assert_eq!(view.count, 2);
        assert_eq!(view.expiries, vec![100, 200]);
        assert_eq!(view.earliest, Some(100));
    }

    #[test]
    fn glm_windows_are_independent_and_unknown_expiry_still_counts() {
        use ss_usage::subscription::ResetWindow;
        let credits = [
            credit("glm-five-reset-card", "0"),
            credit("glm-five-reset-card", "50"),
            credit("glm-week-reset-card", "200"),
        ];
        let five = reset_view(
            &super::read_reset_bank(&credits, "zcode", ResetWindow::FiveHour),
            100,
        );
        let week = reset_view(
            &super::read_reset_bank(&credits, "zcode", ResetWindow::Weekly),
            100,
        );
        assert_eq!(five.count, 1);
        assert_eq!(five.earliest, None);
        assert_eq!(week.count, 1);
        assert_eq!(week.earliest, Some(200));
    }

    #[test]
    fn missing_record_stays_unknown() {
        let view = reset_view(&read_reset_bank(&[]), 0);
        assert!(!view.known);
        assert_eq!(reset_subtitle(&view, ""), "");
    }

    #[test]
    fn explicit_zero_is_empty() {
        let view = reset_view(&read_reset_bank(&[credit(GROK_RESET_CREDITS, "0")]), 0);
        assert!(view.known);
        assert_eq!(view.count, 0);
        assert_eq!(reset_subtitle(&view, ""), "暂无可用");
    }

    #[test]
    fn count_only_has_no_stamp() {
        let view = reset_view(&read_reset_bank(&[credit(GROK_RESET_CREDITS, "3")]), 0);
        assert_eq!(view.count, 3);
        assert!(view.expiries.is_empty());
        assert_eq!(reset_subtitle(&view, ""), "未注明过期时间");
    }

    #[test]
    fn subtitle_uses_the_injected_stamp() {
        let many = reset_view(
            &read_reset_bank(&[
                credit(GROK_RESET_CARD, "200000"),
                credit(GROK_RESET_CARD, "300000"),
            ]),
            0,
        );
        assert_eq!(
            reset_subtitle(&many, "10月6日 12:00"),
            "最早 10月6日 12:00 过期"
        );
        let one = reset_view(&read_reset_bank(&[credit(GROK_RESET_CARD, "200000")]), 0);
        assert_eq!(one.count, 1);
        assert_eq!(reset_subtitle(&one, "10月6日 12:00"), "10月6日 12:00 过期");
        let lang = crate::i18n::set_language_for_test("en");
        assert_eq!(reset_subtitle(&one, "10/6 12:00"), "Expires 10/6 12:00");
        assert_eq!(reset_subtitle(&many, "10/6 12:00"), "Earliest 10/6 12:00");
        assert_eq!(super::expiry_line(""), "No expiry time");
        lang.set("zh-CN");
        assert_eq!(super::expiry_line("10月6日 12:00"), "10月6日 12:00 过期");
    }

    #[test]
    fn countdown_and_blink() {
        let view = reset_view(&read_reset_bank(&[credit(GROK_RESET_CARD, "90")]), 30);
        assert!(view.countdown);
        assert_eq!(reset_subtitle(&view, "ignored"), "最早 1:00 后过期");
        assert_eq!(format_countdown(65), "1:05");
        assert_eq!(blink_ms(HOUR_SECS), None);
        assert_eq!(blink_ms(0), None);
        assert!(blink_ms(HOUR_SECS - 1).unwrap() > blink_ms(60).unwrap());
        assert_eq!(format_stamp(0), "");
    }

    #[test]
    fn clock_slows_as_expiry_recedes() {
        assert_eq!(clock_delay_for(None), None);
        assert_eq!(
            clock_delay_for(Some(DAY_SECS + 1)),
            Some(Duration::from_secs(1))
        );
        assert_eq!(
            clock_delay_for(Some(COUNTDOWN_SECS)),
            Some(Duration::from_secs(1))
        );
        assert_eq!(
            clock_delay_for(Some(HOUR_SECS)),
            Some(Duration::from_secs(5))
        );
        assert_eq!(
            clock_delay_for(Some(DAY_SECS)),
            Some(Duration::from_secs(15))
        );
    }
}
