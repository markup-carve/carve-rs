//! Lint of source lines after the 0.2 parse has marked paragraphs and opaque spans.

use super::{LintOptions, Range, active, drop_replaced};
use crate::diagnostic::Diagnostic;
use crate::source::Source;

struct Line {
    text: String,
    start: usize,
    path: String,
    kind: String,
    paragraph: bool,
    content_start: Option<usize>,
    opacity: Opacity,
    attached_attribute: bool,
    scope: Scope,
}

struct Scope {
    in_footnote: bool,
    in_description: bool,
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
    ranges: &[Range],
    output: &mut Vec<Diagnostic>,
) {
    let lines = lines(source, ranges);
    let mut sink = Sink {
        source: Source::new(source),
        options,
        output,
        replacing: Vec::new(),
    };
    for (index, line) in lines.iter().enumerate() {
        if line.opacity != Opacity::None {
            if line.opacity == Opacity::Frontmatter {
                frontmatter_quote_marker(line, &mut sink);
            }
            continue;
        }
        let previous = index.checked_sub(1).and_then(|i| lines.get(i));
        markers(line, previous, &mut sink);
        legacy(line, previous, &mut sink);
        prose(line, index, previous, &lines, &mut sink);
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
    let pos = sink.source.pos(line.start, line.start + 1);
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

fn lines(source: &str, ranges: &[Range]) -> Vec<Line> {
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
        for range in fact.masks {
            let from = range.start.saturating_sub(start).min(bytes.len());
            let to = range.end.saturating_sub(start).min(bytes.len());
            bytes
                .get_mut(from..to)
                .into_iter()
                .flatten()
                .for_each(|b| *b = 0);
        }
        out.push(Line {
            text: String::from_utf8_lossy(&bytes).into_owned(),
            start,
            path: paragraph.map_or_else(String::new, |r| r.path.clone()),
            kind: paragraph.map_or_else(String::new, |r| r.kind.clone()),
            paragraph: paragraph.is_some(),
            content_start: paragraph
                .filter(|range| range.start_line == number)
                .map(|range| range.start.saturating_sub(start).min(raw.len())),
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
                in_description: fact.flags & DESCRIPTION != 0,
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
            start: source.len(),
            path: String::new(),
            kind: String::new(),
            paragraph: false,
            content_start: None,
            opacity: Opacity::None,
            attached_attribute: false,
            scope: Scope {
                in_footnote: false,
                in_description: false,
                container: ContainerKind::None,
            },
        });
    }
    out
}

fn line_facts<'a>(starts: &[usize], raws: &[&str], ranges: &'a [Range]) -> Vec<LineFacts<'a>> {
    let count = raws.len();
    // One pass per range over the lines it covers, instead of every range per line.
    let mut facts = vec![LineFacts::default(); count];
    for range in ranges {
        let kind = range.kind.as_str();
        let paragraph_like = matches!(kind, "paragraph" | "caption" | "definition_term");
        let flags = range_flags(kind);
        if paragraph_like || flags != 0 {
            let first = range.start_line.max(1) - 1;
            let covered = facts.get_mut(first..range.end_line.min(count));
            for fact in covered.into_iter().flatten() {
                fact.flags |= flags;
                // `max_by_key` keeps the last of equal keys.
                if paragraph_like
                    && fact.paragraph.is_none_or(|best| {
                        (range.kind == "caption", range.path.len())
                            >= (best.kind == "caption", best.path.len())
                    })
                {
                    fact.paragraph = Some(range);
                }
            }
        }
        if is_opaque_leaf(kind) {
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
    }
    facts
}

fn range_flags(kind: &str) -> u8 {
    match kind {
        "code_block" | "raw_block" => OPAQUE,
        "frontmatter" => OPAQUE | FRONTMATTER,
        "list_item" => LIST_ITEM | CONTAINER,
        "footnote" => FOOTNOTE | CONTAINER,
        "definition_description" => DESCRIPTION | CONTAINER,
        "block_quote" | "div" => CONTAINER,
        _ => 0,
    }
}

