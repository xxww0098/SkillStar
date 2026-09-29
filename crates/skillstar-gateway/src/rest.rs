//! How long a failed upstream sits out, and which seat the next plan may ask.
//!
//! The clock, the allowance share, and the in-a-row count are injected.
//! Nothing here reads Usage or opens a socket. A rate-limit 429 is not the
//! 15-minute quota rest. A quota reset named by the body or
//! `X-Skillstar-Resets-At` can run past an hour, up to 8 days. `Retry-After`
//! stops at one hour.

use std::sync::LazyLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use regex::bytes::Regex;
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::route::USED_SHARE;

/// Out of credit, until someone tops it up.
pub const CREDIT_REST: Duration = Duration::from_secs(30 * 60);
/// Out of quota, when the refusal does not say when it resets.
pub const QUOTA_REST: Duration = Duration::from_secs(15 * 60);
/// The longest a vendor's own "try again at" is trusted.
pub const LONGEST_WAIT: Duration = Duration::from_secs(60 * 60);
/// The longest a quota rest runs when the refusal says when it is back.
pub const LONGEST_QUOTA: Duration = Duration::from_secs(8 * 24 * 60 * 60);
/// The longest a repeated failure sits out.
pub const LONGEST_RETRY: Duration = Duration::from_secs(10 * 60);
/// An account the vendor wants verified sits out this long.
pub const VERIFY_REST: Duration = Duration::from_secs(30 * 60);
/// How long that verification refusal is answered again without asking.
pub const VERIFY_HOLD: Duration = Duration::from_secs(60);
/// A rate limit with no `Retry-After`, and the first step of a backoff.
pub const FALLBACK_COOLDOWN: Duration = Duration::from_secs(60);
/// Unix seconds, in the future, noted from a quota refusal. Not `X-Magpie-Resets-At`.
pub const RESETS_HEADER: &str = "x-skillstar-resets-at";

/// One failed upstream reply.
pub struct UpstreamFailure<'a> {
    pub status: u16,
    pub body: &'a [u8],
    pub headers: &'a [(&'a str, &'a str)],
    pub now: SystemTime,
    /// Failures in a row, including this one. Zero counts as the first.
    /// A later success is the caller's to forget.
    pub failures: u32,
    /// Injected allowance share, 0 to 100. The renew time applies only at
    /// or above 98, and only when `renews` is still in the future.
    pub used: Option<f64>,
    pub renews: Option<SystemTime>,
}

/// Why a candidate sits out, and until when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rest {
    pub why: &'static str,
    pub by: &'static str,
    pub until: SystemTime,
    /// In-a-row count for a backoff. Zero for every other reason.
    pub failures: u32,
    /// Set for a verification refusal. Replay stops at this instant.
    pub hold: Option<SystemTime>,
    /// HTTPS link the vendor gave for verification. Empty otherwise.
    pub link: String,
}

/// One seat the next plan considers. `until` is `None` when it is not resting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestSeat<'a> {
    pub id: &'a str,
    pub until: Option<SystemTime>,
}

/// Deadline for this failure. `until` is `now` plus the spell.
pub fn rest_after(failure: &UpstreamFailure<'_>) -> Rest {
    let decided = decide(failure);
    Rest {
        why: decided.why,
        by: decided.by,
        until: failure
            .now
            .checked_add(decided.spell)
            .unwrap_or(failure.now),
        failures: decided.failures,
        hold: decided.hold,
        link: decided.link,
    }
}

/// The verification refusal is still the answer to give, without asking again.
/// At the hold instant itself this is false. The 30-minute rest is separate.
pub fn verify_held(rest: &Rest, now: SystemTime) -> bool {
    rest.why == "verify" && rest.hold.is_some_and(|hold| now < hold)
}

/// The next upstream this turn may ask.
///
/// A committed hold already gave the agent a content byte, so the answer is
/// `None` and a second upstream is not called. Seats whose rest is still in
/// the future are dropped. A seat whose deadline equals `now` is eligible.
pub fn next_candidate<'a>(
    committed: bool,
    seats: &'a [RestSeat<'a>],
    now: SystemTime,
) -> Option<&'a str> {
    if committed {
        return None;
    }
    seats
        .iter()
        .find(|seat| seat.until.is_none_or(|until| until <= now))
        .map(|seat| seat.id)
}

