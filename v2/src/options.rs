//! The options of `options.json` that affect parse, resolution and HTML
//! (C2-H-137). Defaults come from the generated tables.

use crate::tables;
use std::collections::BTreeMap;

/// `headingIds` (C2-H-131).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeadingIds {
    Off,
    #[default]
    KeepCase,
    Lower,
    Ascii,
}

impl HeadingIds {
    /// Parses an option value.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "off" => Some(Self::Off),
            "keep-case" => Some(Self::KeepCase),
            "lower" => Some(Self::Lower),
            "ascii" => Some(Self::Ascii),
            _ => None,
        }
    }
}

/// `urlPolicy` (C2-H-128): a deny list or an allow list of schemes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UrlPolicy {
    Deny(Vec<String>),
    Allow(Vec<String>),
}

impl Default for UrlPolicy {
    fn default() -> Self {
        Self::Deny(
            tables::DEFAULT_DENIED_SCHEMES
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
        )
    }
}

/// `profile` (C2-H-129).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Profile {
    #[default]
    Full,
    Article,
    Comment,
    Minimal,
}

impl Profile {
    /// Parses an option value.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "full" => Some(Self::Full),
            "article" => Some(Self::Article),
            "comment" => Some(Self::Comment),
            "minimal" => Some(Self::Minimal),
            _ => None,
        }
    }

    /// The name as `url-policy.json` spells it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Article => "article",
            Self::Comment => "comment",
            Self::Minimal => "minimal",
        }
    }
}

/// `mode` (C2-H-122).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Interactive,
    Static,
}

/// The render options (and `headingIds`, which the resolution reads).
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)] // the option list is booleans by spec (C2-H-137)
pub struct Options {
    pub heading_ids: HeadingIds,
    pub mode: Mode,
    pub symbols: BTreeMap<String, String>,
    pub labels: BTreeMap<String, String>,
    pub source_lines: bool,
    pub sections: bool,
    pub smart_typography: bool,
    pub roundtrip: bool,
    pub raw_html: bool,
    pub url_policy: UrlPolicy,
    pub profile: Profile,
    pub strict_losses: bool,
    pub social: Option<Social>,
}

/// URL templates for mentions and tags (C2-H-126). Gap G-022: the
/// specification names no template syntax; as in the oracle, every
/// `{name}` in a template is replaced by the mention's target or the tag's
/// name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Social {
    pub mention_url: Option<String>,
    pub tag_url: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            heading_ids: HeadingIds::default(),
            mode: Mode::default(),
            symbols: BTreeMap::new(),
            labels: tables::DEFAULT_LABELS
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            source_lines: tables::DEFAULT_SOURCE_LINES,
            sections: tables::DEFAULT_SECTIONS,
            smart_typography: tables::DEFAULT_SMART_TYPOGRAPHY,
            roundtrip: tables::DEFAULT_ROUNDTRIP,
            raw_html: tables::DEFAULT_RAW_HTML,
            url_policy: UrlPolicy::default(),
            profile: Profile::default(),
            strict_losses: tables::DEFAULT_STRICT_LOSSES,
            social: None,
        }
    }
}

impl Options {
    /// A label string (C2-H-125); a key left out keeps its English default.
    #[must_use]
    pub fn label(&self, key: &str) -> &str {
        self.labels.get(key).map_or("", String::as_str)
    }
}
