//! Lint of source lines after the 0.2 parse has marked paragraphs and opaque spans.

use super::{LintOptions, Range, active};
use crate::diagnostic::Diagnostic;
use crate::source::Source;

struct Line {
    text: String,
    start: usize,
    path: String,
    kind: String,
    paragraph: bool,
    content_start: usize,
    first_paragraph_line: bool,
    opaque: bool,
    attached_attribute: bool,
    scope: Scope,
}

struct Scope {
    in_footnote: bool,
    in_description: bool,
    in_container: bool,
}

struct Sink<'a> {
    source: Source<'a>,
    options: &'a LintOptions,
    output: &'a mut Vec<Diagnostic>,
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
        if !active(code, self.options) {
            return;
        }
        let pos = self.source.pos(from, to);
        self.output.retain(|d| {
            !(d.code == code
                && d.pos.is_some_and(|p| {
                    p.start_offset < pos.end_offset && pos.start_offset < p.end_offset
                }))
        });
        let mut diagnostic = Diagnostic::new(code, values, None, Some(pos));
        diagnostic.path = Some(path.to_string());
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
    };
    for (index, line) in lines.iter().enumerate() {
        if line.opaque {
            continue;
        }
        let previous = index.checked_sub(1).and_then(|i| lines.get(i));
        markers(line, previous, &mut sink);
        legacy(line, previous, &mut sink);
        prose(line, previous, &lines, &mut sink);
    }
}

fn lines(source: &str, ranges: &[Range]) -> Vec<Line> {
    let mut out = Vec::new();
    let mut start = 0;
    for piece in source.split_inclusive('\n') {
        let raw = piece.trim_end_matches(['\n', '\r']);
        let number = out.len() + 1;
        let paragraph = ranges
            .iter()
            .filter(|r| matches!(r.kind.as_str(), "paragraph" | "caption" | "definition_term"))
            .filter(|r| r.start_line <= number && number <= r.end_line)
            .max_by_key(|r| (r.kind == "caption", r.path.len()));
        let opaque = ranges.iter().any(|r| {
            matches!(r.kind.as_str(), "code_block" | "raw_block" | "frontmatter")
                && r.start_line <= number
                && number <= r.end_line
        });
        let mut bytes = raw.as_bytes().to_vec();
        for range in ranges.iter().filter(|r| is_opaque_leaf(&r.kind)) {
            let from = range.start.saturating_sub(start).min(bytes.len());
            let to = range.end.saturating_sub(start).min(bytes.len());
            if range.end > start && range.start < start + bytes.len() {
                bytes
                    .get_mut(from..to)
                    .into_iter()
                    .flatten()
                    .for_each(|b| *b = 0);
            }
        }
        out.push(Line {
            text: String::from_utf8_lossy(&bytes).into_owned(),
            start,
            path: paragraph.map_or_else(String::new, |r| r.path.clone()),
            kind: paragraph.map_or_else(String::new, |r| r.kind.clone()),
            paragraph: paragraph.is_some(),
            content_start: paragraph
                .filter(|range| range.start_line == number)
                .map_or(0, |range| range.start.saturating_sub(start).min(raw.len())),
            first_paragraph_line: paragraph.is_some_and(|range| range.start_line == number),
            opaque,
            attached_attribute: paragraph
                .is_some_and(|range| range.has_attrs && range.start_line == number),
            scope: scope(ranges, number),
        });
        start += piece.len();
    }
    if source.ends_with('\n') || source.is_empty() {
        out.push(Line {
            text: String::new(),
            start: source.len(),
            path: String::new(),
            kind: String::new(),
            paragraph: false,
            content_start: 0,
            first_paragraph_line: false,
            opaque: false,
            attached_attribute: false,
            scope: Scope {
                in_footnote: false,
                in_description: false,
                in_container: false,
            },
        });
    }
    out
}

