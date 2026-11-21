//! Lint of source lines after the 0.2 parse has marked paragraphs and opaque spans.

use super::{LintOptions, Range, active, drop_replaced};
use crate::diagnostic::Diagnostic;
use crate::source::Source;
use std::collections::HashSet;

#[allow(clippy::struct_excessive_bools)] // Independent facts from separate parse and source checks.
pub(super) struct Line {
    pub(super) text: String,
    /// The text with each backslash-escaped punctuation character read as NUL, for the data rules; set only
    /// when that differs from `text`.
    pub(super) escaped: Option<String>,
    quotes: usize,
    table_row: bool,
    pub(super) start: usize,
    pub(super) path: String,
    kind: String,
    paragraph: bool,
    paragraph_in_div: bool,
    literal_quote_prefix: bool,
    content_start: Option<usize>,
    /// Where the tree places this line's text: its first leaf, or the end of the break before it.
    pub(super) content: Option<usize>,
    opacity: Opacity,
    attached_attribute: bool,
    scope: Scope,
}

struct Scope {
    in_footnote: bool,
    container: ContainerKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Opacity {
    None,
    Other,
    Frontmatter,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ContainerKind {
    None,
    Other,
    ListItem,
}

struct Sink<'a> {
    source: Source<'a>,
    options: &'a LintOptions,
    output: &'a mut Vec<Diagnostic>,
    replacing: Vec<usize>,
}

impl Sink<'_> {
    fn emit(
        &mut self,
        code: &'static str,
        from: usize,
        to: usize,
        path: &str,
        values: &[(&str, &str)],
    ) {
        self.emit_nth(code, from, to, path, values, 0);
    }

    fn emit_nth(
        &mut self,
        code: &'static str,
        from: usize,
        to: usize,
        path: &str,
        values: &[(&str, &str)],
        message: usize,
    ) {
        if !active(code, self.options) {
            return;
        }
        let pos = self.source.pos(from, to);
        let mut diagnostic = Diagnostic::nth(code, message, values, None, Some(pos));
        diagnostic.path = Some(path.to_string());
        self.replacing.push(self.output.len());
        self.output.push(diagnostic);
    }

    fn whole(&mut self, code: &'static str, line: &Line, values: &[(&str, &str)]) {
        let trimmed = line.text.trim();
        let prefix = line.text.len() - line.text.trim_start().len();
        self.emit(
            code,
            line.start + prefix,
            line.start + prefix + trimmed.len(),
            &line.path,
            values,
        );
    }
}

