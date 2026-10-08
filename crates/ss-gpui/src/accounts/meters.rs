//! Remaining-only quota meters.
//!
//! Fill and caption come from remaining percent (`100 - used`). Copy is
//! `usage.remainingPercent`. Count windows show only that caption. A
//! `usd-cents` window prefixes it (`$3.00 / $12.00 · 剩余 75%`). Reset sits
//! on the meter as a relative countdown. Monthly credits keep the caption
//! and omit the bar.

use std::time::{SystemTime, UNIX_EPOCH};

use gpui_kit::*;
use ss_usage::subscription::{UsageUnit, UsageWindow};

use super::theme::{self};
use crate::accounts::theme::palette;

/// Current epoch seconds.
pub fn current_epoch_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Remaining 0–100. `UsageWindow.percent` is used-percent.
pub fn remaining_percent(window: &UsageWindow) -> Option<f32> {
    if let Some(used) = window.percent {
        return Some((100.0 - used as f32).clamp(0.0, 100.0));
    }
    window
        .total
        .filter(|total| *total > 0)
        .map(|total| (100.0 - window.used as f32 / total as f32 * 100.0).clamp(0.0, 100.0))
}

/// `6天12时6分`. Empty when the timestamp is missing.
pub fn format_reset(reset_at_epoch: i64) -> String {
    if reset_at_epoch <= 0 {
        return String::new();
    }
    let delta = reset_at_epoch - current_epoch_seconds();
    if delta <= 0 {
        return crate::i18n::t("usage.meterResetSoon").to_string();
    }
    let total_minutes = ((delta as f64) / 60.0).round().max(1.0) as i64;
    let days = total_minutes / 1440;
    let hours = (total_minutes % 1440) / 60;
    let minutes = total_minutes % 60;
    let mut bits = Vec::new();
    if days > 0 {
        bits.push(part("usage.meterDays", days));
    }
    if hours > 0 {
        bits.push(part("usage.meterHours", hours));
    }
    if minutes > 0 || bits.is_empty() {
        bits.push(part("usage.meterMinutes", minutes));
    }
    let sep = if crate::i18n::language() == "en" {
        " "
    } else {
        ""
    };
    bits.join(sep)
}

fn part(key: &str, n: i64) -> String {
    let n = n.to_string();
    crate::i18n::tf(key, &[("n", &n)]).to_string()
}

fn meter_label(label: &str) -> String {
    let key = match label {
        "模型额度" => "usage.windowModelQuota",
        "Weekly credits" => "usage.windowWeeklyCredits",
        "Monthly credits" => "usage.windowMonthlyCredits",
        "ZCode MCP" => "usage.glmMcp",
        "Included" | "Total" => "usage.includedQuota",
        "Auto + Composer" => "usage.categoryAutoComposer",
        "API" => "usage.categoryApi",
        _ => return localize_quota_phrases(label),
    };
    crate::i18n::t(key).to_string()
}

/// Known Antigravity bucket phrases inside a fetched window label.
fn localize_quota_phrases(label: &str) -> String {
    const PHRASES: &[(&str, &str)] = &[
        ("Five Hour Limit", "usage.antigravityFiveHourLimit"),
        ("Weekly Limit", "usage.antigravityWeeklyLimit"),
    ];
    let mut label = label.to_string();
    for (phrase, key) in PHRASES {
        label = replace_ascii_phrase_ignore_case(&label, phrase, &crate::i18n::t(key));
    }
    label
}

fn replace_ascii_phrase_ignore_case(label: &str, phrase: &str, replacement: &str) -> String {
    let lower_label = label.to_ascii_lowercase();
    let lower_phrase = phrase.to_ascii_lowercase();
    let mut out = String::new();
    let mut rest = label;
    let mut lower_rest = lower_label.as_str();
    while let Some(index) = lower_rest.find(&lower_phrase) {
        out.push_str(&rest[..index]);
        out.push_str(replacement);
        let next = index + phrase.len();
        rest = &rest[next..];
        lower_rest = &lower_rest[next..];
    }
    out.push_str(rest);
    out
}

fn remaining_caption(percent: f32) -> String {
    let percent = format!("{percent:.0}");
    crate::i18n::tf("usage.remainingPercent", &[("percent", &percent)]).to_string()
}

/// US cents as `$3.00`. Negative cents keep the sign.
fn format_usd_cents(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let cents = cents.unsigned_abs();
    format!("{sign}${}.{:02}", cents / 100, cents % 100)
}

fn amount_text(window: &UsageWindow) -> String {
    if window.unit != UsageUnit::UsdCents {
        return String::new();
    }
    match window.total {
        Some(total) => format!(
            "{} / {}",
            format_usd_cents(window.used),
            format_usd_cents(total)
        ),
        None if window.used != 0 => format_usd_cents(window.used),
        None => String::new(),
    }
}