struct Decision {
    why: &'static str,
    by: &'static str,
    spell: Duration,
    failures: u32,
    hold: Option<SystemTime>,
    link: String,
}

fn decide(failure: &UpstreamFailure<'_>) -> Decision {
    match kind(failure.status, failure.body) {
        Kind::Credit => Decision {
            why: "credit",
            by: "credit",
            spell: CREDIT_REST,
            failures: 0,
            hold: None,
            link: String::new(),
        },
        Kind::Quota => quota_decision(failure),
        Kind::Rate => rate_decision(failure),
        Kind::Verify { link } => Decision {
            why: "verify",
            by: "verify",
            spell: VERIFY_REST,
            failures: 0,
            hold: failure.now.checked_add(VERIFY_HOLD),
            link,
        },
        Kind::Other => other_decision(failure),
    }
}

fn quota_decision(failure: &UpstreamFailure<'_>) -> Decision {
    let (spell, by) = if let Some(wait) = resets_at(failure.body, failure.now) {
        (wait, "resets")
    } else if let Some(wait) = resets_noted(failure.headers, failure.now) {
        (wait, "resets")
    } else if let Some(wait) = full_window(failure) {
        (wait, "window")
    } else if let Some(wait) = retry_after(failure.headers, failure.now) {
        (wait, "retry-after")
    } else {
        (QUOTA_REST, "quota")
    };
    Decision {
        why: "quota",
        by,
        spell: spell.min(LONGEST_QUOTA),
        failures: 0,
        hold: None,
        link: String::new(),
    }
}

fn rate_decision(failure: &UpstreamFailure<'_>) -> Decision {
    if let Some(wait) = retry_after(failure.headers, failure.now) {
        Decision {
            why: "rate",
            by: "retry-after",
            spell: wait,
            failures: 0,
            hold: None,
            link: String::new(),
        }
    } else {
        Decision {
            why: "rate",
            by: "cooldown",
            spell: FALLBACK_COOLDOWN,
            failures: 0,
            hold: None,
            link: String::new(),
        }
    }
}

fn other_decision(failure: &UpstreamFailure<'_>) -> Decision {
    let (spell, failures) = backoff(failure.failures);
    if let Some(wait) = full_window(failure) {
        Decision {
            why: "other",
            by: "window",
            spell: wait,
            failures,
            hold: None,
            link: String::new(),
        }
    } else {
        Decision {
            why: "other",
            by: "backoff",
            spell,
            failures,
            hold: None,
            link: String::new(),
        }
    }
}

enum Kind {
    Credit,
    Quota,
    Rate,
    Verify { link: String },
    Other,
}

/// Status gates the word lists. Credit words on a 429 are not credit unless
/// the body contains `insufficient_quota`. A 429 that is a rate limit is not
/// quota, unless the plan's own allowance is what ran out.
fn kind(status: u16, body: &[u8]) -> Kind {
    if matches!(status, 401 | 403)
        && let Some(link) = verification(body)
    {
        return Kind::Verify { link };
    }
    let lists = lists();
    let insufficient_quota = contains_slice(body, b"insufficient_quota");
    if status == 402 || (lists.credit.is_match(body) && status != 429) || insufficient_quota {
        return Kind::Credit;
    }
    let planned = lists.planned.is_match(body);
    if status == 429 && lists.rate.is_match(body) && !planned {
        return Kind::Rate;
    }
    if lists.used_up.is_match(body) || (status == 429 && planned) {
        return Kind::Quota;
    }
    if status == 429 {
        return Kind::Rate;
    }
    Kind::Other
}

fn backoff(failures: u32) -> (Duration, u32) {
    let n = failures.max(1);
    let shift = n.saturating_sub(1).min(10);
    let scaled = FALLBACK_COOLDOWN.saturating_mul(1_u32 << shift);
    (scaled.min(LONGEST_RETRY), n)
}

fn full_window(failure: &UpstreamFailure<'_>) -> Option<Duration> {
    let used = failure.used?;
    if !used.is_finite() || used < USED_SHARE {
        return None;
    }
    let renews = failure.renews?;
    let spell = renews.duration_since(failure.now).ok()?;
    if spell.is_zero() { None } else { Some(spell) }
}

fn resets_at(body: &[u8], now: SystemTime) -> Option<Duration> {
    if let Some(wait) = pipe_reset(body, now) {
        return Some(wait);
    }
    resets_in(body, now)
}