const OPAQUE: u8 = 1;
const FRONTMATTER: u8 = 2;
const LIST_ITEM: u8 = 4;
const FOOTNOTE: u8 = 8;
const DESCRIPTION: u8 = 16;
const CONTAINER: u8 = 32;

#[derive(Clone, Default)]
struct LineFacts<'r> {
    flags: u8,
    paragraph: Option<&'r Range>,
    masks: Vec<&'r Range>,
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
    ["# ", "= ", "> ", ":: ", ": ", "{."]
        .iter()
        .any(|p| text.starts_with(p))
        || text.starts_with('[') && text.contains("]:")
}

fn markers(line: &Line, previous: Option<&Line>, sink: &mut Sink<'_>) {
    let text = line.text.trim();
    let content_start = line.content_start.unwrap_or(0);
    let content = line.text.get(content_start..).unwrap_or("").trim_start();
    let (prefix, tail) = marker_tail(&line.text);
    continuation_marker(line, prefix, tail, sink);
    quote_list_marker(line, previous, sink);
    thematic_marker(line, previous, text, prefix, tail, sink);
    marker_as_text(line, content_start, content, sink);
    attribute_line_like(line, sink);
    if line.text.starts_with('>')
        && text.len() > 1
        && !text.starts_with("> ")
        && !text.starts_with(">\t")
    {
        sink.emit(
            "blockquote-marker-without-space",
            line.start,
            line.start + 1,
            &line.path,
            &[("text", text.get(1..).unwrap_or(""))],
        );
    }
    table_line(line, sink);
}

fn continuation_marker(line: &Line, prefix: usize, tail: &str, sink: &mut Sink<'_>) {
    if tail.trim_end() == "+" {
        let message = if line.text.trim_start().starts_with("- +")
            || line.text.trim_start().starts_with("* +")
        {
            2
        } else {
            usize::from(line.scope.container == ContainerKind::ListItem)
        };
        sink.emit_nth(
            "deprecated-continuation-marker",
            line.start + prefix,
            line.start + prefix + 1,
            &line.path,
            &[],
            message,
        );
    }
}

fn thematic_marker(
    line: &Line,
    previous: Option<&Line>,
    text: &str,
    prefix: usize,
    tail: &str,
    sink: &mut Sink<'_>,
) {
    let marker_start = prefix + tail.len() - tail.trim_start().len();
    let candidate = tail.trim();
    if run(candidate, "-_", 3) {
        let is_setext = candidate.starts_with('-')
            && previous
                .is_some_and(|p| p.paragraph && p.path == line.path && !p.text.trim().is_empty());
        if is_setext {
            sink.emit(
                "setext-heading-underline",
                line.start + marker_start,
                line.start + marker_start + candidate.len(),
                &line.path,
                &[],
            );
        } else {
            sink.emit(
                "thematic-break-spelling",
                line.start + marker_start,
                line.start + marker_start + candidate.len(),
                &line.path,
                &[("raw", candidate)],
            );
        }
    }
    if spaced_run(text) {
        sink.whole("deprecated-spaced-thematic-break", line, &[]);
    }
}