pub(super) fn scan(
    source: &str,
    options: &LintOptions,
    ranges: &[Range<'_>],
    lines: &[Line],
    output: &mut Vec<Diagnostic>,
) {
    let lists = DescriptionLists::new(ranges);
    let mut sink = Sink {
        source: Source::new(source),
        options,
        output,
        replacing: Vec::new(),
    };
    let mut last_nonblank = None;
    for (index, line) in lines.iter().enumerate() {
        if line.opacity != Opacity::None {
            if line.opacity == Opacity::Frontmatter {
                frontmatter_quote_marker(line, &mut sink);
            }
            if !line.text.trim().is_empty() {
                last_nonblank = Some(index);
            }
            continue;
        }
        let previous = index.checked_sub(1).and_then(|i| lines.get(i));
        markers(line, previous, &mut sink);
        let below = lists.below_list_column(line, index, lines);
        if below.is_none() {
            markdown_definition(line, previous, &mut sink);
        }
        if let Some((at, text, column, host)) = below {
            description_column(line, at, text, column, host, &mut sink);
        }
        let prior_block = last_nonblank.and_then(|previous| lines.get(previous));
        legacy(line, previous, prior_block, &mut sink);
        prose(line, index, previous, lines, &mut sink);
        if !line.text.trim().is_empty() {
            last_nonblank = Some(index);
        }
    }
    drop_replaced(sink.output, &sink.replacing);
}

fn frontmatter_quote_marker(line: &Line, sink: &mut Sink<'_>) {
    if !active("blockquote-marker-without-space", sink.options) {
        return;
    }
    let raw = sink
        .source
        .text()
        .get(line.start..line.start + line.text.len())
        .unwrap_or("");
    if !raw.starts_with('>') || raw.len() == 1 || raw.starts_with("> ") {
        return;
    }
    let end = line.start + raw.trim_end_matches([' ', '\t']).len();
    let pos = sink.source.pos(line.start, end);
    let mut finding = Diagnostic::nth(
        "blockquote-marker-without-space",
        1,
        &[("text", raw.get(1..).unwrap_or(""))],
        None,
        Some(pos),
    );
    finding.path = Some(line.path.clone());
    sink.output.push(finding);
}

pub(super) fn lines(source: &str, ranges: &[Range<'_>]) -> Vec<Line> {
    let div_paths: HashSet<String> = ranges
        .iter()
        .filter(|range| {
            matches!(
                range.kind,
                "div" | "directive" | "admonition" | "figure_group"
            )
        })
        .map(|range| range.path.text())
        .collect();
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
    for (index, ((&start, raw), fact)) in lines.enumerate() {
        let number = index + 1;
        let paragraph = fact.paragraph;
        let mut bytes = raw.as_bytes().to_vec();
        // C2-B-027: the quote marks that prefix a later line of a span are no span characters.
        let prefix = super::data_model::quoted_start_pub(raw, fact.quotes);
        for range in fact.masks {
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
        let path = paragraph
            .or(fact.line_block)
            .map_or_else(String::new, |r| r.path.text());
        let paragraph_in_div = paragraph.is_some()
            && path
                .rsplit_once("/children/")
                .is_some_and(|(parent, _)| div_paths.contains(parent));
        let opaque = fact.flags & (FRONTMATTER | OPAQUE) != 0;
        out.push(Line {
            escaped: (!opaque).then(|| escape_masked(raw, &bytes)).flatten(),
            text: String::from_utf8_lossy(&bytes).into_owned(),
            quotes: fact.quotes,
            table_row: fact.flags & TABLE_ROW != 0,
            start,
            path,
            kind: paragraph.map_or_else(
                || {
                    fact.line_block
                        .map_or_else(String::new, |_| "line".to_string())
                },
                |r| r.kind.to_string(),
            ),
            paragraph: paragraph.is_some(),
            paragraph_in_div,
            literal_quote_prefix: fact.flags & LITERAL_QUOTE_PREFIX != 0,
            content_start: paragraph
                .filter(|range| range.start_line == number)
                .map(|range| range.start.saturating_sub(start).min(raw.len())),
            content: fact
                .content
                .filter(|at| *at >= start && *at <= start + raw.len())
                .map(|at| at - start),
            opacity: if fact.flags & FRONTMATTER != 0 {
                Opacity::Frontmatter
            } else if fact.flags & OPAQUE != 0 {
                Opacity::Other
            } else {
                Opacity::None
            },
            attached_attribute: paragraph
                .is_some_and(|range| range.has_attrs && range.start_line == number),
            scope: Scope {
                in_footnote: fact.flags & FOOTNOTE != 0,
                container: if fact.flags & LIST_ITEM != 0 {
                    ContainerKind::ListItem
                } else if fact.flags & CONTAINER != 0 {
                    ContainerKind::Other
                } else {
                    ContainerKind::None
                },
            },
        });
    }
    if source.ends_with('\n') || source.is_empty() {
        out.push(Line {
            text: String::new(),
            escaped: None,
            quotes: 0,
            table_row: false,
            start: source.len(),
            path: String::new(),
            kind: String::new(),
            paragraph: false,
            paragraph_in_div: false,
            literal_quote_prefix: false,
            content_start: None,
            content: None,
            opacity: Opacity::None,
            attached_attribute: false,
            scope: Scope {
                in_footnote: false,
                container: ContainerKind::None,
            },
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
        if kind == "text"
            && let Some(line) = range.start_line.checked_sub(1)
            && starts.get(line) == Some(&range.start)
            && raws.get(line).is_some_and(|raw| raw.starts_with("> "))
            && let Some(fact) = facts.get_mut(line)
        {
            fact.flags |= LITERAL_QUOTE_PREFIX;
        }
        mark_content(&mut facts, range);
        let paragraph_like = matches!(kind, "paragraph" | "caption" | "definition_term");
        let flags = range_flags(kind);
        if paragraph_like || flags != 0 || kind == "line_block" {
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
        "list_item" => LIST_ITEM | CONTAINER,
        "footnote" => FOOTNOTE | CONTAINER,
        "definition_description" => DESCRIPTION | CONTAINER,
        "block_quote" | "div" | "directive" | "admonition" | "figure_group" => CONTAINER,
        "table_row" => TABLE_ROW,
        _ => 0,
    }
}

const OPAQUE: u8 = 1;
const FRONTMATTER: u8 = 2;
const LIST_ITEM: u8 = 4;
const FOOTNOTE: u8 = 8;
const DESCRIPTION: u8 = 16;
const CONTAINER: u8 = 32;
const TABLE_ROW: u8 = 64;
const LITERAL_QUOTE_PREFIX: u8 = 128;

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

fn run(text: &str, chars: &str, minimum: usize) -> bool {
    let mut letters = text.chars();
    let Some(first) = letters.next() else {
        return false;
    };
    text.len() >= minimum && chars.contains(first) && letters.all(|c| c == first)
}

fn spaced_run(text: &str) -> bool {
    let Some(first) = text.chars().next() else {
        return false;
    };
    first != ' '
        && "*-_".contains(first)
        && text.chars().filter(|c| *c == first).count() >= 3
        && text.chars().any(char::is_whitespace)
        && text.chars().all(|c| c == first || c.is_whitespace())
}

pub(super) fn list_marker(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.get(1) == Some(&b' ') && bytes.first().is_some_and(|b| b"-*".contains(b)) {
        return true;
    }
    let digits = bytes
        .iter()
        .take(9)
        .take_while(|b| b.is_ascii_digit())
        .count();
    digits > 0
        && bytes.get(digits).is_some_and(|b| matches!(b, b'.' | b')'))
        && bytes.get(digits + 1) == Some(&b' ')
}

fn starts_block(text: &str) -> bool {
    (text == ">" || ["> ", ":: ", ": "].iter().any(|p| text.starts_with(p)))
        || valid_attribute_line(text)
        || heading_mark(text)
        || (text.starts_with('[') || text.starts_with("*[")) && text.contains("]:")
}

/// C2-B-090: one or more glued valid attribute blocks alone on the line, spaced or with an id or a key (carve#6197).
fn valid_attribute_line(text: &str) -> bool {
    let text = text.trim_end_matches([' ', '\t']);
    text.starts_with('{')
        && crate::attr::parse_glued(text, 0).is_some_and(|(_, end)| end == text.len())
}

/// C2-B-151, C2-B-152: one to six `=` or `#` and a space.
fn heading_mark(text: &str) -> bool {
    let Some(mark) = text.bytes().next().filter(|b| matches!(b, b'#' | b'=')) else {
        return false;
    };
    let run = text.bytes().take_while(|b| *b == mark).count();
    run <= 6 && text.as_bytes().get(run) == Some(&b' ')
}

fn markers(line: &Line, previous: Option<&Line>, sink: &mut Sink<'_>) {
    let content_start = line.content_start.unwrap_or(0);
    let content = line.text.get(content_start..).unwrap_or("").trim_start();
    let (prefix, tail) = marker_tail(&line.text);
    continuation_marker(line, sink);
    quote_list_marker(line, previous, sink);
    thematic_marker(line, previous, prefix, tail, sink);
    marker_as_text(line, content_start, content, sink);
    attribute_line_like(line, sink);
    let raw = sink
        .source
        .text()
        .get(line.start..line.start + line.text.len())
        .unwrap_or("");
    if raw.starts_with('>')
        && raw.len() > 1
        && !raw.starts_with("> ")
        && matches!(
            line.kind.as_str(),
            "paragraph" | "caption" | "definition_term" | "heading" | "cell_paragraph" | "line"
        )
    {
        let end = line.start + raw.trim_end_matches([' ', '\t']).len();
        let text = raw.get(1..).unwrap_or("").trim_end().to_string();
        sink.emit(
            "blockquote-marker-without-space",
            line.start,
            end,
            &line.path,
            &[("text", &text)],
        );
    }
    table_line(line, sink);
}

fn continuation_marker(line: &Line, sink: &mut Sink<'_>) {
    if replaced!("deprecated-continuation-marker") {
        return;
    }
    if !line.text.as_bytes().contains(&b'+') {
        return;
    }
    let (mut start, mut text) = leaf_after_quotes(line);
    let mut item = false;
    while text.starts_with("- ") || text.starts_with("* ") {
        item = true;
        text = &text[2..];
        start += 2;
        let trimmed = text.trim_start_matches([' ', '\t']);
        start += text.len() - trimmed.len();
        text = trimmed;
        // A quote in the item's marker-line text is not another physical-line prefix.
        if text.starts_with("> ") {
            return;
        }
    }
    if text == "+" {
        let message = if item {
            2
        } else {
            usize::from(line.scope.container == ContainerKind::ListItem)
        };
        sink.emit_nth(
            "deprecated-continuation-marker",
            line.start + start,
            line.start + start + 1,
            &line.path,
            &[],
            message,
        );
    }
}

fn thematic_marker(
    line: &Line,
    previous: Option<&Line>,
    prefix: usize,
    tail: &str,
    sink: &mut Sink<'_>,
) {
    if replaced!(
        "deprecated-spaced-thematic-break",
        "setext-heading-underline",
        "thematic-break-spelling"
    ) {
        return;
    }
    let marker_start = prefix + tail.len() - tail.trim_start().len();
    // A list-looking prefix folded into text is not an item marker.
    if prefix > line.text.len() - line.text.trim_start_matches([' ', '\t']).len()
        && line.scope.container != ContainerKind::ListItem
        && !line.paragraph
    {
        return;
    }
    let candidate = tail.trim();
    let after_paragraph =
        previous.is_some_and(|p| p.kind == "paragraph" && !p.text.trim().is_empty());
    if (run(candidate, "=", 3) || run(candidate, "-", 3))
        && after_paragraph
        && !item_marker_underline(line, prefix, candidate, after_paragraph)
    {
        sink.emit(
            "setext-heading-underline",
            line.start + marker_start,
            line.start + marker_start + candidate.len(),
            &line.path,
            &[],
        );
    }
    // carve#6174: the run is read behind the container prefixes, marker lines included.
    let (from, own) = line_text(line);
    // A hyphen run below any paragraph line is left to `setext-heading-underline`, as carve-js reads it.
    let below_paragraph =
        previous.is_some_and(|p| p.kind == "paragraph" && !p.text.trim().is_empty());
    if run(own, "-_", 3) && !(own.starts_with('-') && below_paragraph) {
        sink.emit(
            "thematic-break-spelling",
            line.start + from,
            line.start + from + own.len(),
            &line.path,
            &[("raw", own)],
        );
    }
    let (from, spaced) = spaced_text(line);
    if spaced_run(spaced) {
        sink.emit(
            "deprecated-spaced-thematic-break",
            line.start + from,
            line.start + from + spaced.len(),
            &line.path,
            &[],
        );
    }
}

fn item_marker_underline(
    line: &Line,
    prefix: usize,
    candidate: &str,
    after_paragraph: bool,
) -> bool {
    after_paragraph
        && (run(candidate, "-", 3) || run(candidate, "=", 3))
        && prefix > line.text.len() - line.text.trim_start_matches([' ', '\t']).len()
        && list_marker(line.text.trim_start_matches([' ', '\t']))
}

fn quote_list_marker(line: &Line, previous: Option<&Line>, sink: &mut Sink<'_>) {
    if replaced!("list-marker-without-blank-line") {
        return;
    }
    if line.text.starts_with("> ")
        && !in_caption(line)
        && previous.is_some_and(|p| p.paragraph && p.text.starts_with("> "))
        && line.text.strip_prefix("> ").is_some_and(list_marker)
    {
        sink.emit(
            "list-marker-without-blank-line",
            line.start + 2,
            line.start + 3,
            &line.path,
            &[],
        );
    }
}

fn marker_tail(text: &str) -> (usize, &str) {
    let mut offset = text.len() - text.trim_start_matches([' ', '\t']).len();
    let mut rest = &text[offset..];
    loop {
        let prefix = if rest.starts_with("> ") || rest.starts_with("- ") || rest.starts_with("* ") {
            2
        } else {
            0
        };
        if prefix == 0 {
            return (offset, rest);
        }
        offset += prefix;
        rest = &rest[prefix..];
    }
}

fn marker_as_text(line: &Line, content_start: usize, content: &str, sink: &mut Sink<'_>) {
    if line.kind == "paragraph"
        && line.content_start.is_some()
        && (content.starts_with("~~~") || content.starts_with(":::"))
    {
        let what = if content.starts_with("~~~") {
            "a tilde fence"
        } else {
            "a div fence"
        };
        let marker = line
            .text
            .get(content_start..)
            .and_then(|tail| tail.find(content))
            .map_or(content_start, |at| content_start + at);
        sink.emit(
            "block-marker-as-text",
            line.start + marker,
            line.start + line.text.trim_end().len(),
            &line.path,
            &[("what", what)],
        );
    }
}

fn attribute_line_like(line: &Line, sink: &mut Sink<'_>) {
    let text = line.text.trim();
    let opens = text
        .as_bytes()
        .get(1)
        .is_some_and(|c| c.is_ascii_alphabetic() || matches!(c, b'.' | b'#' | b'_'));
    let complete = crate::attr::parse_glued(text, 0).is_some_and(|(_, end)| end == text.len());
    if line.paragraph && text.starts_with('{') && opens && !complete {
        sink.whole(
            "block-marker-as-text",
            line,
            &[("what", "an attribute line")],
        );
    }
}

fn table_line(line: &Line, sink: &mut Sink<'_>) {
    let (prefix, tail) = marker_tail(&line.text);
    let text = tail.trim_end();
    for (at, _) in line.text.match_indices("|=").filter(|_| line.table_row) {
        if let Some(mark) = line.text.as_bytes().get(at + 2)
            && matches!(mark, b'<' | b'>' | b'~')
            && line.text.as_bytes().get(at + 3) == Some(&b' ')
        {
            let prefix = (*mark as char).to_string();
            let braced = format!("{{{prefix}}}");
            sink.emit(
                "deprecated-cell-alignment",
                line.start + at + 2,
                line.start + at + 3,
                &line.path,
                &[("prefix", &prefix), ("braced", &braced)],
            );
        }
    }
    if text.starts_with('|')
        && text.len() > 1
        && (!text.ends_with('|') || source_escaped(text.as_bytes(), text.len() - 1))
        && !text.contains("%%")
    {
        let owner = match line.kind.as_str() {
            "caption" => "caption",
            "definition_term" => "term",
            _ => "paragraph",
        };
        sink.emit(
            "table-shaped-paragraph",
            line.start + prefix,
            line.start + prefix + text.len(),
            &line.path,
            &[("owner", owner)],
        );
        if text.matches('|').count() >= 2 {
            sink.emit(
                "table-row-without-closing-pipe",
                line.start + prefix,
                line.start + prefix + text.len(),
                &line.path,
                &[],
            );
        }
    }
}

fn legacy(line: &Line, previous: Option<&Line>, prior_block: Option<&Line>, sink: &mut Sink<'_>) {
    legacy_markup(line, previous, prior_block, sink);
    legacy_context(line, previous, sink);
}

fn legacy_markup(
    line: &Line,
    previous: Option<&Line>,
    prior_block: Option<&Line>,
    sink: &mut Sink<'_>,
) {
    legacy_authored(line, sink);
    legacy_layout(line, previous, prior_block, sink);
}

fn legacy_authored(line: &Line, sink: &mut Sink<'_>) {
    let text = line.text.trim();
    if line.paragraph {
        let content = line
            .text
            .get(line.content_start.unwrap_or(0)..)
            .unwrap_or("");
        let offset = line.text.len() - content.trim_start().len();
        let content = content.trim_start();
        if content.starts_with('+')
            && content
                .get(1..)
                .is_some_and(|tail| tail.starts_with([' ', '\t']) && !tail.trim().is_empty())
        {
            sink.emit(
                "djot-plus-bullet",
                line.start + offset,
                line.start + offset + content.trim_end().len(),
                &line.path,
                &[],
            );
        }
    }
    if line.paragraph && (text.starts_with("#region ") || text.starts_with("#endregion")) {
        sink.whole("region-marker-renders-as-text", line, &[]);
    }
    let (at, leaf) = leaf_text(line);
    if line.paragraph && leaf.starts_with("^ ") && line.kind != "caption" {
        sink.emit(
            "unattached-caption",
            line.start + at,
            line.start + at + leaf.len(),
            &line.path,
            &[],
        );
    }
}

fn legacy_layout(
    line: &Line,
    previous: Option<&Line>,
    prior_block: Option<&Line>,
    sink: &mut Sink<'_>,
) {
    let text = line.text.trim();
    if !replaced!("indented-code-block")
        && line.paragraph
        && line.text.starts_with("    ")
        && !text.is_empty()
        && line.path.matches('/').count() <= 2
        && !prior_block.is_some_and(|p| p.scope.container == ContainerKind::ListItem)
        && previous.is_some_and(|p| {
            (p.text.trim().is_empty() || p.text.trim_start().starts_with('{'))
                && p.scope.container != ContainerKind::ListItem
        })
    {
        sink.whole("indented-code-block", line, &[]);
    }
    if text.starts_with("::: >") && previous.is_some_and(|p| p.text.trim_start().starts_with('>')) {
        sink.whole("quote-fence-ends-the-quote-above", line, &[]);
    }
}

/// The text of a line's leaf behind its container prefixes and indent, and where it starts in the line.
fn leaf_text(line: &Line) -> (usize, &str) {
    let (from, tail) = match line.content_start {
        Some(start) => (start, line.text.get(start..).unwrap_or("")),
        None => quote_tail(&line.text, line.quotes),
    };
    let content = tail.trim_start_matches([' ', '\t']);
    (from + tail.len() - content.len(), content.trim_end())
}

/// The text behind the line's container prefixes and indent (C2-B-027), as the tree places it.
fn line_text(line: &Line) -> (usize, &str) {
    match line.content {
        Some(start) => (start, line.text.get(start..).unwrap_or("").trim_end()),
        None => leaf_after_quotes(line),
    }
}

/// A spaced run parses as nested list markers, so it reaches back over the markers it makes itself.
fn spaced_text(line: &Line) -> (usize, &str) {
    if line.content.is_none() {
        return leaf_after_quotes(line);
    }
    let (start, own) = line_text(line);
    let marker = own.bytes().next();
    let bytes = line.text.as_bytes();
    let mut at = start;
    while let Some(&byte) = at.checked_sub(1).and_then(|before| bytes.get(before))
        && (matches!(byte, b' ' | b'\t') || Some(byte) == marker)
    {
        at -= 1;
    }
    while matches!(bytes.get(at), Some(b' ' | b'\t')) {
        at += 1;
    }
    (at, line.text.get(at..).unwrap_or("").trim_end())
}

/// A later line of a leaf carries only quote marks before its text: list markers stand on first lines.
fn quote_tail(text: &str, quotes: usize) -> (usize, &str) {
    let mut rest = text;
    for _ in 0..quotes {
        let trimmed = rest.trim_start_matches([' ', '\t']);
        // C2-B-031: a quote mark is `>` followed by a space or the end of the line.
        match trimmed.strip_prefix('>') {
            Some(tail) if tail.is_empty() || tail.starts_with(' ') => {
                rest = tail.strip_prefix(' ').unwrap_or(tail);
            }
            _ => break,
        }
    }
    (text.len() - rest.len(), rest)
}

/// The description lists and the lines where a term or description starts, for the column rule.
struct DescriptionLists<'r> {
    lists: Vec<&'r Range<'r>>,
    entry_lines: std::collections::HashSet<usize>,
}

impl<'r> DescriptionLists<'r> {
    fn new(ranges: &'r [Range<'r>]) -> Self {
        Self {
            lists: ranges
                .iter()
                .filter(|r| r.kind == "definition_list")
                .collect(),
            entry_lines: ranges
                .iter()
                .filter(|r| matches!(r.kind, "definition_term" | "definition_description"))
                .map(|r| r.start_line)
                .collect(),
        }
    }

    /// C2-B-067, C2-B-071, C2-B-074: a colon mark that opens no entry, below a description list whose first term
    /// mark stands further right; gives the mark's offset in the line, the text and the list's column.
    fn below_list_column<'l>(
        &self,
        line: &'l Line,
        index: usize,
        lines: &[Line],
    ) -> Option<(usize, &'l str, usize, usize)> {
        let (at, text) = leaf_after_quotes(line);
        let mark = text.strip_prefix("::").or_else(|| text.strip_prefix(':'))?;
        if !(mark.is_empty() || mark.starts_with([' ', '\t']))
            || self.entry_lines.contains(&(index + 1))
        {
            return None;
        }
        let host = lines
            .get(..index)?
            .iter()
            .enumerate()
            .rev()
            .find(|(_, l)| !l.text.trim().is_empty() && l.quotes == line.quotes)?;
        let number = host.0 + 1;
        let list = self
            .lists
            .iter()
            .filter(|r| r.start_line <= number && number <= r.end_line)
            .max_by_key(|r| r.path.len())?;
        let list_line = lines.get(list.start_line.checked_sub(1)?)?;
        let host = visual_column(list_line, list.start.checked_sub(list_line.start)?);
        let column = visual_column(line, at);
        (host > column).then_some((at, text, column, host))
    }
}

/// C2-B-004: the column of a byte of the line behind its own quote marks, counting tab stops of four.
fn visual_column(line: &Line, at: usize) -> usize {
    let (from, _) = quote_tail(&line.text, line.quotes);
    let column = |to: usize| {
        line.text
            .get(..to)
            .unwrap_or("")
            .chars()
            .fold(0, |c, ch| if ch == '\t' { c + 4 - c % 4 } else { c + 1 })
    };
    column(at.max(from)) - column(from)
}

/// The text behind the line's own quote marks and indent, and where it starts in the line.
fn leaf_after_quotes(line: &Line) -> (usize, &str) {
    let (from, tail) = quote_tail(&line.text, line.quotes);
    let content = tail.trim_start_matches([' ', '\t']);
    (from + tail.len() - content.len(), content.trim_end())
}

fn description_column(
    line: &Line,
    at: usize,
    text: &str,
    column: usize,
    host: usize,
    sink: &mut Sink<'_>,
) {
    let name = if text.starts_with("::") {
        "term"
    } else {
        "description"
    };
    let column = (column + 1).to_string();
    let host = (host + 1).to_string();
    sink.emit(
        "description-separator-below-content-column",
        line.start + at,
        line.start + at + text.len(),
        &line.path,
        &[("name", name), ("column", &column), ("host.column", &host)],
    );
}

// C2-B-074: a later paragraph line below a line that is no `::` term.
fn markdown_definition(line: &Line, previous: Option<&Line>, sink: &mut Sink<'_>) {
    let (at, text) = leaf_text(line);
    let Some(previous) = previous else { return };
    if line.kind != "paragraph"
        || previous.path != line.path
        || previous.text.trim().is_empty()
        || !text.starts_with(": ")
        || leaf_text(previous).1.starts_with(":: ")
    {
        return;
    }
    sink.emit(
        "markdown-definition-paragraph",
        line.start + at,
        line.start + at + text.len(),
        &line.path,
        &[],
    );
}

fn legacy_context(line: &Line, previous: Option<&Line>, sink: &mut Sink<'_>) {
    if line.paragraph
        && previous.is_some_and(|p| p.text.starts_with("# "))
        && !line.text.starts_with(' ')
    {
        if let Some(previous) = previous {
            sink.emit(
                "djot-heading-continuation",
                previous.start,
                line.start + line.text.trim_end().len(),
                &previous.path,
                &[],
            );
        }
    }
    definition_in_container(line, sink);
    term_folded(line, sink);
}

// C2-B-081: a definition-shaped line behind container marks, a list marker or description mark included.
fn definition_in_container(line: &Line, sink: &mut Sink<'_>) {
    if replaced!("definition-in-container") {
        return;
    }
    let (at, text) = leaf_text(line);
    if !line.paragraph
        || (line.kind == "definition_term" && line.text.starts_with('['))
        || !text.starts_with('[')
        || line.scope.in_footnote
        || !(line.text.starts_with([' ', '\t']) || line.scope.container != ContainerKind::None)
    {
        return;
    }
    let Some(close) = text
        .find("]:")
        .filter(|close| *close > 1 && close + 2 < text.len())
    else {
        return;
    };
    sink.emit(
        "definition-in-container",
        line.start + at,
        line.start + at + close + 2,
        &line.path,
        &[],
    );
}

// C2-B-075: a quote mark or list marker opening a term's text, on the term's first line only.
fn term_folded(line: &Line, sink: &mut Sink<'_>) {
    if replaced!("definition-term-block-folded") {
        return;
    }
    if line.kind != "definition_term" || line.content_start.is_none() {
        return;
    }
    let (at, text) = leaf_text(line);
    let body = match text.strip_prefix("::") {
        Some(rest) if rest.starts_with([' ', '\t']) => rest.trim_start_matches([' ', '\t']),
        Some(_) => return,
        None => text,
    };
    let spaced = |marker: &str| {
        body.strip_prefix(marker)
            .is_some_and(|t| t.starts_with([' ', '\t']))
    };
    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
    let ordered = (1..=9).contains(&digits)
        && matches!(body.as_bytes().get(digits), Some(b'.' | b')'))
        && matches!(body.as_bytes().get(digits + 1), Some(b' ' | b'\t'));
    let marker = if spaced(">") || spaced("-") || spaced("*") {
        1
    } else if ordered {
        digits + 1
    } else {
        return;
    };
    let from = at + text.len() - body.len();
    sink.emit(
        "definition-term-block-folded",
        line.start + from,
        line.start + from + marker,
        &line.path,
        &[],
    );
}

fn prose(line: &Line, index: usize, previous: Option<&Line>, lines: &[Line], sink: &mut Sink<'_>) {
    prose_tokens(line, previous, sink);
    prose_context(line, index, previous, lines, sink);
}

fn prose_tokens(line: &Line, previous: Option<&Line>, sink: &mut Sink<'_>) {
    let text = line.text.as_str();
    let (from, trimmed) = line_text(line);
    if !replaced!("blank-reference-label") {
        blank_reference_label(line, sink);
    }
    if line.paragraph
        && trimmed.starts_with('[')
        && trimmed.ends_with("]:")
        && trimmed.len() >= 4
        && {
            let label = trimmed[1..trimmed.len() - 2].trim();
            !label.is_empty()
                && !label.contains(']')
                && !label
                    .strip_prefix('^')
                    .is_some_and(|rest| rest.trim().is_empty())
        }
    {
        let message = usize::from(trimmed[1..].starts_with('^'));
        sink.emit_nth(
            "empty-reference-definition",
            line.start + from,
            line.start + from + trimmed.len(),
            &line.path,
            &[],
            message,
        );
    }
    unattached_attribute(line, previous, sink);
    if let Some(at) = text.find("<!--")
        && line.paragraph
        && !source_escaped(text.as_bytes(), at)
    {
        if let Some(close) = text.get(at + 4..).and_then(|s| s.find("-->")) {
            sink.emit(
                "html-comment-renders-as-text",
                line.start + at,
                line.start + at + close + 7,
                &line.path,
                &[],
            );
        }
    }
    link_gap(line, sink);
    empty_include_path(line, sink);
}

// C2-I-079: a footnote label, a full reference's label or a definition label of only spaces and tabs;
// a task box and a bracket pair with no reference glued to it are no label.
fn blank_reference_label(line: &Line, sink: &mut Sink<'_>) {
    let text = line.text.as_str();
    let (from, _) = leaf_text(line);
    // Every `[` before one `]` shares it: found once, not once per `[`.
    let mut next_close: Option<Option<usize>> = None;
    for (open, _) in text.match_indices('[').filter(|(at, _)| *at >= from) {
        if next_close.is_none_or(|found| found.is_some_and(|at| at <= open)) {
            next_close = Some(text[open + 1..].find(']').map(|at| open + 1 + at));
        }
        let Some(Some(close)) = next_close else {
            continue;
        };
        let inner = &text[open + 1..close];
        let footnote = inner.starts_with('^');
        if line.literal_quote_prefix && !footnote {
            continue;
        }
        let label = inner.strip_prefix('^').unwrap_or(inner);
        if label.is_empty() || !label.bytes().all(|b| b == b' ' || b == b'\t') {
            continue;
        }
        let full = open.checked_sub(1).and_then(|at| text.as_bytes().get(at)) == Some(&b']');
        let definition = open == from && text[close + 1..].starts_with(':');
        if footnote || full || definition {
            let end = close + 1 + usize::from(definition && !footnote && !full);
            sink.emit(
                "blank-reference-label",
                line.start + open,
                line.start + end,
                &line.path,
                &[],
            );
        }
    }
}

fn empty_include_path(line: &Line, sink: &mut Sink<'_>) {
    if replaced!("empty-include-path") {
        return;
    }
    let text = line.text.as_str();
    if let Some(at) = text.find("{{")
        && line.paragraph
        && let Some(end) = text.get(at + 2..).and_then(|s| s.find("}}"))
        && text.get(at + 2..at + 2 + end).is_some_and(|s| {
            let path = s.trim();
            path.is_empty()
                || matches!(path, "\"\"" | "''")
                || path.starts_with('@')
                || (s.starts_with(char::is_whitespace) && path.starts_with('#'))
        })
    {
        let token = text.get(at..at + end + 4).unwrap_or("");
        sink.emit(
            "empty-include-path",
            line.start + at,
            line.start + at + end + 4,
            &line.path,
            &[("text", token)],
        );
    }
}

fn link_gap(line: &Line, sink: &mut Sink<'_>) {
    if !line.paragraph {
        return;
    }
    let text = line.text.as_str();
    let next = |from: usize, byte: u8| {
        text.as_bytes()
            .get(from..)
            .and_then(|rest| rest.iter().position(|b| *b == byte))
            .map(|at| from + at)
    };
    // Every `[` before one `]` shares that bracket and what follows it, and the `)` searches only move right,
    // so each bracket, gap and destination is scanned once rather than once per `[`.
    let mut close: Option<Option<usize>> = None;
    let mut paren: Option<(usize, Option<usize>)> = None;
    let mut settled: (usize, Option<usize>) = (usize::MAX, None);
    for (open, _) in text.match_indices('[') {
        if open > 0 && text.as_bytes().get(open - 1) == Some(&b'\\') {
            continue;
        }
        if close.is_none_or(|found| found.is_some_and(|at| at <= open)) {
            close = Some(next(open + 1, b']'));
        }
        let Some(Some(close)) = close else {
            continue;
        };
        if close == open + 1 {
            continue;
        }
        if settled.0 != close {
            let gap = text[close + 1..]
                .bytes()
                .take_while(u8::is_ascii_whitespace)
                .count();
            let from = close + gap + 2;
            let end = if gap == 0 || text.as_bytes().get(close + 1 + gap) != Some(&b'(') {
                None
            } else {
                if paren.is_none_or(|(searched, found)| {
                    found.is_some_and(|at| at < from) || searched > from
                }) {
                    paren = Some((from, next(from, b')')));
                }
                paren
                    .and_then(|(_, found)| found)
                    .filter(|&end| !text[from..end].trim().is_empty())
            };
            settled = (close, end);
        }
        let Some(end) = settled.1 else {
            continue;
        };
        sink.emit(
            "link-gap-before-destination",
            line.start + open,
            line.start + end + 1,
            &line.path,
            &[],
        );
    }
}

fn prose_context(
    line: &Line,
    index: usize,
    previous: Option<&Line>,
    lines: &[Line],
    sink: &mut Sink<'_>,
) {
    if replaced!(
        "bare-html-tag",
        "block-opener-without-blank-line",
        "caption-looking-paragraph",
        "description-term-multi-line",
        "github-alert-marker",
        "list-marker-without-blank-line"
    ) {
        return;
    }
    let (marker_at, marker_text) = leaf_after_quotes(line);
    let list_candidate = list_marker(marker_text);
    if term_continuation_marker(line, previous, marker_at, list_candidate) {
        sink.emit(
            "description-term-multi-line",
            line.start + marker_at,
            line.start + line.text.trim_end().len(),
            &line.path,
            &[],
        );
    }
    if paragraph_continuation(line, previous) {
        if quote_opener_candidate(line, sink.source.text()) {
            let (at, _) = line_text(line);
            sink.emit(
                "block-opener-without-blank-line",
                line.start + at,
                line.start + at + 1,
                &line.path,
                &[("opener", "a quote")],
            );
        }
        prose_block_opener(line, previous, sink);
    }
    if list_candidate
        && matches!(line.kind.as_str(), "paragraph" | "definition_term")
        && !in_caption(line)
        && (paragraph_continuation(line, previous)
            || list_after_open_paragraph(line, previous)
            || previous.is_some_and(|p| {
                p.kind == "definition_term" && !p.text.trim().is_empty() && line.scope.in_footnote
            }))
    {
        sink.emit(
            "list-marker-without-blank-line",
            line.start + marker_at,
            line.start + marker_at + marker_text.len(),
            &line.path,
            &[],
        );
    }
    bare_html(line, sink);
    github_alert(line, sink);
    caption_looking(line, index, lines, sink);
}

fn term_continuation_marker(
    line: &Line,
    previous: Option<&Line>,
    marker_at: usize,
    list_candidate: bool,
) -> bool {
    line.scope.container == ContainerKind::ListItem
        && line.kind != "definition_term"
        && list_candidate
        && previous.is_some_and(|p| {
            p.kind == "definition_term"
                && p.content_start.is_some()
                && p.scope.container == ContainerKind::ListItem
                && visual_column(line, marker_at)
                    > visual_column(
                        p,
                        p.text.len() - p.text.trim_start_matches([' ', '\t']).len(),
                    )
        })
}

fn list_after_open_paragraph(line: &Line, previous: Option<&Line>) -> bool {
    previous.is_some_and(|p| {
        p.kind == "paragraph"
            && p.paragraph_in_div
            && !p.text.trim().is_empty()
            && line.scope.container == ContainerKind::ListItem
            && p.content.is_some_and(|content| {
                visual_column(line, leaf_after_quotes(line).0) >= visual_column(p, content)
            })
            && p.path
                .rsplit_once("/children/")
                .is_some_and(|(parent, _)| !parent.is_empty() && line.path.starts_with(parent))
    })
}

fn quote_opener_candidate(line: &Line, source: &str) -> bool {
    if !matches!(line.kind.as_str(), "paragraph" | "caption") {
        return false;
    }
    let (at, text) = line_text(line);
    if !text.starts_with('>') {
        return false;
    }
    if text == ">" && empty_quote_with_space(line, source) {
        return false;
    }
    source
        .get(line.start + at..line.start + line.text.len())
        .is_some_and(|raw| {
            raw == ">"
                || raw
                    .strip_prefix("> ")
                    .is_some_and(|tail| !tail.trim().is_empty())
        })
}

fn empty_quote_with_space(line: &Line, source: &str) -> bool {
    source
        .get(line.start..)
        .and_then(|tail| tail.split('\n').next())
        .is_some_and(|raw| raw.trim_start_matches([' ', '\t']) == "> ")
}

fn empty_spaced_quote_content(line: &Line, content: &str, source: &str) -> bool {
    content == ">" && empty_quote_with_space(line, source)
}

fn paragraph_continuation(line: &Line, previous: Option<&Line>) -> bool {
    line.paragraph
        && previous.is_some_and(|p| p.paragraph && p.path == line.path && !p.text.trim().is_empty())
}

fn block_opener_width(trimmed: &str) -> usize {
    if trimmed.starts_with("::") {
        2
    } else if trimmed.starts_with(['#', '=']) {
        trimmed
            .bytes()
            .take_while(|byte| matches!(byte, b'#' | b'='))
            .count()
    } else if trimmed.starts_with('{') {
        trimmed.find('}').map_or(1, |end| end + 1)
    } else if trimmed.starts_with("*[") {
        trimmed.find("]:").map_or(2, |end| end + 2)
    } else if trimmed.starts_with('[') {
        trimmed.find("]:").map_or(1, |end| end + 2)
    } else {
        1
    }
}

/// In a quote the mark on both lines is the quote's own; a lazy line carries no mark of the quote before
/// its content, so its `>` is the candidate (carve#6178).
fn quote_continues(line: &Line, previous: Option<&Line>, prefix: usize, content: &str) -> bool {
    let own_mark = line.content.is_none()
        || line
            .text
            .get(..prefix)
            .is_some_and(|before| before.contains('>'));
    content.starts_with("> ")
        && own_mark
        && line.quotes > 0
        && previous.is_some_and(|p| p.text.trim_start().starts_with("> "))
}

fn prose_block_opener(line: &Line, previous: Option<&Line>, sink: &mut Sink<'_>) {
    if replaced!("block-opener-without-blank-line") {
        return;
    }
    // C2-B-019, C2-B-200: only a paragraph or caption line is a candidate; a term's later line is not.
    if !matches!(
        line.kind.as_str(),
        "paragraph" | "caption" | "cell_paragraph"
    ) {
        return;
    }
    let (prefix, content) = line_text(line);
    if !starts_block(content)
        || empty_spaced_quote_content(line, content, sink.source.text())
        || (content.starts_with('{') && !content.ends_with('}'))
        || (content.starts_with('[') && line.scope.in_footnote)
        || quote_continues(line, previous, prefix, content)
    {
        return;
    }
    let opener = if content.starts_with(['#', '=']) {
        "a heading"
    } else if content.starts_with('>') {
        "a quote"
    } else if content.starts_with('{') {
        "an attribute line"
    } else if content.starts_with("::") {
        "a term"
    } else if content.starts_with(':') {
        "a description"
    } else {
        "a definition"
    };
    let marker = block_opener_width(content);
    sink.emit(
        "block-opener-without-blank-line",
        line.start + prefix,
        line.start + prefix + marker,
        &line.path,
        &[("opener", opener)],
    );
}

fn unattached_attribute(line: &Line, previous: Option<&Line>, sink: &mut Sink<'_>) {
    if replaced!("unattached-block-attribute") {
        return;
    }
    // C2-W-107: a later line of a paragraph is its text, which block-opener-without-blank-line reports (carve#6195).
    if previous.is_some_and(|previous| previous.paragraph && previous.path == line.path) {
        return;
    }
    let (at, text) = leaf_text(line);
    let tail = sink
        .source
        .text()
        .get(line.start + line.text.len()..)
        .unwrap_or("");
    let following = tail.strip_prefix('\n').unwrap_or(tail);
    let next = following.split('\n').next().unwrap_or("").trim();
    if !line.paragraph
        || line.content_start.is_none()
        || (line.content_start.is_none() && (next.starts_with(":::") || next.starts_with('>')))
        || line.attached_attribute
        || !valid_attribute_line(text)
    {
        return;
    }
    let after_run = following
        .lines()
        .find(|candidate| !valid_attribute_line(candidate.trim()))
        .unwrap_or("");
    sink.emit_nth(
        "unattached-block-attribute",
        line.start + at,
        line.start + at + text.len(),
        &line.path,
        &[],
        usize::from(after_run.trim().is_empty() && !following.is_empty()),
    );
}

fn bare_html(line: &Line, sink: &mut Sink<'_>) {
    if replaced!("bare-html-tag") {
        return;
    }
    if !matches!(
        line.kind.as_str(),
        "paragraph" | "heading" | "cell_paragraph"
    ) && !line.table_row
    {
        return;
    }
    let Some(at) = line.text.find('<') else {
        return;
    };
    if in_caption(line) {
        return;
    }
    if source_escaped(line.text.as_bytes(), at) {
        return;
    }
    if line.text.get(at..).is_some_and(|s| s.starts_with("<!--")) {
        return;
    }
    let Some(after) = line.text.as_bytes().get(at + 1) else {
        return;
    };
    if !after.is_ascii_alphabetic() && *after != b'/' {
        return;
    }
    let Some(end) = line.text.get(at + 2..).and_then(|s| s.find('>')) else {
        return;
    };
    let inside = line.text.get(at + 1..at + 2 + end).unwrap_or("");
    if inside.contains("://") || inside.contains('@') || inside.starts_with("/#") {
        return;
    }
    sink.emit(
        "bare-html-tag",
        line.start + at,
        line.start + at + end + 3,
        &line.path,
        &[],
    );
}

fn in_caption(line: &Line) -> bool {
    line.path.split('/').any(|part| part == "caption")
}

fn source_escaped(text: &[u8], at: usize) -> bool {
    text.get(..at)
        .unwrap_or(&[])
        .iter()
        .rev()
        .take_while(|byte| **byte == b'\\')
        .count()
        % 2
        == 1
}

fn github_alert(line: &Line, sink: &mut Sink<'_>) {
    if replaced!("github-alert-marker") {
        return;
    }
    let starts = line.text.trim_start();
    // GitHub also writes the alert mark immediately after `>`; this is still a
    // useful migration warning even though Carve reads the glued line as text.
    if !starts.starts_with("> ") && !starts.starts_with(">[") {
        return;
    }
    if !matches!(
        line.kind.as_str(),
        "paragraph" | "heading" | "cell_paragraph"
    ) && !line.table_row
    {
        return;
    }
    if let Some(at) = line.text.find("[!")
        && line.text.get(at..).is_some_and(|s| {
            [
                "[!NOTE]",
                "[!TIP]",
                "[!IMPORTANT]",
                "[!WARNING]",
                "[!CAUTION]",
            ]
            .iter()
            .any(|token| s.starts_with(token))
        })
    {
        sink.emit(
            "github-alert-marker",
            line.start + at,
            line.start + line.text.len(),
            &line.path,
            &[],
        );
    }
}

fn caption_looking(line: &Line, index: usize, lines: &[Line], sink: &mut Sink<'_>) {
    if replaced!("caption-looking-paragraph") {
        return;
    }
    let start = line.content_start.unwrap_or(0);
    let tail = line.text.get(start..).unwrap_or("");
    let marker = start + tail.len() - tail.trim_start().len();
    if !line.paragraph || !caption_like(tail.trim_start()) {
        return;
    }
    let below = lines
        .iter()
        .skip(index + 1)
        .take(2)
        .any(|other| pipe_table_candidate(leaf_after_quotes(other).1));
    let above = lines.iter().take(index).rev().take(2).any(|other| {
        let text = other.text.trim_start();
        pipe_table_candidate(text) || text.contains("![")
    });
    if below || above {
        sink.emit_nth(
            "caption-looking-paragraph",
            line.start + marker,
            line.start + marker + 1,
            &line.path,
            &[],
            usize::from(!below),
        );
    }
}

fn pipe_table_candidate(text: &str) -> bool {
    text.starts_with('|') && text[1..].contains('|')
}

fn caption_like(text: &str) -> bool {
    let text = text.strip_prefix('*').unwrap_or(text).trim_start();
    if text.starts_with(": ") && !text[2..].trim().is_empty() {
        return true;
    }
    for name in ["Caption", "Table", "Tabelle", "Figure", "Image"] {
        let Some(prefix) = text.get(..name.len()) else {
            continue;
        };
        if !prefix.eq_ignore_ascii_case(name) {
            continue;
        }
        let rest = &text[name.len()..];
        if rest.trim_start().starts_with(':') {
            return true;
        }
        if rest.starts_with(char::is_whitespace) {
            let digits = rest
                .trim_start()
                .bytes()
                .take_while(u8::is_ascii_digit)
                .count();
            if digits > 0 && rest.trim_start()[digits..].trim_start().starts_with(':') {
                return true;
            }
        }
    }
    false
}