fn meter_value(window: &UsageWindow) -> String {
    let amount = amount_text(window);
    let caption = remaining_percent(window)
        .map(remaining_caption)
        .unwrap_or_default();
    match (amount.is_empty(), caption.is_empty()) {
        (false, false) => format!("{amount} · {caption}"),
        (false, true) => amount,
        (true, false) => caption,
        (true, true) => String::new(),
    }
}

/// Monthly credits already show dollars and remaining percent. A bar would
/// only restate that percent.
fn shows_progress_bar(label: &str) -> bool {
    label != "Monthly credits"
}

/// One remaining meter: label, remaining caption (dollar used/limit when the
/// window is `usd-cents`), reset, and a bar except for monthly credits.
pub fn render_remaining_meter(window: &UsageWindow) -> Div {
    let remaining = remaining_percent(window);
    let tone = remaining
        .map(theme::quota_tone)
        .unwrap_or(palette().os_muted);
    let value = meter_value(window);

    let mut col = div().flex().flex_col().w_full().gap(px(4.0));
    col = col.child(
        div()
            .flex()
            .items_baseline()
            .justify_between()
            .gap(px(10.0))
            .w_full()
            .text_size(px(12.0))
            .child(
                div()
                    .text_color(rgb(palette().os_muted))
                    .child(meter_label(&window.label)),
            )
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(tone))
                    .child(value),
            ),
    );

    if let Some(reset_at) = window.reset_at {
        let reset = format_reset(reset_at);
        if !reset.is_empty() {
            col = col.child(
                div()
                    .w_full()
                    .text_size(px(11.0))
                    .text_color(rgb(palette().os_faint))
                    .text_right()
                    .child(reset),
            );
        }
    }

    if shows_progress_bar(&window.label) {
        if let Some(pct) = remaining {
            let fill = theme::quota_fill(pct);
            col = col.child(
                div()
                    .w_full()
                    .h(px(6.0))
                    .rounded_full()
                    .bg(rgb(palette().os_hair))
                    .overflow_hidden()
                    .child(
                        div()
                            .h_full()
                            .rounded_full()
                            .bg(rgb(fill))
                            .w(relative(pct / 100.0)),
                    ),
            );
        }
    }

    if !window.breakdown.is_empty() {
        let mut notes = div().flex().flex_wrap().gap_1().w_full();
        let mut nested = false;
        for item in &window.breakdown {
            if item.percent.is_some() || item.total.is_some() {
                nested = true;
                break;
            }
            notes = notes.child(
                div()
                    .px(px(5.0))
                    .py(px(2.0))
                    .rounded(px(5.0))
                    .bg(rgb(super::palette().os_fill_2))
                    .text_size(px(10.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(super::palette().os_tag))
                    .child(format!("{} ×{}", item.label, item.used)),
            );
        }
        if nested {
            let mut stack = div().flex().flex_col().gap(px(10.0)).w_full();
            for item in &window.breakdown {
                stack = stack.child(render_remaining_meter(item));
            }
            col = col.child(stack);
        } else {
            col = col.child(notes);
        }
    }

    col
}

#[cfg(test)]
mod tests {
    use super::super::theme::{quota_fill, quota_tone};
    use super::{format_reset, remaining_percent};
    use crate::accounts::theme::palette;
    use ss_usage::subscription::{UsageUnit, UsageWindow};

    fn window(percent: Option<i32>, used: i64, total: Option<i64>) -> UsageWindow {
        UsageWindow {
            label: "每周".into(),
            used,
            total,
            percent,
            reset_at: None,
            breakdown: Vec::new(),
            unit: UsageUnit::Count,
        }
    }

    #[test]
    fn remaining_is_inverse_of_used_percent() {
        assert_eq!(remaining_percent(&window(Some(14), 0, None)), Some(86.0));
        assert_eq!(remaining_percent(&window(None, 25, Some(100))), Some(75.0));
        assert_eq!(remaining_percent(&window(None, 0, None)), None);
    }

    #[test]
    fn quota_tone_reserves_color_for_warnings() {
        assert_eq!(quota_tone(86.0), palette().fg);
        assert_eq!(quota_tone(40.0), palette().os_warn);
        assert_eq!(quota_tone(15.0), palette().os_bad);
    }

    #[test]
    fn quota_fill_ramps_green_to_red() {
        assert_eq!(quota_fill(100.0), palette().os_ok);
        assert_eq!(quota_fill(50.0), palette().os_warn);
        assert_eq!(quota_fill(0.0), palette().os_bad);
    }

    #[test]
    fn past_reset_reads_as_soon() {
        let _lang = crate::i18n::set_language_for_test("zh-CN");
        assert_eq!(format_reset(0), "");
        assert_eq!(format_reset(1), "即将重置");
    }