fn quote_list_marker(line: &Line, previous: Option<&Line>, sink: &mut Sink<'_>) {
    if line.text.starts_with("> ")
        && previous.is_some_and(|p| p.text.starts_with("> "))
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
    if line.paragraph && (content.starts_with("~~~") || content.starts_with(":::")) {
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
    for (at, _) in line.text.match_indices("|=") {
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

fn legacy(line: &Line, previous: Option<&Line>, sink: &mut Sink<'_>) {
    legacy_markup(line, previous, sink);
    legacy_context(line, previous, sink);
    legacy_separator(line, previous, sink);
}

fn legacy_markup(line: &Line, previous: Option<&Line>, sink: &mut Sink<'_>) {
    legacy_authored(line, sink);
    legacy_layout(line, previous, sink);
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
    if line.paragraph && text.starts_with("^ ") && line.kind != "caption" {
        sink.whole("unattached-caption", line, &[]);
    }
}

fn legacy_layout(line: &Line, previous: Option<&Line>, sink: &mut Sink<'_>) {
    let text = line.text.trim();
    if line.paragraph && text.starts_with(": ") && !line.scope.in_description {
        sink.whole("markdown-definition-paragraph", line, &[]);
    }
    if line.paragraph
        && line.text.starts_with("    ")
        && !text.is_empty()
        && line.path.matches('/').count() <= 2
        && previous
            .is_some_and(|p| p.text.trim().is_empty() || p.text.trim_start().starts_with('{'))
    {
        sink.whole("indented-code-block", line, &[]);
    }
    if text.starts_with("::: >") && previous.is_some_and(|p| p.text.trim_start().starts_with('>')) {
        sink.whole("quote-fence-ends-the-quote-above", line, &[]);
    }
}

fn legacy_context(line: &Line, previous: Option<&Line>, sink: &mut Sink<'_>) {
    let text = line.text.trim();
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
    if line.paragraph
        && line.text.find('[').is_some_and(|at| {
            line.text[..at]
                .bytes()
                .all(|byte| matches!(byte, b' ' | b'\t' | b'>'))
        })
        && text
            .find("]:")
            .is_some_and(|at| at > 1 && at + 2 < text.len())
        && (line.text.starts_with([' ', '\t']) || line.scope.container != ContainerKind::None)
        && let Some(start) = line.text.find('[')
        && let Some(end) = line.text.get(start..).and_then(|rest| rest.find("]:"))
    {
        sink.emit(
            "definition-in-container",
            line.start + start,
            line.start + start + end + 2,
            &line.path,
            &[],
        );
    }
    term_folded(line, previous, sink);
}

fn term_folded(line: &Line, previous: Option<&Line>, sink: &mut Sink<'_>) {
    let text = line.text.trim();
    if line.kind == "definition_term"
        && previous.is_some_and(|p| p.path == line.path)
        && starts_block(text)
        && line.text == text
    {
        sink.whole("definition-term-block-folded", line, &[]);
    }
    if line.kind == "definition_term"
        && let Some(body) = text.strip_prefix(":: ")
        && (body.starts_with("> ") || list_marker(body))
    {
        let prefix = line.text.find(":: ").unwrap_or(0) + 3;
        let marker = body.find(' ').unwrap_or(body.len());
        sink.emit(
            "definition-term-block-folded",
            line.start + prefix,
            line.start + prefix + marker,
            &line.path,
            &[],
        );
    }
}

fn legacy_separator(line: &Line, previous: Option<&Line>, sink: &mut Sink<'_>) {
    let text = line.text.trim();
    if let Some(previous) = previous
        && line.paragraph
        && (text.starts_with(": ") || text.starts_with(":: "))
    {
        let prior = previous.text.trim_start();
        let prior_indent = previous.text.len() - prior.len();
        let indent = line.text.len() - line.text.trim_start().len();
        if (prior.starts_with(": ") || prior.starts_with(":: ")) && indent > prior_indent {
            let column = (indent + 1).to_string();
            let host = (prior_indent + 1).to_string();
            let name = if text.starts_with(":: ") {
                "term"
            } else {
                "description"
            };
            sink.whole(
                "description-separator-below-content-column",
                line,
                &[("name", name), ("column", &column), ("host.column", &host)],
            );
        }
    }
}

fn prose(line: &Line, index: usize, previous: Option<&Line>, lines: &[Line], sink: &mut Sink<'_>) {
    prose_tokens(line, sink);
    prose_context(line, index, previous, lines, sink);
}

fn prose_tokens(line: &Line, sink: &mut Sink<'_>) {
    let text = line.text.as_str();
    let trimmed = text.trim();
    blank_reference_label(line, sink);
    if line.paragraph
        && trimmed.starts_with('[')
        && trimmed.ends_with("]:")
        && trimmed.len() >= 4
        && {
            let label = trimmed[1..trimmed.len() - 2].trim();
            !label.is_empty()
                && !label
                    .strip_prefix('^')
                    .is_some_and(|rest| rest.trim().is_empty())
        }
    {
        let prefix = text.len() - text.trim_start().len();
        let message = usize::from(trimmed[1..].starts_with('^'));
        sink.emit_nth(
            "empty-reference-definition",
            line.start + prefix,
            line.start + prefix + trimmed.len(),
            &line.path,
            &[],
            message,
        );
    }
    unattached_attribute(line, sink);
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

fn blank_reference_label(line: &Line, sink: &mut Sink<'_>) {
    let text = line.text.as_str();
    // A task box (C2-B-046) sits before the paragraph's own start.
    let from = line.content_start.unwrap_or(0);
    for (open, _) in text.match_indices('[').filter(|(at, _)| *at >= from) {
        let Some(close) = text[open + 1..].find(']').map(|at| open + 1 + at) else {
            continue;
        };
        let label = text[open + 1..close]
            .strip_prefix('^')
            .unwrap_or(&text[open + 1..close]);
        if label.trim().is_empty() {
            sink.emit(
                "blank-reference-label",
                line.start + open,
                line.start + close + 1,
                &line.path,
                &[],
            );
        }
    }
}

fn empty_include_path(line: &Line, sink: &mut Sink<'_>) {
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
    for (open, _) in text.match_indices('[') {
        if open > 0 && text.as_bytes().get(open - 1) == Some(&b'\\') {
            continue;
        }
        let Some(close) = text[open + 1..].find(']').map(|at| open + 1 + at) else {
            continue;
        };
        if close == open + 1 {
            continue;
        }
        let gap = text[close + 1..]
            .bytes()
            .take_while(u8::is_ascii_whitespace)
            .count();
        if gap == 0 || text.as_bytes().get(close + 1 + gap) != Some(&b'(') {
            continue;
        }
        let Some(end) = text[close + gap + 2..]
            .find(')')
            .map(|at| close + gap + 2 + at)
        else {
            continue;
        };
        if text[close + gap + 2..end].trim().is_empty() {
            continue;
        }
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
    let trimmed = line.text.trim();
    if paragraph_continuation(line, previous) {
        prose_block_opener(line, previous, sink);
        if list_marker(trimmed) {
            sink.whole("list-marker-without-blank-line", line, &[]);
        }
    }
    bare_html(line, sink);
    github_alert(line, sink);
    caption_looking(line, index, lines, sink);
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
    } else if trimmed.starts_with('[') {
        trimmed.find("]:").map_or(1, |end| end + 2)
    } else {
        1
    }
}

fn prose_block_opener(line: &Line, previous: Option<&Line>, sink: &mut Sink<'_>) {
    let (prefix, content) = marker_tail(&line.text);
    let content = content.trim_start();
    let prefix = prefix + line.text[prefix..].len() - line.text[prefix..].trim_start().len();
    if !starts_block(content)
        || (content.starts_with('{') && !content.ends_with('}'))
        || (content.starts_with('[') && line.scope.in_footnote)
        || (content.starts_with("> ")
            && previous.is_some_and(|p| p.text.trim_start().starts_with("> ")))
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

fn unattached_attribute(line: &Line, sink: &mut Sink<'_>) {
    let text = line.text.trim();
    if !line.paragraph
        || line.attached_attribute
        || !text.starts_with('{')
        || !text.ends_with('}')
        || text.len() < 3
    {
        return;
    }
    let opener = text.as_bytes().get(1).copied().unwrap_or_default();
    if opener != b'.' && opener != b'#' && !opener.is_ascii_lowercase() {
        return;
    }
    sink.whole("unattached-block-attribute", line, &[]);
}

fn bare_html(line: &Line, sink: &mut Sink<'_>) {
    if !line.paragraph {
        return;
    }
    let Some(at) = line.text.find('<') else {
        return;
    };
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
    if !line.text.trim_start().starts_with('>') {
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
    if !line.paragraph || !caption_like(line.text.trim_start()) {
        return;
    }
    let below = lines
        .iter()
        .skip(index + 1)
        .take(2)
        .any(|other| other.text.trim_start().starts_with('|'));
    let above = lines.iter().take(index).rev().take(2).any(|other| {
        let text = other.text.trim_start();
        text.starts_with('|') || text.starts_with("![")
    });
    if below || above {
        let prefix = line.text.len() - line.text.trim_start().len();
        sink.emit_nth(
            "caption-looking-paragraph",
            line.start + prefix,
            line.start + prefix + 1,
            &line.path,
            &[],
            usize::from(!below),
        );
    }
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
