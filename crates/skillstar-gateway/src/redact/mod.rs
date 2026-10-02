//! Mask secrets before a body leaves the machine, and put them back in the reply.
//!
//! Switches live at the top of `config_dir()/model_gateway.json`: `redact`,
//! `redact_personal`, `redact_words`, `redact_rules`, plus `redact_word_list`
//! and `redact_rule_list`. Missing file, unreadable file, or any value other
//! than JSON `true` leaves the body untouched and does not create a key.
//! A placeholder is written only when some switch actually matches.

mod json;
mod rules;

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::{Captures, Regex};
use serde_json::Value;

use crate::store::doc::ModelGatewayDoc;

const MAX_VALUES: usize = 200_000;
const MAX_REGEX: usize = 300;
const MIN_CUSTOM: usize = 4;
const B32: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

struct Memory {
    values: HashMap<String, String>,
    key: Option<[u8; 32]>,
    key_path: PathBuf,
}

static MEMORY: LazyLock<Mutex<Memory>> = LazyLock::new(|| {
    Mutex::new(Memory {
        values: HashMap::new(),
        key: None,
        key_path: PathBuf::new(),
    })
});

static PLACEHOLDER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{\{[A-Z][A-Z0-9_]*_[a-z2-7]{8}\}\}").expect("placeholder pattern")
});

struct UserRule {
    kind: String,
    prefix: String,
    regex: String,
}

struct LiveRule {
    kind: String,
    re: Regex,
    markers: Vec<String>,
}

struct Opts {
    secrets: bool,
    personal: bool,
    words_on: bool,
    rules_on: bool,
    words: Vec<String>,
    rules: Vec<UserRule>,
}

struct Span {
    start: usize,
    end: usize,
    kind: String,
}

/// Replace configured secrets in `body` with placeholders.
///
/// The bytes come back unchanged when redaction is off or nothing matches.
/// [`unmask_response`] restores the values this call replaced.
pub fn mask_outbound(body: &[u8]) -> Vec<u8> {
    let opts = settings();
    if !opts.active() {
        return body.to_vec();
    }
    match walk_mask(body, &opts) {
        json::Walked::Changed(bytes) => bytes,
        json::Walked::Same => body.to_vec(),
        json::Walked::NotJson => mask_raw(body, &opts),
    }
}

/// Put placeholders from [`mask_outbound`] back into a response body.
pub fn unmask_response(body: &[u8]) -> Vec<u8> {
    if !body.windows(2).any(|pair| pair == b"{{") {
        return body.to_vec();
    }
    match walk_unmask(body) {
        json::Walked::Changed(bytes) => bytes,
        json::Walked::Same => body.to_vec(),
        json::Walked::NotJson => unmask_raw(body),
    }
}

fn settings() -> Opts {
    let doc = ModelGatewayDoc::open_lenient();
    let rest = doc.rest();
    Opts {
        secrets: flag(rest, "redact"),
        personal: flag(rest, "redact_personal"),
        words_on: flag(rest, "redact_words"),
        rules_on: flag(rest, "redact_rules"),
        words: strings(rest, "redact_word_list"),
        rules: user_rules(rest, "redact_rule_list"),
    }
}

impl Opts {
    fn active(&self) -> bool {
        self.secrets
            || self.personal
            || (self.words_on && self.words.iter().any(|word| word.trim().len() >= 2))
            || (self.rules_on && self.rules.iter().any(UserRule::usable))
    }
}

fn flag(map: &BTreeMap<String, Value>, key: &str) -> bool {
    map.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn strings(map: &BTreeMap<String, Value>, key: &str) -> Vec<String> {
    map.get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn user_rules(map: &BTreeMap<String, Value>, key: &str) -> Vec<UserRule> {
    let Some(items) = map.get(key).and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let obj = item.as_object()?;
            Some(UserRule {
                kind: obj
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                prefix: obj
                    .get("prefix")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string(),
                regex: obj
                    .get("regex")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            })
        })
        .collect()
}

impl UserRule {
    fn usable(&self) -> bool {
        !self.prefix.is_empty() || !self.regex.is_empty()
    }
}

fn walk_mask(body: &[u8], opts: &Opts) -> json::Walked {
    let custom = custom_rules(opts);
    json::walk(body, &mut |_, key, text| {
        if keep(key, text) {
            return None;
        }
        let next = mask_text(text, opts, &custom);
        if next == text { None } else { Some(next) }
    })
}