    #[test]
    fn countdown_uses_compact_units_and_omits_zero_parts() {
        let _lang = crate::i18n::set_language_for_test("zh-CN");
        let now = super::current_epoch_seconds();
        for (minutes, expected) in [
            (6 * 1440 + 22 * 60 + 31, "6天22时31分"),
            (22 * 60 + 31, "22时31分"),
            (31, "31分"),
            (1440, "1天"),
            (60, "1时"),
        ] {
            assert_eq!(format_reset(now + minutes * 60 + 10), expected);
        }
        assert_eq!(format_reset(now + 20), "1分");
    }

    #[test]
    fn countdown_and_caption_follow_the_interface_language() {
        let now = super::current_epoch_seconds();
        let lang = crate::i18n::set_language_for_test("en");
        assert_eq!(format_reset(1), "Resetting soon");
        assert_eq!(
            format_reset(now + (6 * 1440 + 22 * 60 + 31) * 60 + 10),
            "6d 22h 31m"
        );
        assert_eq!(format_reset(now + 31 * 60 + 10), "31m");
        assert_eq!(super::remaining_caption(86.0), "86% left");
        assert_eq!(super::meter_label("模型额度"), "Model quota");
        assert_eq!(super::meter_label("Weekly credits"), "Weekly credits");
        assert_eq!(super::meter_label("Monthly credits"), "Monthly credits");
        assert_eq!(
            super::meter_label("Gemini Models · Weekly Limit"),
            "Gemini Models · Weekly Limit"
        );
        assert_eq!(
            super::meter_label("Claude and GPT models · Five Hour Limit"),
            "Claude and GPT models · Five Hour Limit"
        );
        assert_eq!(super::meter_label("5h"), "5h");
        assert_eq!(super::meter_label("Five Hour Limit"), "Five Hour Limit");
        assert_eq!(super::meter_label("Weekly Limit"), "Weekly Limit");
        assert_eq!(super::meter_label("ZCode MCP"), "ZCode MCP");
        assert_eq!(super::meter_label("Included"), "Included");
        assert_eq!(super::meter_label("Auto + Composer"), "Tab & Composer");
        assert_eq!(super::meter_label("API"), "API calls");
        lang.set("zh-CN");
        assert_eq!(super::remaining_caption(86.0), "剩余 86%");
        assert_eq!(super::meter_label("模型额度"), "模型额度");
        assert_eq!(super::meter_label("Weekly credits"), "每周额度");
        assert_eq!(super::meter_label("Monthly credits"), "每月额度");
        assert_eq!(
            super::meter_label("Gemini Models · weekly limit"),
            "Gemini Models · 周额度"
        );
        assert_eq!(
            super::meter_label("Claude and GPT models · Five Hour Limit"),
            "Claude and GPT models · 5 小时额度"
        );
        assert_eq!(super::meter_label("5h"), "5h");
        assert_eq!(super::meter_label("Five Hour Limit"), "5 小时额度");
        assert_eq!(super::meter_label("Weekly Limit"), "周额度");
        assert_eq!(super::meter_label("ZCode MCP"), "ZCode MCP");
        assert_eq!(super::meter_label("Included"), "包含额度");
        assert_eq!(super::meter_label("Total"), "包含额度");
        assert_eq!(super::meter_label("Auto + Composer"), "补全 & Composer");
        assert_eq!(super::meter_label("API"), "API 调用");
    }

    #[test]
    fn usd_cents_join_the_remaining_percent() {
        let lang = crate::i18n::set_language_for_test("zh-CN");
        let mut window = window(Some(25), 300, Some(1_200));
        window.unit = UsageUnit::UsdCents;
        assert_eq!(super::meter_value(&window), "$3.00 / $12.00 · 剩余 75%");
        lang.set("en");
        assert_eq!(super::meter_value(&window), "$3.00 / $12.00 · 75% left");
        let mut one_dollar = window;
        one_dollar.percent = Some(100);
        one_dollar.used = 100;
        one_dollar.total = Some(100);
        assert_eq!(super::amount_text(&one_dollar), "$1.00 / $1.00");
        drop(lang);
    }

    #[test]
    fn monthly_credits_omit_the_progress_bar() {
        assert!(!super::shows_progress_bar("Monthly credits"));
        assert!(super::shows_progress_bar("Weekly credits"));
        assert!(super::shows_progress_bar("每周"));
    }

    #[test]
    fn count_windows_show_only_the_remaining_caption() {
        let _lang = crate::i18n::set_language_for_test("zh-CN");
        assert_eq!(super::amount_text(&window(Some(13), 13, Some(100))), "");
        assert_eq!(super::amount_text(&window(Some(0), 0, Some(100))), "");
        assert_eq!(
            super::amount_text(&window(Some(38), 54_300, Some(140_000))),
            ""
        );
        assert_eq!(super::amount_text(&window(None, 13, None)), "");
        assert_eq!(
            super::meter_value(&window(Some(0), 0, Some(100_000_000))),
            "剩余 100%"
        );
        assert_eq!(
            super::meter_value(&window(Some(38), 54_300, Some(140_000))),
            "剩余 62%"
        );
    }
}