fn pipe_reset(body: &[u8], now: SystemTime) -> Option<Duration> {
    let caps = lists().resets.captures(body)?;
    let digits = std::str::from_utf8(caps.get(1)?.as_bytes()).ok()?;
    future_unix(digits.parse().ok()?, now)
}

fn resets_in(body: &[u8], now: SystemTime) -> Option<Duration> {
    let parsed: ResetsDoc = serde_json::from_slice(body).ok()?;
    let error = parsed.error?;
    if error.resets_at > 0
        && let Some(wait) = future_unix(error.resets_at, now)
    {
        return Some(wait);
    }
    if error.resets_in_seconds > 0 {
        u64::try_from(error.resets_in_seconds)
            .ok()
            .map(Duration::from_secs)
    } else {
        None
    }
}

fn resets_noted(headers: &[(&str, &str)], now: SystemTime) -> Option<Duration> {
    let value = header(headers, RESETS_HEADER)?;
    if value.is_empty() {
        return None;
    }
    future_unix(value.parse().ok()?, now)
}

fn retry_after(headers: &[(&str, &str)], now: SystemTime) -> Option<Duration> {
    let mut wait = None;
    if let Some(value) = header(headers, "retry-after")
        && !value.is_empty()
    {
        match parse_retry_after(value, now) {
            Parsed::Wait(spell) => wait = Some(spell),
            Parsed::NonPositive | Parsed::Absent => {}
        }
    }
    if wait.is_none() {
        for (name, value) in headers {
            let lower = name.to_ascii_lowercase();
            if value.is_empty() || !lower.contains("reset") || !lower.contains("ratelimit") {
                continue;
            }
            if let Some(spell) = parse_reset_value(value, now) {
                wait = Some(spell);
                break;
            }
        }
    }
    wait.map(|spell| spell.min(LONGEST_WAIT))
        .filter(|spell| !spell.is_zero())
}

fn verification(body: &[u8]) -> Option<String> {
    let mut ok = false;
    let mut link = String::new();
    let mut help = String::new();
    if let Ok(parsed) = serde_json::from_slice::<VerifyDoc>(body)
        && let Some(error) = parsed.error
    {
        for detail in error.details {
            if detail.reason == "VALIDATION_REQUIRED" {
                ok = true;
                if link.is_empty() {
                    link = meta_url(&detail.metadata, "validation_url")
                        .or_else(|| meta_url(&detail.metadata, "validationUrl"))
                        .unwrap_or_default();
                }
            }
            if help.is_empty()
                && let Some(url) = detail.links.iter().find(|item| !item.url.is_empty())
            {
                help = url.url.clone();
            }
        }
    }
    if ok && link.is_empty() {
        link = help;
    }
    if !ok && !lists().verify.is_match(body) {
        return None;
    }
    if link.is_empty()
        && let Some(found) = linked_verify(body)
    {
        link = found;
    }
    if !https_link(&link) {
        link.clear();
    }
    Some(link)
}