fn walk_unmask(body: &[u8]) -> json::Walked {
    let mut event = String::new();
    json::walk(body, &mut |path, key, text| {
        if path == "type" {
            event = text.to_string();
        }
        let escaped = key == "arguments"
            || key == "partial_json"
            || (key == "delta" && event.contains("arguments"));
        unmask_text(text, escaped)
    })
}

fn mask_raw(body: &[u8], opts: &Opts) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(body) else {
        return body.to_vec();
    };
    let next = mask_text(text, opts, &custom_rules(opts));
    if next == text {
        body.to_vec()
    } else {
        next.into_bytes()
    }
}

fn unmask_raw(body: &[u8]) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(body) else {
        return body.to_vec();
    };
    match unmask_text(text, false) {
        Some(next) => next.into_bytes(),
        None => body.to_vec(),
    }
}

fn custom_rules(opts: &Opts) -> Vec<LiveRule> {
    if opts.rules_on {
        live_rules(&opts.rules)
    } else {
        Vec::new()
    }
}

fn mask_text(text: &str, opts: &Opts, custom: &[LiveRule]) -> String {
    if text.len() < 3 {
        return text.to_string();
    }
    let mut spans = Vec::new();
    if opts.words_on {
        for word in &opts.words {
            let word = word.trim();
            if word.len() < 2 {
                continue;
            }
            let mut from = 0;
            while let Some(rel) = text[from..].find(word) {
                let start = from + rel;
                spans.push(Span {
                    start,
                    end: start + word.len(),
                    kind: "TERM".to_string(),
                });
                from = start + word.len();
            }
        }
    }
    if opts.rules_on {
        for rule in custom {
            let markers: Vec<&str> = rule.markers.iter().map(String::as_str).collect();
            scan(
                text,
                &rule.kind,
                &rule.re,
                &markers,
                &[],
                rules::Check::Min(MIN_CUSTOM),
                &mut spans,
            );
        }
    }
    if opts.secrets || opts.personal {
        for built in rules::builtins() {
            if (built.personal && !opts.personal) || (!built.personal && !opts.secrets) {
                continue;
            }
            scan(
                text,
                built.kind,
                &built.re,
                built.markers.as_slice(),
                built.bound,
                built.check,
                &mut spans,
            );
        }
    }
    if spans.is_empty() {
        return text.to_string();
    }
    if text.contains("{{") {
        let holes: Vec<(usize, usize)> = PLACEHOLDER
            .find_iter(text)
            .map(|hit| (hit.start(), hit.end()))
            .collect();
        spans.retain(|span| {
            !holes
                .iter()
                .any(|(start, end)| span.start < *end && span.end > *start)
        });
    }
    if spans.is_empty() {
        return text.to_string();
    }
    spans.sort_by(|left, right| left.start.cmp(&right.start).then(right.end.cmp(&left.end)));
    let mut out = String::new();
    let mut last = 0;
    for span in spans {
        if span.start < last {
            continue;
        }
        out.push_str(&text[last..span.start]);
        out.push_str(&placeholder(&span.kind, &text[span.start..span.end]));
        last = span.end;
    }
    out.push_str(&text[last..]);
    out
}

fn scan(
    text: &str,
    kind: &str,
    re: &Regex,
    markers: &[&str],
    bound: &[u8],
    check: rules::Check,
    out: &mut Vec<Span>,
) {
    if !markers.is_empty() && !markers.iter().any(|marker| text.contains(marker)) {
        return;
    }
    for caps in re.captures_iter(text) {
        let Some((start, end)) = span_of(&caps) else {
            continue;
        };
        if touches(text, start, end, bound) || !passes(check, &text[start..end]) {
            continue;
        }
        out.push(Span {
            start,
            end,
            kind: kind.to_string(),
        });
    }
}

fn span_of(caps: &Captures<'_>) -> Option<(usize, usize)> {
    // A pattern with a group masks that group. Anything else masks the whole match.
    let matched = if caps.len() > 1 {
        caps.get(1).filter(|group| group.end() > group.start())?
    } else {
        caps.get(0)?
    };
    Some((matched.start(), matched.end()))
}