fn scope(ranges: &[Range], number: usize) -> Scope {
    let contains = |kind: &str| {
        ranges
            .iter()
            .any(|r| r.kind == kind && r.start_line <= number && number <= r.end_line)
    };
    Scope {
        in_footnote: contains("footnote"),
        in_description: contains("definition_description"),
        in_container: [
            "block_quote",
            "list_item",
            "div",
            "definition_description",
            "footnote",
        ]
        .iter()
        .any(|kind| contains(kind)),
    }
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
    if bytes.get(1) == Some(&b' ') && bytes.first().is_some_and(|b| b"-*+".contains(b)) {
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
    let content = line
        .text
        .get(line.content_start..)
        .unwrap_or("")
        .trim_start();
    if text == "+" {
        sink.whole("deprecated-continuation-marker", line, &[]);
    }
    if run(text, "-_", 3) {
        let is_setext = text.starts_with('-')
            && previous.is_some_and(|p| p.paragraph && !p.text.trim().is_empty());
        if is_setext {
            sink.whole("setext-heading-underline", line, &[]);
        } else {
            sink.whole("thematic-break-spelling", line, &[("raw", text)]);
        }
    }
    if spaced_run(text) {
        sink.whole("deprecated-spaced-thematic-break", line, &[]);
    }
    if line.first_paragraph_line && (content.starts_with("~~~") || content.starts_with(":::")) {
        let what = if content.starts_with("~~~") {
            "a tilde fence"
        } else {
            "a div fence"
        };
        sink.emit(
            "block-marker-as-text",
            line.start + line.content_start,
            line.start + line.text.trim_end().len(),
            &line.path,
            &[("what", what)],
        );
    }
    attribute_line_like(line, sink);
    if line.text.trim_start_matches([' ', '\t']).starts_with('>')
        && text.starts_with('>')
        && text.len() > 1
        && !text.starts_with("> ")
        && !text.starts_with(">\t")
    {
        sink.whole(
            "blockquote-marker-without-space",
            line,
            &[("text", text.get(1..).unwrap_or(""))],
        );
    }
    table_line(line, sink);
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
    let text = line.text.trim();
    if line.paragraph && text.starts_with('|') && !text.ends_with('|') {
        sink.whole("table-shaped-paragraph", line, &[]);
        if text.matches('|').count() >= 2 {
            sink.whole("table-row-without-closing-pipe", line, &[]);
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
    if line.paragraph && text.starts_with("+ ") && text.len() > 2 {
        sink.whole("djot-plus-bullet", line, &[]);
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
        sink.whole("djot-heading-continuation", line, &[]);
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
        && (line.text.starts_with([' ', '\t']) || line.scope.in_container)
    {
        if let Some(start) = line.text.find('[')
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
    }
    if line.kind == "definition_term"
        && previous.is_some_and(|p| p.path == line.path)
        && starts_block(text)
    {
        sink.whole("definition-term-block-folded", line, &[]);
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

fn prose(line: &Line, previous: Option<&Line>, lines: &[Line], sink: &mut Sink<'_>) {
    prose_tokens(line, sink);
    prose_context(line, previous, lines, sink);
}

fn prose_tokens(line: &Line, sink: &mut Sink<'_>) {
    let text = line.text.as_str();
    let trimmed = text.trim();
    if line.paragraph && trimmed.starts_with('[') && trimmed.ends_with("]:") && trimmed.len() >= 4 {
        sink.whole("empty-reference-definition", line, &[]);
    }
    unattached_attribute(line, sink);
    if let Some(at) = text.find("<!--")
        && line.paragraph
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
    if let Some(at) = text.find("] (")
        && line.paragraph
    {
        sink.emit(
            "link-gap-before-destination",
            line.start + at,
            line.start + at + 3,
            &line.path,
            &[],
        );
    }
    if let Some(at) = text.find("{{")
        && line.paragraph
        && let Some(end) = text.get(at + 2..).and_then(|s| s.find("}}"))
        && text
            .get(at + 2..at + 2 + end)
            .is_some_and(|s| s.trim().is_empty() || s.trim_start().starts_with(['#', '@']))
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

fn prose_context(line: &Line, previous: Option<&Line>, lines: &[Line], sink: &mut Sink<'_>) {
    let trimmed = line.text.trim();
    if line.paragraph
        && previous.is_some_and(|p| p.paragraph && p.path == line.path && !p.text.trim().is_empty())
    {
        if starts_block(trimmed)
            && (!trimmed.starts_with('{') || trimmed.ends_with('}'))
            && !(trimmed.starts_with('[') && line.scope.in_footnote)
            && !(trimmed.starts_with("> ")
                && previous.is_some_and(|p| p.text.trim_start().starts_with("> ")))
        {
            let opener = if trimmed.starts_with(['#', '=']) {
                "a heading"
            } else if trimmed.starts_with('>') {
                "a quote"
            } else if trimmed.starts_with('{') {
                "an attribute line"
            } else if trimmed.starts_with("::") {
                "a term"
            } else if trimmed.starts_with(':') {
                "a description"
            } else {
                "a definition"
            };
            sink.whole(
                "block-opener-without-blank-line",
                line,
                &[("opener", opener)],
            );
        }
        if list_marker(trimmed) {
            sink.whole("list-marker-without-blank-line", line, &[]);
        }
    }
    bare_html(line, sink);
    github_alert(line, sink);
    caption_looking(line, previous, lines, sink);
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

fn github_alert(line: &Line, sink: &mut Sink<'_>) {
    if !line.text.trim_start().starts_with('>') {
        return;
    }
    if let Some(at) = line.text.find("[!")
        && line.text.get(at..).is_some_and(|s| s.contains(']'))
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

fn caption_looking(line: &Line, previous: Option<&Line>, lines: &[Line], sink: &mut Sink<'_>) {
    let above = previous.filter(|p| !p.text.trim().is_empty()).or_else(|| {
        lines
            .iter()
            .rev()
            .find(|p| p.start < line.start && !p.text.trim().is_empty())
    });
    if !line.paragraph || !above.is_some_and(|p| p.text.trim_start().starts_with('|')) {
        return;
    }
    if line.text.trim_start().starts_with("Table:") || line.text.trim_start().starts_with("Figure:")
    {
        sink.whole("caption-looking-paragraph", line, &[]);
    }
}
