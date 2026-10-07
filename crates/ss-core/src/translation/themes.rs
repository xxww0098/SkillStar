//! Visual themes for a translation, taken from KISS Translator's text styles.
//!
//! Order matches the settings gallery. One theme styles a translated
//! description on a card and the line under an English paragraph. Ids are
//! stable config values and are not prefixes of each other.

/// One selectable translation theme.
pub struct Theme {
    pub id: &'static str,
    pub label_key: &'static str,
}

/// Full catalog. Order is the settings gallery order.
pub const THEMES: &[Theme] = &[
    Theme {
        id: "none",
        label_key: "settings.tranThemeNone",
    },
    Theme {
        id: "line",
        label_key: "settings.tranThemeLine",
    },
    Theme {
        id: "dot",
        label_key: "settings.tranThemeDot",
    },
    Theme {
        id: "dash",
        label_key: "settings.tranThemeDash",
    },
    Theme {
        id: "dbold",
        label_key: "settings.tranThemeDbold",
    },
    Theme {
        id: "wavy",
        label_key: "settings.tranThemeWavy",
    },
    Theme {
        id: "wbold",
        label_key: "settings.tranThemeWbold",
    },
    Theme {
        id: "box",
        label_key: "settings.tranThemeBox",
    },
    Theme {
        id: "xbold",
        label_key: "settings.tranThemeXbold",
    },
    Theme {
        id: "mark",
        label_key: "settings.tranThemeMark",
    },
    Theme {
        id: "gmark",
        label_key: "settings.tranThemeGmark",
    },
    Theme {
        id: "fuzzy",
        label_key: "settings.tranThemeFuzzy",
    },
    Theme {
        id: "hi",
        label_key: "settings.tranThemeHi",
    },
    Theme {
        id: "quote",
        label_key: "settings.tranThemeQuote",
    },
    Theme {
        id: "grad",
        label_key: "settings.tranThemeGrad",
    },
    Theme {
        id: "blink",
        label_key: "settings.tranThemeBlink",
    },
    Theme {
        id: "glow",
        label_key: "settings.tranThemeGlow",
    },
    Theme {
        id: "color",
        label_key: "settings.tranThemeColor",
    },
];

pub const DEFAULT_READER: &str = "quote";

pub fn canonical_reader(id: &str) -> &'static str {
    THEMES
        .iter()
        .find(|theme| theme.id == id)
        .map(|theme| theme.id)
        .unwrap_or(DEFAULT_READER)
}

pub fn reader_themes() -> impl Iterator<Item = &'static Theme> {
    THEMES.iter()
}