fn touches(text: &str, start: usize, end: usize, bound: &[u8]) -> bool {
    if bound.is_empty() {
        return false;
    }
    let bytes = text.as_bytes();
    (start > 0 && bound.contains(&bytes[start - 1]))
        || (end < bytes.len() && bound.contains(&bytes[end]))
}

fn passes(check: rules::Check, value: &str) -> bool {
    match check {
        rules::Check::None => true,
        rules::Check::Secret => secret_value(value),
        rules::Check::Variable => not_a_variable(value),
        rules::Check::Email => real_email(value),
        rules::Check::Id => chinese_id(value),
        rules::Check::Luhn => luhn(value),
        rules::Check::Min(min) => value.len() >= min,
    }
}

fn not_a_variable(value: &str) -> bool {
    let Some(first) = value.chars().next() else {
        return false;
    };
    if "$%{<[*".contains(first) {
        return false;
    }
    !value
        .trim_matches(|c| c == '*' || c == 'x' || c == 'X' || c == '.')
        .is_empty()
}

fn secret_value(value: &str) -> bool {
    if !not_a_variable(value) || value.starts_with("process.env") || value.starts_with("os.") {
        return false;
    }
    value.chars().any(|c| c.is_ascii_digit()) && value.chars().any(|c| c.is_ascii_alphabetic())
}

fn real_email(value: &str) -> bool {
    let Some(host) = value
        .rsplit_once('@')
        .map(|(_, host)| host.to_ascii_lowercase())
    else {
        return false;
    };
    const FAKES: &[&str] = &[
        "example.com",
        "example.org",
        "example.net",
        "localhost",
        ".test",
        ".invalid",
        ".example",
    ];
    for fake in FAKES {
        let bare = fake.trim_start_matches('.');
        if host == bare || host.ends_with(fake) {
            return false;
        }
    }
    !host.starts_with("noreply") && !value.contains("noreply@")
}

fn chinese_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 18 {
        return false;
    }
    let weights = [7, 9, 10, 5, 8, 4, 2, 1, 6, 3, 7, 9, 10, 5, 8, 4, 2];
    let mut sum = 0;
    for (index, weight) in weights.iter().enumerate() {
        let Some(digit) = (bytes[index] as char).to_digit(10) else {
            return false;
        };
        sum += digit as usize * weight;
    }
    bytes[17].to_ascii_uppercase() == b"10X98765432"[sum % 11]
}

fn luhn(value: &str) -> bool {
    let digits: Vec<u32> = value.chars().filter_map(|c| c.to_digit(10)).collect();
    if !(15..=19).contains(&digits.len()) {
        return false;
    }
    let mut sum = 0;
    for (index, digit) in digits.iter().rev().enumerate() {
        let mut digit = *digit;
        if index % 2 == 1 {
            digit *= 2;
            if digit > 9 {
                digit -= 9;
            }
        }
        sum += digit;
    }
    sum % 10 == 0
}

fn unmask_text(text: &str, escaped: bool) -> Option<String> {
    if !PLACEHOLDER.is_match(text) {
        return None;
    }
    let memory = lock();
    let mut changed = false;
    let next = PLACEHOLDER.replace_all(text, |caps: &Captures| {
        let token = caps.get(0).map(|item| item.as_str()).unwrap_or("");
        match memory.values.get(token) {
            Some(value) => {
                changed = true;
                if escaped {
                    json::json_escape(value)
                } else {
                    value.clone()
                }
            }
            None => token.to_string(),
        }
    });
    if changed {
        Some(next.into_owned())
    } else {
        None
    }
}

fn placeholder(kind: &str, value: &str) -> String {
    let key = ensure_key();
    let token = suffix(&key, kind, value);
    let written = format!("{{{{{kind}_{token}}}}}");
    remember(written.clone(), value);
    written
}

fn suffix(key: &[u8; 32], kind: &str, value: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in key
        .iter()
        .copied()
        .chain(kind.as_bytes().iter().copied())
        .chain(std::iter::once(0))
        .chain(value.as_bytes().iter().copied())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    let mut out = String::with_capacity(8);
    for index in 0..8 {
        let shift = 5 * (7 - index);
        let piece = ((hash >> shift) & 31) as usize;
        out.push(B32[piece] as char);
    }
    out
}