fn meta_url(metadata: &Map<String, Value>, key: &str) -> Option<String> {
    let value = metadata.get(key)?.as_str()?;
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn linked_verify(body: &[u8]) -> Option<String> {
    let caps = lists().verify_link.captures(body)?;
    let text = std::str::from_utf8(caps.get(1)?.as_bytes()).ok()?;
    Some(text.to_string())
}

fn https_link(link: &str) -> bool {
    let Some(rest) = link.strip_prefix("https://") else {
        return false;
    };
    match rest.split(['/', '?', '#']).next() {
        Some(host) => !host.is_empty(),
        None => false,
    }
}

fn contains_slice(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn header<'a>(headers: &'a [(&'a str, &'a str)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| *value)
}

enum Parsed {
    Absent,
    NonPositive,
    Wait(Duration),
}

fn parse_retry_after(value: &str, now: SystemTime) -> Parsed {
    if let Ok(seconds) = value.parse::<f64>() {
        return wait_from_seconds(seconds);
    }
    match imf_fixdate(value) {
        Some(at) => match at.duration_since(now) {
            Ok(spell) if !spell.is_zero() => Parsed::Wait(spell),
            _ => Parsed::NonPositive,
        },
        None => Parsed::Absent,
    }
}

fn parse_reset_value(value: &str, now: SystemTime) -> Option<Duration> {
    if let Some(at) = rfc3339(value) {
        return at.duration_since(now).ok().filter(|spell| !spell.is_zero());
    }
    if let Some(spell) = go_duration(value) {
        return Some(spell);
    }
    let secs: i64 = value.parse().ok()?;
    if secs > 1_000_000_000 {
        future_unix(secs, now)
    } else {
        None
    }
}

fn wait_from_seconds(seconds: f64) -> Parsed {
    if !seconds.is_finite() {
        return Parsed::Absent;
    }
    if seconds <= 0.0 {
        return Parsed::NonPositive;
    }
    let nanos = seconds * 1_000_000_000.0;
    if !nanos.is_finite() || nanos >= u64::MAX as f64 {
        return Parsed::Wait(LONGEST_WAIT);
    }
    let spell = Duration::from_nanos(nanos as u64);
    if spell.is_zero() {
        Parsed::NonPositive
    } else {
        Parsed::Wait(spell)
    }
}

fn future_unix(secs: i64, now: SystemTime) -> Option<Duration> {
    let at = unix_time(secs)?;
    at.duration_since(now).ok().filter(|spell| !spell.is_zero())
}

fn unix_time(secs: i64) -> Option<SystemTime> {
    if secs >= 0 {
        UNIX_EPOCH.checked_add(Duration::from_secs(u64::try_from(secs).ok()?))
    } else {
        UNIX_EPOCH.checked_sub(Duration::from_secs(secs.unsigned_abs()))
    }
}

fn go_duration(input: &str) -> Option<Duration> {
    let (negative, mut left) = if let Some(rest) = input.strip_prefix('-') {
        (true, rest)
    } else if let Some(rest) = input.strip_prefix('+') {
        (false, rest)
    } else {
        (false, input)
    };
    if left.is_empty() {
        return None;
    }
    let mut total = 0.0_f64;
    let mut saw = false;
    while !left.is_empty() {
        let (number, next) = take_number(left)?;
        let (unit, next) = take_unit(next)?;
        total += number * unit_seconds(unit);
        left = next;
        saw = true;
    }
    if !saw || negative {
        return None;
    }
    match wait_from_seconds(total) {
        Parsed::Wait(spell) => Some(spell),
        Parsed::Absent | Parsed::NonPositive => None,
    }
}

fn take_number(input: &str) -> Option<(f64, &str)> {
    let bytes = input.as_bytes();
    let mut index = 0;
    let mut digits = 0;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
        digits += 1;
    }
    if index < bytes.len() && bytes[index] == b'.' {
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    let number = input[..index].parse().ok()?;
    Some((number, &input[index..]))
}

fn take_unit(input: &str) -> Option<(&str, &str)> {
    for unit in ["µs", "μs", "ns", "us", "ms", "s", "m", "h"] {
        if let Some(rest) = input.strip_prefix(unit) {
            return Some((unit, rest));
        }
    }
    None
}

fn unit_seconds(unit: &str) -> f64 {
    match unit {
        "ns" => 1e-9,
        "us" | "µs" | "μs" => 1e-6,
        "ms" => 1e-3,
        "s" => 1.0,
        "m" => 60.0,
        "h" => 3_600.0,
        _ => 0.0,
    }
}

fn imf_fixdate(value: &str) -> Option<SystemTime> {
    let mut parts = value.split_whitespace();
    let _weekday = parts.next()?;
    let day: u32 = parts.next()?.parse().ok()?;
    let month = month_index(parts.next()?)?;
    let year: i32 = parts.next()?.parse().ok()?;
    let (hour, minute, second) = hms(parts.next()?)?;
    let zone = parts.next()?;
    if parts.next().is_some() || !matches!(zone, "GMT" | "UTC") {
        return None;
    }
    civil_time(year, month, day, hour, minute, second)
}

fn rfc3339(value: &str) -> Option<SystemTime> {
    let bytes = value.as_bytes();
    if bytes.len() < 20 || bytes[10] != b'T' || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year: i32 = value.get(0..4)?.parse().ok()?;
    let month: u32 = value.get(5..7)?.parse().ok()?;
    let day: u32 = value.get(8..10)?.parse().ok()?;
    let (hour, minute, second) = hms(value.get(11..19)?)?;
    let offset = zone_offset(value.get(19..)?)?;
    let local = civil_secs(year, month, day, hour, minute, second)?;
    unix_time(local.checked_sub(offset)?)
}

fn zone_offset(rest: &str) -> Option<i64> {
    if rest == "Z" {
        return Some(0);
    }
    let (sign, rest) = if let Some(rest) = rest.strip_prefix('+') {
        (1_i64, rest)
    } else {
        let rest = rest.strip_prefix('-')?;
        (-1, rest)
    };
    let bytes = rest.as_bytes();
    if bytes.len() != 5 || bytes[2] != b':' {
        return None;
    }
    let hour: i64 = rest.get(0..2)?.parse().ok()?;
    let minute: i64 = rest.get(3..5)?.parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(sign * (hour * 3_600 + minute * 60))
}

fn hms(value: &str) -> Option<(u32, u32, u32)> {
    let bytes = value.as_bytes();
    if bytes.len() != 8 || bytes[2] != b':' || bytes[5] != b':' {
        return None;
    }
    let hour: u32 = value.get(0..2)?.parse().ok()?;
    let minute: u32 = value.get(3..5)?.parse().ok()?;
    let second: u32 = value.get(6..8)?.parse().ok()?;
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    Some((hour, minute, second))
}

fn civil_time(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> Option<SystemTime> {
    unix_time(civil_secs(year, month, day, hour, minute, second)?)
}

fn civil_secs(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> Option<i64> {
    let days = civil_days(year, month, day)?;
    days.checked_mul(86_400)?
        .checked_add(i64::from(hour) * 3_600 + i64::from(minute) * 60 + i64::from(second))
}

fn civil_days(year: i32, month: u32, day: u32) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let year = i64::from(year) - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let yoe = u64::try_from(year - era * 400).ok()?;
    let month_prime = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * u64::from(month_prime) + 2) / 5 + u64::from(day) - 1;
    let day_of_era = yoe * 365 + yoe / 4 - yoe / 100 + day_of_year;
    Some(era * 146_097 + i64::try_from(day_of_era).ok()? - 719_468)
}

fn month_index(name: &str) -> Option<u32> {
    Some(match name {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    })
}

struct Lists {
    credit: Regex,
    used_up: Regex,
    rate: Regex,
    planned: Regex,
    resets: Regex,
    verify: Regex,
    verify_link: Regex,
}

fn lists() -> &'static Lists {
    static LISTS: LazyLock<Lists> = LazyLock::new(|| Lists {
        credit: must_compile(
            r"(?i)insufficient.?(balance|credit|fund)|balance|credit|billing|payment|arrear|overdue|suspended|余额|欠费|充值|账户.*(不足|停)",
        ),
        used_up: must_compile(
            r"(?i)quota|usage.?limit|limit.?reached|hit your .*limit|limit.{0,24}resets|exceeded.*(plan|limit)|额度|用量|套餐|上限",
        ),
        rate: must_compile(
            r"(?i)rate.?limit|too many requests|per.?(second|sec|minute|min)(?-u:\b)|(?-u:\b)[rt]pm(?-u:\b)|频率|太频繁",
        ),
        planned: must_compile(
            r"(?i)quota|usage.?limit|hit your .*limit|limit.{0,24}resets|per.?(day|week|month)|daily|weekly|monthly|额度|用量|套餐",
        ),
        resets: must_compile(r"(?i)limit reached\|(\d{10})(?-u:\b)"),
        verify: must_compile(
            r"(?i)verify your account|account verification required|this Google account needs to be verified",
        ),
        verify_link: must_compile(
            r#"this Google account needs to be verified: open (https://[^\s"\\]+) in a browser"#,
        ),
    });
    &LISTS
}

fn must_compile(pattern: &str) -> Regex {
    Regex::new(pattern).expect("rest word list")
}

#[derive(Deserialize)]
struct ResetsDoc {
    #[serde(default)]
    error: Option<ResetsError>,
}

#[derive(Deserialize)]
struct ResetsError {
    #[serde(default)]
    resets_at: i64,
    #[serde(default)]
    resets_in_seconds: i64,
}

#[derive(Deserialize)]
struct VerifyDoc {
    #[serde(default)]
    error: Option<VerifyError>,
}

#[derive(Deserialize)]
struct VerifyError {
    #[serde(default)]
    details: Vec<VerifyDetail>,
}

#[derive(Deserialize)]
struct VerifyDetail {
    #[serde(default)]
    reason: String,
    #[serde(default)]
    metadata: Map<String, Value>,
    #[serde(default)]
    links: Vec<VerifyLink>,
}

#[derive(Deserialize)]
struct VerifyLink {
    #[serde(default)]
    url: String,
}
