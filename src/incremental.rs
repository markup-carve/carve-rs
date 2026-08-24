use std::{fmt, ops::Range};

use crate::{parse_with_source_layout, Document};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextChange {
    pub range: Range<usize>,
    pub replacement: String,
}

#[derive(Debug, Clone)]
pub struct ParserSnapshot {
    source: String,
}

impl ParserSnapshot {
    pub fn source(&self) -> &str {
        &self.source
    }
}

#[derive(Debug, Clone)]
pub struct IncrementalParse {
    pub document: Document,
    pub source_layout_json: String,
    pub snapshot: ParserSnapshot,
    pub changed_source: Vec<Range<usize>>,
    pub reused_previous_tree: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncrementalParseError(pub String);

impl fmt::Display for IncrementalParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for IncrementalParseError {}

pub fn parse_snapshot(source: &str) -> IncrementalParse {
    let (document, source_layout_json) = parse_with_source_layout(source);
    IncrementalParse {
        document,
        source_layout_json,
        snapshot: ParserSnapshot {
            source: source.to_owned(),
        },
        changed_source: std::iter::once(0..source.len()).collect(),
        reused_previous_tree: false,
    }
}

pub fn reparse(
    snapshot: ParserSnapshot,
    changes: &[TextChange],
) -> Result<IncrementalParse, IncrementalParseError> {
    let mut source = snapshot.source;
    let mut ordered = changes.to_vec();
    ordered.sort_by_key(|change| change.range.start);
    for pair in ordered.windows(2) {
        if pair[0].range.end > pair[1].range.start {
            return Err(IncrementalParseError("text changes overlap".into()));
        }
    }
    for change in ordered.iter().rev() {
        if change.range.start > change.range.end || change.range.end > source.len() {
            return Err(IncrementalParseError("text change is out of bounds".into()));
        }
        if !source.is_char_boundary(change.range.start)
            || !source.is_char_boundary(change.range.end)
        {
            return Err(IncrementalParseError(
                "text change splits a UTF-8 code point".into(),
            ));
        }
        source.replace_range(change.range.clone(), &change.replacement);
    }
    let (document, source_layout_json) = parse_with_source_layout(&source);
    Ok(IncrementalParse {
        document,
        source_layout_json,
        snapshot: ParserSnapshot { source },
        changed_source: ordered.into_iter().map(|change| change.range).collect(),
        // This first draft establishes edit validation and snapshot semantics.
        // Region reuse follows without changing the public result contract.
        reused_previous_tree: false,
    })
}