fn remember(placeholder: String, value: &str) {
    let mut memory = lock();
    if memory.values.len() >= MAX_VALUES {
        let drop_n = memory.values.len() / 2;
        let stale: Vec<String> = memory.values.keys().take(drop_n).cloned().collect();
        for key in stale {
            memory.values.remove(&key);
        }
    }
    memory
        .values
        .entry(placeholder)
        .or_insert_with(|| value.to_string());
}

fn ensure_key() -> [u8; 32] {
    let path = key_path();
    let mut memory = lock();
    if memory.key_path == path
        && let Some(key) = memory.key
    {
        return key;
    }
    let key = load_or_create(&path);
    memory.key = Some(key);
    memory.key_path = path;
    key
}

fn load_or_create(path: &Path) -> [u8; 32] {
    if let Ok(bytes) = fs::read(path)
        && bytes.len() >= 32
    {
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes[..32]);
        return key;
    }
    let key = fresh_key();
    let _ = write_key(path, &key);
    key
}

fn fresh_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    if let Ok(mut file) = File::open("/dev/urandom")
        && file.read_exact(&mut key).is_ok()
    {
        return key;
    }
    let tick = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let mut state = 0x6a09_e667_f3bc_c909_u64;
    let mixed = (tick as u64) ^ ((tick >> 64) as u64) ^ u64::from(std::process::id());
    for (index, byte) in key.iter_mut().enumerate() {
        state = state
            .wrapping_mul(0x100_0000_01b3)
            .wrapping_add(mixed)
            .wrapping_add(index as u64);
        *byte = (state >> 13) as u8;
    }
    key[0] |= 0xa5;
    key
}

fn write_key(path: &Path, key: &[u8; 32]) -> std::io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        }
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(key)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn live_rules(rules: &[UserRule]) -> Vec<LiveRule> {
    let mut out = Vec::new();
    for rule in rules {
        if !rule.usable() || (rule.prefix.is_empty() && rule.regex.len() > MAX_REGEX) {
            continue;
        }
        let source = if rule.prefix.is_empty() {
            rule.regex.clone()
        } else {
            format!("{}[A-Za-z0-9_\\-]{{8,}}", regex::escape(&rule.prefix))
        };
        let Ok(re) = Regex::new(&source) else {
            continue;
        };
        if re.is_match("") {
            continue;
        }
        let markers = if rule.prefix.is_empty() {
            Vec::new()
        } else {
            vec![rule.prefix.clone()]
        };
        out.push(LiveRule {
            kind: rule_kind(&rule.kind),
            re,
            markers,
        });
    }
    out
}

fn rule_kind(kind: &str) -> String {
    let mut raw = String::new();
    for ch in kind.trim().chars() {
        let upper = ch.to_ascii_uppercase();
        match upper {
            'A'..='Z' | '0'..='9' | '_' => raw.push(upper),
            ' ' | '-' | '.' => raw.push('_'),
            _ => {}
        }
    }
    let trimmed = raw.trim_matches('_');
    let mut kind = if trimmed.is_empty() {
        "CUSTOM".to_string()
    } else if !trimmed.starts_with(|c: char| c.is_ascii_uppercase()) {
        format!("CUSTOM_{trimmed}")
    } else {
        trimmed.to_string()
    };
    if kind.len() > 24 {
        kind.truncate(24);
        while kind.ends_with('_') {
            kind.pop();
        }
    }
    if kind.is_empty() {
        "CUSTOM".to_string()
    } else {
        kind
    }
}

fn keep(key: &str, text: &str) -> bool {
    text.starts_with("data:")
        || matches!(
            key,
            "signature"
                | "encrypted_content"
                | "data"
                | "thoughtSignature"
                | "thought_signature"
                | "model"
                | "id"
                | "tool_use_id"
                | "call_id"
                | "item_id"
                | "type"
                | "role"
                | "name"
                | "previous_response_id"
                | "prompt_cache_key"
                | "media_type"
                | "mime_type"
                | "mimeType"
                | "url"
                | "image_url"
                | "file_id"
                | "reasoning_effort"
                | "effort"
                | "stop_reason"
                | "finish_reason"
                | "status"
                | "object"
                | "event"
        )
}

fn lock() -> std::sync::MutexGuard<'static, Memory> {
    MEMORY
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn key_path() -> PathBuf {
    skillstar_core::infra::paths::config_dir().join("redact.key")
}
