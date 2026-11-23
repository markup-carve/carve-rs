//! Lint of source lines after the 0.2 parse has marked paragraphs and opaque spans.

use super::Range;

pub(super) struct Line {
    pub(super) text: String,
    /// The text with each backslash-escaped punctuation character read as NUL, for the data rules; set only
    /// when that differs from `text`.
    pub(super) escaped: Option<String>,
    pub(super) start: usize,
    pub(super) path: String,
    /// Where the tree places this line's text: its first leaf, or the end of the break before it.
    pub(super) content: Option<usize>,
}

pub(super) fn lines(source: &str, ranges: &[Range<'_>]) -> Vec<Line> {
    let mut starts = Vec::new();
    let mut raws = Vec::new();
    let mut start = 0;
    for piece in source.split_inclusive('\n') {
        starts.push(start);
        raws.push(piece.trim_end_matches(['\n', '\r']));
        start += piece.len();
    }
    let count = raws.len();
    let facts = line_facts(&starts, &raws, ranges);
    let mut out = Vec::with_capacity(count + 1);
    let lines = starts.iter().zip(raws).zip(facts);
    for ((&start, raw), fact) in lines {
        let bytes = masked(raw, start, &fact);
        let path = fact
            .paragraph
            .or(fact.line_block)
            .map_or_else(String::new, |r| r.path.text());
        let opaque = fact.flags & (FRONTMATTER | OPAQUE) != 0;
        out.push(Line {
            escaped: (!opaque).then(|| escape_masked(raw, &bytes)).flatten(),
            text: String::from_utf8_lossy(&bytes).into_owned(),
            start,
            path,
            content: fact
                .content
                .filter(|at| *at >= start && *at <= start + raw.len())
                .map(|at| at - start),
        });
    }
    if source.ends_with('\n') || source.is_empty() {
        out.push(Line {
            text: String::new(),
            escaped: None,
            start: source.len(),
            path: String::new(),
            content: None,
        });
    }
    out
}

fn line_facts<'a>(starts: &[usize], raws: &[&str], ranges: &'a [Range<'a>]) -> Vec<LineFacts<'a>> {
    let count = raws.len();
    // One pass per range over the lines it covers, instead of every range per line.
    let mut facts = vec![LineFacts::default(); count];
    for range in ranges {
        let kind = range.kind;
        mark_content(&mut facts, range);
        let paragraph_like = matches!(kind, "paragraph" | "caption" | "definition_term");
        let flags = range_flags(kind);
        if paragraph_like || flags != 0 || matches!(kind, "line_block" | "block_quote") {
            let first = range.start_line.max(1) - 1;
            let covered = facts.get_mut(first..range.end_line.min(count));
            for fact in covered.into_iter().flatten() {
                fact.flags |= flags;
                fact.quotes += usize::from(kind == "block_quote");
                // `max_by_key` keeps the last of equal keys.
                if paragraph_like
                    && fact.paragraph.is_none_or(|best| {
                        (range.kind == "caption", range.path.len())
                            >= (best.kind == "caption", best.path.len())
                    })
                {
                    fact.paragraph = Some(range);
                }
                if kind == "line_block" {
                    fact.line_block = Some(range);
                }
            }
        }
        if is_opaque_leaf(kind) {
            mask_opaque_lines(starts, raws, &mut facts, range);
        }
    }
    facts
}

/// The line with the characters of its masked spans read as NUL.
fn masked(raw: &str, start: usize, fact: &LineFacts<'_>) -> Vec<u8> {
    let mut bytes = raw.as_bytes().to_vec();
    // C2-B-027: the quote marks that prefix a later line of a span are no span characters.
    let prefix = super::data_model::quote_prefix_end(raw, fact.quotes);
    for range in &fact.masks {
        let mut from = range.start.saturating_sub(start).min(bytes.len());
        if range.start < start {
            from = from.max(prefix.min(bytes.len()));
        }
        let to = range.end.saturating_sub(start).min(bytes.len());
        bytes
            .get_mut(from..to)
            .into_iter()
            .flatten()
            .for_each(|b| *b = 0);
    }
    bytes
}

/// C2-B-010: a punctuation character behind an odd run of backslashes is escaped; counted on the raw line.
fn escape_masked(raw: &str, masked: &[u8]) -> Option<String> {
    let raw = raw.as_bytes();
    if !raw.contains(&b'\\') {
        return None;
    }
    let mut bytes = masked.to_vec();
    let mut changed = false;
    let mut slashes = 0;
    for (byte, out) in raw.iter().zip(bytes.iter_mut()) {
        if byte.is_ascii_punctuation() && *out != 0 && slashes % 2 == 1 {
            *out = 0;
            changed = true;
        }
        slashes = if *byte == b'\\' { slashes + 1 } else { 0 };
    }
    changed.then(|| String::from_utf8_lossy(&bytes).into_owned())
}

fn mask_opaque_lines<'a>(
    starts: &[usize],
    raws: &[&str],
    facts: &mut [LineFacts<'a>],
    range: &'a Range<'a>,
) {
    let first = starts
        .partition_point(|s| *s <= range.start)
        .saturating_sub(1);
    let lines = starts
        .get(first..)
        .unwrap_or_default()
        .iter()
        .zip(raws.get(first..).unwrap_or_default())
        .zip(facts.get_mut(first..).unwrap_or_default());
    for ((line_start, raw), fact) in lines {
        if *line_start >= range.end {
            break;
        }
        if range.start < line_start + raw.len() {
            fact.masks.push(range);
        }
    }
}

/// A leaf block starts its first line's text, and each break ends where the next line's text starts.
fn mark_content(facts: &mut [LineFacts<'_>], range: &Range<'_>) {
    let content = match range.kind {
        "soft_break" | "hard_break" => Some((range.end_line, range.end)),
        "paragraph" | "caption" | "line" | "heading" | "definition_term" | "thematic_break"
        | "table_row" => Some((range.start_line, range.start)),
        _ => None,
    };
    if let Some((line, at)) = content
        && let Some(fact) = facts.get_mut(line.max(1) - 1)
        && fact.content.is_none_or(|known| at < known)
    {
        fact.content = Some(at);
    }
}

fn range_flags(kind: &str) -> u8 {
    match kind {
        "code_block" | "raw_block" => OPAQUE,
        "frontmatter" => OPAQUE | FRONTMATTER,
        _ => 0,
    }
}

const OPAQUE: u8 = 1;
const FRONTMATTER: u8 = 2;

#[derive(Clone, Default)]
struct LineFacts<'r> {
    flags: u8,
    content: Option<usize>,
    quotes: usize,
    paragraph: Option<&'r Range<'r>>,
    line_block: Option<&'r Range<'r>>,
    masks: Vec<&'r Range<'r>>,
}

fn is_opaque_leaf(kind: &str) -> bool {
    matches!(
        kind,
        "code"
            | "autolink"
            | "raw_inline"
            | "literal_inline"
            | "math"
            | "comment"
            | "code_block"
            | "raw_block"
            | "frontmatter"
            | "include"
            | "link_reference_definition"
            | "abbreviation_def"
    )
}

/// C2-B-090: one or more glued valid attribute blocks alone on the line, spaced or with an id or a key (carve#6197).
pub(super) fn valid_attribute_line(text: &str) -> bool {
    let text = text.trim_end_matches([' ', '\t']);
    text.starts_with('{')
        && crate::attr::parse_glued(text, 0).is_some_and(|(_, end)| end == text.len())
}
