//! `semanticSpan`: the span keys `samp`, `var`, `cite` and `dfn` beside the
//! core three, for render and lint (carve-js#5060).

use super::{Bundle, HostFields, SemanticKeys, declaration};
use crate::api::Extension;

/// The 0.1 order from inside out (C2-H-075); a `dfn` value fills `title`.
pub const SEMANTIC_SPAN_KEYS: SemanticKeys = &[
    ("abbr", Some("title")),
    ("time", Some("datetime")),
    ("samp", None),
    ("var", None),
    ("kbd", None),
    ("cite", None),
    ("dfn", Some("title")),
];

/// The `carve.semanticSpan` bundle.
#[must_use]
pub fn semantic_span() -> Bundle {
    Bundle {
        extensions: vec![Extension {
            declaration: declaration("carve.semanticSpan", &[]),
            renderers: Vec::new(),
            transform: None,
        }],
        matchers: Vec::new(),
        host: HostFields {
            semantic_keys: Some(SEMANTIC_SPAN_KEYS),
            quotes: None,
        },
    }
}
