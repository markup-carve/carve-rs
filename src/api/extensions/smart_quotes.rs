//! `smartQuotes`: locale quote glyphs for smart typography; the
//! `smartTypography` option still turns it on.

use super::{Bundle, HostFields, declaration};
use crate::api::Extension;

/// Open and close double, then open and close single quote.
pub type QuoteCharacters = [&'static str; 4];

const ENGLISH: QuoteCharacters = ["\u{201c}", "\u{201d}", "\u{2018}", "\u{2019}"];

/// The locale table, in the carve-js order.
pub const SMART_QUOTE_LOCALES: &[(&str, QuoteCharacters)] = &[
    ("en", ["\u{201c}", "\u{201d}", "\u{2018}", "\u{2019}"]),
    ("de", ["\u{201e}", "\u{201c}", "\u{201a}", "\u{2018}"]),
    ("de-CH", ["\u{ab}", "\u{bb}", "\u{2039}", "\u{203a}"]),
    (
        "fr",
        [
            "\u{ab}\u{a0}",
            "\u{a0}\u{bb}",
            "\u{2039}\u{a0}",
            "\u{a0}\u{203a}",
        ],
    ),
    ("pl", ["\u{201e}", "\u{201d}", "\u{201a}", "\u{2019}"]),
    ("ru", ["\u{ab}", "\u{bb}", "\u{201e}", "\u{201c}"]),
    ("ja", ["\u{300c}", "\u{300d}", "\u{300e}", "\u{300f}"]),
    ("zh", ["\u{300c}", "\u{300d}", "\u{300e}", "\u{300f}"]),
    ("sv", ["\u{201d}", "\u{201d}", "\u{2019}", "\u{2019}"]),
    ("da", ["\u{201e}", "\u{201c}", "\u{201a}", "\u{2018}"]),
    ("fi", ["\u{201d}", "\u{201d}", "\u{2019}", "\u{2019}"]),
    ("cs", ["\u{201e}", "\u{201c}", "\u{201a}", "\u{2018}"]),
    ("hu", ["\u{201e}", "\u{201d}", "\u{201a}", "\u{2019}"]),
    ("it", ["\u{ab}", "\u{bb}", "\u{201c}", "\u{201d}"]),
    ("es", ["\u{ab}", "\u{bb}", "\u{201c}", "\u{201d}"]),
    ("pt", ["\u{ab}", "\u{bb}", "\u{201c}", "\u{201d}"]),
    ("nl", ["\u{201c}", "\u{201d}", "\u{2018}", "\u{2019}"]),
    ("nb", ["\u{ab}", "\u{bb}", "\u{2018}", "\u{2019}"]),
    ("nn", ["\u{ab}", "\u{bb}", "\u{2018}", "\u{2019}"]),
    ("uk", ["\u{ab}", "\u{bb}", "\u{201e}", "\u{201c}"]),
];

/// The options of [`smart_quotes`]; a glyph given here replaces the locale's.
#[derive(Debug, Clone, Default)]
pub struct SmartQuotesOptions {
    /// Exact locale, then its language; an unknown locale uses English.
    pub locale: Option<String>,
    pub open_double_quote: Option<String>,
    pub close_double_quote: Option<String>,
    pub open_single_quote: Option<String>,
    pub close_single_quote: Option<String>,
}

fn row(key: &str) -> Option<&'static QuoteCharacters> {
    SMART_QUOTE_LOCALES
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, quotes)| quotes)
}

/// The exact locale, then its language subtag.
fn lookup(locale: &str) -> Option<&'static QuoteCharacters> {
    let normalized = locale.replace('_', "-").to_ascii_lowercase();
    let language = normalized.split('-').next().unwrap_or(&normalized);
    row(&normalized).or_else(|| row(language))
}

/// The locales of the table.
#[must_use]
pub fn smart_quote_locales() -> Vec<&'static str> {
    SMART_QUOTE_LOCALES.iter().map(|(name, _)| *name).collect()
}

/// Whether the locale or its language is in the table.
#[must_use]
pub fn is_smart_quote_locale_supported(locale: &str) -> bool {
    lookup(locale).is_some()
}

/// The `carve.smartQuotes` bundle.
#[must_use]
pub fn smart_quotes(options: &SmartQuotesOptions) -> Bundle {
    let quotes = lookup(options.locale.as_deref().unwrap_or("en")).unwrap_or(&ENGLISH);
    let pick = |given: &Option<String>, at: usize| {
        given
            .clone()
            .unwrap_or_else(|| quotes.get(at).copied().unwrap_or_default().to_string())
    };
    Bundle {
        extensions: vec![Extension {
            declaration: declaration("carve.smartQuotes", &[]),
            renderers: Vec::new(),
            transform: None,
        }],
        matchers: Vec::new(),
        host: HostFields {
            semantic_keys: None,
            quotes: Some([
                pick(&options.open_double_quote, 0),
                pick(&options.close_double_quote, 1),
                pick(&options.open_single_quote, 2),
                pick(&options.close_single_quote, 3),
            ]),
        },
    }
}
