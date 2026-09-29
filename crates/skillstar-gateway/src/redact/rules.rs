//! Built-in secret shapes. A personal rule runs only when `redact_personal`
//! is on. The others run only when `redact` is on.

use std::sync::LazyLock;

use regex::Regex;

const TOKEN: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";
const ALNUM: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
const DIGITS: &[u8] = b"0123456789";

#[derive(Clone, Copy)]
pub(crate) enum Check {
    None,
    Secret,
    Variable,
    Email,
    Id,
    Luhn,
    Min(usize),
}

pub(crate) struct Built {
    pub(crate) kind: &'static str,
    pub(crate) re: Regex,
    pub(crate) markers: Vec<&'static str>,
    pub(crate) bound: &'static [u8],
    pub(crate) personal: bool,
    pub(crate) check: Check,
}

pub(crate) fn builtins() -> &'static [Built] {
    static BUILT: LazyLock<Vec<Built>> = LazyLock::new(|| {
        let pattern = |source: &str| Regex::new(source).expect("redact pattern");
        let secret_names =
            "password|passwd|secret|token|api[_-]?key|access[_-]?key|private[_-]?key|credential";
        let quote = r#"(?:\\?["'])?"#;
        let secret_chars = r"[A-Za-z0-9_\-./+=~!@#%^&*]";
        vec![
            Built {
                kind: "PRIVATE_KEY",
                re: pattern(
                    r"-----BEGIN (?:[A-Z0-9]+ )*PRIVATE KEY(?: BLOCK)?-----[\s\S]+?-----END (?:[A-Z0-9]+ )*PRIVATE KEY(?: BLOCK)?-----",
                ),
                markers: vec!["PRIVATE KEY"],
                bound: b"",
                personal: false,
                check: Check::None,
            },
            Built {
                kind: "API_KEY",
                re: pattern(r"sk-(?:ant-|proj-|or-|svcacct-|admin-)?[A-Za-z0-9_-]{20,}"),
                markers: vec!["sk-"],
                bound: TOKEN,
                personal: false,
                check: Check::None,
            },
            Built {
                kind: "API_KEY",
                re: pattern(r"[a-z]{2,4}_sk_[A-Za-z0-9_-]{20,}"),
                markers: vec!["_sk_"],
                bound: TOKEN,
                personal: false,
                check: Check::Secret,
            },
            Built {
                kind: "API_KEY",
                re: pattern(
                    r"(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,}|glpat-[A-Za-z0-9_-]{20,})",
                ),
                markers: vec!["gh", "github_pat_", "glpat-"],
                bound: TOKEN,
                personal: false,
                check: Check::None,
            },
            Built {
                kind: "API_KEY",
                re: pattern(r"AIza[0-9A-Za-z_-]{35}"),
                markers: vec!["AIza"],
                bound: TOKEN,
                personal: false,
                check: Check::None,
            },
            Built {
                kind: "API_KEY",
                re: pattern(r"xox[abposr]-[0-9A-Za-z-]{10,}"),
                markers: vec!["xox"],
                bound: TOKEN,
                personal: false,
                check: Check::None,
            },
            Built {
                kind: "API_KEY",
                re: pattern(r"(?:sk|rk)_live_[0-9A-Za-z]{20,}"),
                markers: vec!["_live_"],
                bound: TOKEN,
                personal: false,
                check: Check::None,
            },
            Built {
                kind: "API_KEY",
                re: pattern(r"(?:AKIA|ASIA)[0-9A-Z]{16}"),
                markers: vec!["AKIA", "ASIA"],
                bound: ALNUM,
                personal: false,
                check: Check::None,
            },
            Built {
                kind: "API_KEY",
                re: pattern(
                    r"(?:hf_[A-Za-z0-9]{30,}|gsk_[A-Za-z0-9]{40,}|xai-[A-Za-z0-9]{40,}|npm_[A-Za-z0-9]{36}|pypi-[A-Za-z0-9_-]{50,}|dop_v1_[a-f0-9]{64}|SG\.[A-Za-z0-9_-]{22}\.[A-Za-z0-9_-]{43})",
                ),
                markers: vec!["hf_", "gsk_", "xai-", "npm_", "pypi-", "dop_v1_", "SG."],
                bound: TOKEN,
                personal: false,
                check: Check::None,
            },
            Built {
                kind: "TOKEN",
                re: pattern(r"eyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}"),
                markers: vec!["eyJ"],
                bound: TOKEN,
                personal: false,
                check: Check::None,
            },
            Built {
                kind: "PASSWORD",
                re: pattern(r#"[A-Za-z][A-Za-z0-9+.-]*://[^\s:@/?#"'<>]+:([^\s@/?#"'<>]{3,})@"#),
                markers: vec!["://"],
                bound: b"",
                personal: false,
                check: Check::Variable,
            },
            Built {
                kind: "SECRET",
                re: pattern(&format!(
                    r"(?i)[A-Za-z0-9_.-]*(?:{secret_names})[A-Za-z0-9_.-]*{quote}[ \t]*[:=][ \t]*{quote}({secret_chars}{{8,}})"
                )),
                markers: vec![
                    "pass",
                    "PASS",
                    "Pass",
                    "secret",
                    "SECRET",
                    "Secret",
                    "token",
                    "TOKEN",
                    "Token",
                    "key",
                    "KEY",
                    "Key",
                    "credential",
                    "CREDENTIAL",
                    "Credential",
                ],
                bound: b"",
                personal: false,
                check: Check::Secret,
            },
            Built {
                kind: "SECRET",
                re: pattern(&format!(
                    r#"(?i)(?:^|[^A-Za-z0-9_.-]){quote}key{quote}[ \t]*[:=][ \t]*{quote}({secret_chars}{{16,}})"#
                )),
                markers: vec!["key", "KEY", "Key"],
                bound: b"",
                personal: false,
                check: Check::Secret,
            },
            Built {
                kind: "EMAIL",
                re: pattern(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}"),
                markers: vec!["@"],
                bound: b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789._%+-",
                personal: true,
                check: Check::Email,
            },
            Built {
                kind: "ID_CARD",
                re: pattern(
                    r"[1-9][0-9]{5}(?:18|19|20)[0-9]{2}(?:0[1-9]|1[0-2])(?:0[1-9]|[12][0-9]|3[01])[0-9]{3}[0-9Xx]",
                ),
                markers: Vec::new(),
                bound: ALNUM,
                personal: true,
                check: Check::Id,
            },
            Built {
                kind: "PHONE",
                re: pattern(r"(?:\+?86[- ]?)?1[3-9][0-9]{9}"),
                markers: Vec::new(),
                bound: DIGITS,
                personal: true,
                check: Check::None,
            },
            Built {
                kind: "BANK_CARD",
                re: pattern(r"[3-6][0-9]{3}(?:[ -]?[0-9]{4}){2,3}(?:[ -]?[0-9]{1,3})?"),
                markers: Vec::new(),
                bound: DIGITS,
                personal: true,
                check: Check::Luhn,
            },
        ]
    });
    BUILT.as_slice()
}
