use crate::{
    HtmlImportSeverity, MigrationConfidence, MigrationDiagnostic, MigrationFidelity,
    SmartTypographyMode,
};
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use pulldown_cmark::{CodeBlockKind, Event, LinkType, Options, Parser, Tag, TagEnd};
use std::{cell::Cell, collections::BTreeSet, sync::OnceLock};

thread_local! {
    static ATTRIBUTE_OPENER_BYTES: Cell<u64> = const { Cell::new(0) };
}

#[doc(hidden)]
pub fn take_attribute_opener_scan_bytes() -> u64 {
    ATTRIBUTE_OPENER_BYTES.with(|count| count.replace(0))
}

pub(crate) struct Assessment {
    pub diagnostics: Vec<MigrationDiagnostic>,
    pub complete: bool,
    /// Source lines of table rows whose cells past the header width held content.
    pub extra_cell_lines: Vec<usize>,
}

/// Marks a bare link whose table-cell pipes were unescaped when the event was built.
const TABLE_BARE_LINK: &str = "\u{E002}table-bare";
const BRACKET_OPEN_SENTINEL: &str = "\u{E000}";
const BRACKET_CLOSE_SENTINEL: &str = "\u{E001}";
const BRACKET_OPEN_ENCODED: &str = "%EE%80%80";
const BRACKET_CLOSE_ENCODED: &str = "%EE%80%81";

/// Restores the bracket sentinels inside `href` and `src` attribute values only.
fn restore_destination_brackets(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    let mut inspected = 0u64;
    loop {
        let bytes = rest.as_bytes();
        let mut at = 0;
        while at < bytes.len()
            && !(bytes[at] == b' '
                && (rest[at..].starts_with(" href=\"") || rest[at..].starts_with(" src=\"")))
        {
            at += 1;
        }
        inspected += at as u64 + u64::from(at < bytes.len());
        if at == bytes.len() {
            break;
        }
        at += if rest[at..].starts_with(" href=\"") {
            7
        } else {
            6
        };
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let end = rest.find('"').unwrap_or(rest.len());
        out.push_str(
            &rest[..end]
                .replace(BRACKET_OPEN_ENCODED, "[")
                .replace(BRACKET_CLOSE_ENCODED, "]"),
        );
        rest = &rest[end..];
    }
    ATTRIBUTE_OPENER_BYTES.with(|count| count.set(count.get() + inspected));
    out.push_str(rest);
    out
}

fn destination_has_entity(source: &str) -> bool {
    static ENTITY: OnceLock<regex::Regex> = OnceLock::new();
    let entity = ENTITY.get_or_init(|| {
        regex::Regex::new(r"&(?:#[xX][0-9A-Fa-f]+|#[0-9]+|[A-Za-z][A-Za-z0-9]+);").unwrap()
    });
    for opener in ["](", "]:"] {
        for (at, _) in source.match_indices(opener) {
            let rest = &source[at + opener.len()..];
            let end = rest.find([')', '\n']).unwrap_or(rest.len());
            if entity.is_match(&rest[..end]) {
                return true;
            }
        }
    }
    false
}

fn bare_link_events<'a>(label: &str, prefix: &str) -> [Event<'a>; 3] {
    [
        Event::Start(Tag::Link {
            link_type: LinkType::Autolink,
            dest_url: crate::markdown_import::markdown_bare_link_destination(&format!(
                "{prefix}{label}"
            ))
            .into(),
            title: "".into(),
            id: "".into(),
        }),
        Event::Text(label.to_owned().into()),
        Event::End(TagEnd::Link),
    ]
}

fn angle_email() -> &'static regex::Regex {
    static ANGLE_EMAIL: OnceLock<regex::Regex> = OnceLock::new();
    ANGLE_EMAIL.get_or_init(|| regex::Regex::new(r"<([A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?)*)>").unwrap())
}

fn email_events<'a>(
    events: Vec<(Event<'a>, usize)>,
    line_starts: &[usize],
    diagnostics: &mut Vec<(usize, MigrationDiagnostic)>,
) -> Vec<Event<'a>> {
    use crate::markdown_gfm_autolinks::{bare_links, Mode};
    let mut result = Vec::new();
    let mut input = events.into_iter().peekable();
    let mut opaque = 0usize;
    let mut in_table = false;
    while let Some((event, origin)) = input.next() {
        match &event {
            Event::Start(Tag::Table(_)) => in_table = true,
            Event::End(TagEnd::Table) => in_table = false,
            _ => {}
        }
        if opaque == 0 {
            if let Event::Text(first) = &event {
                let mut text = first.to_string();
                let mut original = vec![event];
                while let Some((Event::Text(next), _)) = input.peek() {
                    text.push_str(next);
                    original.push(input.next().unwrap().0);
                }
                if in_table && text.contains('|') {
                    let angle = angle_email();
                    if angle
                        .captures_iter(&text)
                        .any(|found| found[1].contains('|'))
                    {
                        // A cell's `\|` is already a pipe when GFM reads the autolink.
                        let mut cursor = 0;
                        for found in angle.captures_iter(&text).filter(|f| f[1].contains('|')) {
                            let whole = found.get(0).unwrap();
                            if cursor < whole.start() {
                                result.push(Event::Text(
                                    text[cursor..whole.start()].to_owned().into(),
                                ));
                            }
                            result.extend(bare_link_events(&found[1], "mailto:"));
                            diagnostics.push(positioned(
                                "autolink",
                                line_starts,
                                origin + whole.start(),
                                MigrationFidelity::Normalized,
                            ));
                            cursor = whole.end();
                        }
                        if cursor < text.len() {
                            result.push(Event::Text(text[cursor..].to_owned().into()));
                        }
                        continue;
                    }
                }
                let links = bare_links(&text, Mode::Email);
                if links.is_empty() {
                    result.extend(original);
                    continue;
                }
                let mut cursor = 0;
                for link in links {
                    let start = link.start;
                    if cursor < start {
                        result.push(Event::Text(text[cursor..start].to_owned().into()));
                    }
                    result.extend(bare_link_events(&text[link.start..link.end], link.prefix));
                    diagnostics.push(positioned(
                        "bare-autolink",
                        line_starts,
                        origin + start,
                        MigrationFidelity::Normalized,
                    ));
                    cursor = link.end;
                }
                if cursor < text.len() {
                    result.push(Event::Text(text[cursor..].to_owned().into()));
                }
                continue;
            }
        }
        match &event {
            Event::Start(
                Tag::Link { .. } | Tag::Image { .. } | Tag::CodeBlock(_) | Tag::MetadataBlock(_),
            ) => opaque += 1,
            Event::End(
                TagEnd::Link | TagEnd::Image | TagEnd::CodeBlock | TagEnd::MetadataBlock(_),
            ) => opaque = opaque.saturating_sub(1),
            _ => {}
        }
        result.push(event);
    }
    result
}

fn positioned(
    construct: &str,
    line_starts: &[usize],
    offset: usize,
    fidelity: MigrationFidelity,
) -> (usize, MigrationDiagnostic) {
    (offset, diagnostic(construct, line_starts, offset, fidelity))
}

fn diagnostic(
    construct: &str,
    line_starts: &[usize],
    offset: usize,
    fidelity: MigrationFidelity,
) -> MigrationDiagnostic {
    let line = line_starts.partition_point(|start| *start <= offset);
    let code = match construct {
        "ordered-task" => "structure-unspellable".to_owned(),
        "raw-html" => "raw-preserved".to_owned(),
        _ => format!("markdown-{construct}"),
    };
    MigrationDiagnostic {
        code,
        message: if construct == "ordered-task" {
            crate::html_import::ORDERED_TASK_ITEM_UNSPELLABLE.to_owned()
        } else {
            format!("Assessed Markdown {construct}.")
        },
        severity: if fidelity == MigrationFidelity::Dropped {
            HtmlImportSeverity::Warning
        } else {
            HtmlImportSeverity::Info
        },
        fidelity,
        confidence: MigrationConfidence::Exact,
        path: Some(format!("line:{line}")),
    }
}

fn table_row_pipe_escapes(row: &str, start: usize) -> (Vec<usize>, bool) {
    let bytes = row.as_bytes();
    let mut escapes = Vec::new();
    let mut doubled_boundary = false;
    let mut column = 0;
    while column < bytes.len() {
        if bytes[column] != b'\\' {
            column += 1;
            continue;
        }
        let first = column;
        while column < bytes.len() && bytes[column] == b'\\' {
            column += 1;
        }
        if bytes.get(column) == Some(&b'|') && (column - first) % 2 == 1 {
            escapes.push(start + column - 1);
            column += 1;
        } else if bytes.get(column) == Some(&b'|') && column - first > 0 {
            doubled_boundary = true;
        }
    }
    (escapes, doubled_boundary)
}

/// Cells of a pipe row split on unescaped pipes, as GFM reads them.
fn table_row_cells(row: &str) -> Vec<&str> {
    let bytes = row.as_bytes();
    let mut cells = Vec::new();
    let mut start = 0;
    let mut column = 0;
    while column < bytes.len() {
        match bytes[column] {
            b'\\' => column += 2,
            b'|' => {
                cells.push(&row[start..column]);
                start = column + 1;
                column += 1;
            }
            _ => column += 1,
        }
    }
    cells.push(&row[start..]);
    let trimmed = row.trim();
    if trimmed.starts_with('|') && !cells.is_empty() {
        cells.remove(0);
    }
    if trimmed.ends_with('|') && !trimmed.ends_with("\\|") && !cells.is_empty() {
        cells.pop();
    }
    cells
}

#[derive(Debug, PartialEq, Eq)]
enum Shape {
    Text(String),
    Comment(String),
    Element(String, Vec<(String, String)>, Vec<Shape>),
}

fn shape(html: &str, list_marker: Option<&str>) -> Vec<Shape> {
    fn visit(node: &Handle, list_marker: Option<&str>) -> Vec<Shape> {
        match &node.data {
            NodeData::Text { contents } => vec![Shape::Text(contents.borrow().to_string())],
            NodeData::Comment { contents } => {
                if list_marker.is_some_and(|marker| contents.as_ref() == marker) {
                    Vec::new()
                } else {
                    vec![Shape::Comment(contents.to_string())]
                }
            }
            NodeData::Element { name, attrs, .. } => {
                let tag = if name.local.as_ref() == "del" {
                    "s".to_owned()
                } else {
                    name.local.to_string()
                };
                let generated_delimiter = tag == "ol"
                    && node.children.borrow().iter().any(|child| {
                        matches!(&child.data, NodeData::Comment { contents }
                            if list_marker.is_some_and(|marker| contents.as_ref() == marker))
                    });
                let mut children: Vec<_> = node
                    .children
                    .borrow()
                    .iter()
                    .flat_map(|node| visit(node, list_marker))
                    .collect();
                let layout = |child: &Shape| matches!(child, Shape::Text(value) if value.trim_matches([' ', '\t', '\r', '\n']).is_empty());
                if tag == "section" && children.iter().any(|child| matches!(child, Shape::Element(tag, _, _) if matches!(tag.as_str(), "h1"|"h2"|"h3"|"h4"|"h5"|"h6"))) {
                    children.retain(|child| !layout(child));
                    return children;
                }
                if matches!(
                    tag.as_str(),
                    "html"
                        | "head"
                        | "body"
                        | "ul"
                        | "ol"
                        | "li"
                        | "blockquote"
                        | "table"
                        | "thead"
                        | "tbody"
                        | "tr"
                ) {
                    children.retain(|child| !layout(child));
                }
                if tag == "tbody" && children.is_empty() {
                    return Vec::new();
                }
                if tag == "li" {
                    for index in 0..children.len().saturating_sub(1) {
                        let nested_list = matches!(
                            &children[index + 1],
                            Shape::Element(tag, _, _) if matches!(tag.as_str(), "ul" | "ol")
                        );
                        if nested_list {
                            if let Shape::Text(value) = &mut children[index] {
                                if let Some(newline) = value.rfind('\n') {
                                    if value[newline + 1..]
                                        .bytes()
                                        .all(|byte| matches!(byte, b' ' | b'\t'))
                                    {
                                        value.truncate(newline + 1);
                                    }
                                }
                            }
                        }
                    }
                }
                let mut attributes: Vec<_> = attrs
                    .borrow()
                    .iter()
                    .filter_map(|attr| {
                        let key = attr.name.local.to_string();
                        if (key == "id"
                            && matches!(tag.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6"))
                            || (key == "scope" && tag == "th")
                            || (key == "aria-label" && tag == "input")
                        {
                            return None;
                        }
                        if key == "align" && matches!(tag.as_str(), "td" | "th") {
                            return Some((
                                "style".to_owned(),
                                format!("text-align: {};", attr.value),
                            ));
                        }
                        Some((key, attr.value.to_string()))
                    })
                    .collect();
                if generated_delimiter {
                    attributes.push(("data-delim".to_owned(), ")".to_owned()));
                }
                if list_marker.is_some() {
                    let checkbox = |child: &Shape| matches!(child, Shape::Element(tag, attrs, _) if tag == "input" && attrs.iter().any(|(key, value)| key == "type" && value == "checkbox"));
                    if tag == "li" && children.iter().any(checkbox) && children.iter().any(|child| matches!(child, Shape::Element(tag, attrs, _) if tag == "input" && attrs.iter().any(|(key, _)| key == "checked"))) {
                        attributes.push(("data-task-state".to_owned(), "x".to_owned()));
                    }
                    let items: Vec<_> = children
                        .iter()
                        .filter_map(|child| match child {
                            Shape::Element(tag, _, children) if tag == "li" => Some(children),
                            _ => None,
                        })
                        .collect();
                    if tag == "ul"
                        && !items.is_empty()
                        && items.iter().all(|children| children.iter().any(checkbox))
                    {
                        attributes.push(("class".to_owned(), "task-list".to_owned()));
                    }
                }
                attributes.sort();
                vec![Shape::Element(tag, attributes, children)]
            }
            _ => node
                .children
                .borrow()
                .iter()
                .flat_map(|node| visit(node, list_marker))
                .collect(),
        }
    }
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    visit(&dom.document, list_marker)
}

fn unused_list_marker(source: &str, value: &str) -> String {
    const PREFIX: &str = "carve-migration-list-delimiter-";
    let mut occupied = BTreeSet::new();
    for text in [source, value] {
        for (offset, _) in text.match_indices(PREFIX) {
            let rest = &text[offset + PREFIX.len()..];
            let digits = rest
                .bytes()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            if let Ok(number) = rest[..digits].parse::<u64>() {
                occupied.insert(number);
            }
        }
    }
    let counter = (0u64..)
        .find(|counter| !occupied.contains(counter))
        .unwrap();
    format!("{PREFIX}{counter}:)")
}

fn is_atx_heading(source: &str) -> bool {
    let line = source
        .lines()
        .next()
        .unwrap_or("")
        .trim_start_matches([' ', '\t']);
    let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
    (1..=6).contains(&hashes)
        && line
            .as_bytes()
            .get(hashes)
            .map_or(true, |byte| matches!(byte, b' ' | b'\t'))
}

fn ordered_delimiters(blocks: &[crate::BlockNode]) -> Vec<char> {
    let mut found = Vec::new();
    let mut pending: Vec<_> = blocks.iter().rev().collect();
    while let Some(block) = pending.pop() {
        match block {
            crate::BlockNode::List(list) => {
                if list.ordered {
                    found.push(list.delim.unwrap_or('.'));
                }
                for item in list.items.iter().rev() {
                    pending.extend(item.children.iter().rev());
                }
            }
            crate::BlockNode::BlockQuote(quote) => pending.extend(quote.children.iter().rev()),
            crate::BlockNode::Section(section) => pending.extend(section.children.iter().rev()),
            crate::BlockNode::Div(div) => pending.extend(div.children.iter().rev()),
            _ => {}
        }
    }
    found
}

/// cmark-gfm drops one backslash before a table pipe and keeps the pipe in the
/// cell, for any run length; pulldown splits the cell on an even run. Rewrite each
/// even run in a table row as one backslash fewer plus a stand-in character, so
/// pulldown reads the cell as cmark-gfm does. Returns the stand-in and where it sits.
fn hide_even_pipe_runs(source: &str) -> Option<(String, char, Vec<usize>)> {
    let delimiter = |line: &str| {
        let line = line.trim();
        line.contains('-')
            && line
                .chars()
                .all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
    };
    let lines: Vec<&str> = source.split('\n').collect();
    let mut rows = BTreeSet::new();
    for (index, line) in lines.iter().enumerate() {
        if index == 0 || !delimiter(line) || !lines[index - 1].contains('|') {
            continue;
        }
        rows.insert(index - 1);
        let mut next = index + 1;
        while next < lines.len() && !lines[next].trim().is_empty() {
            rows.insert(next);
            next += 1;
        }
    }
    let has_even_run = rows.iter().any(|&index| {
        let bytes = lines[index].as_bytes();
        (0..bytes.len()).any(|at| {
            bytes[at] == b'|' && {
                let run = bytes[..at]
                    .iter()
                    .rev()
                    .take_while(|b| **b == b'\\')
                    .count();
                run >= 2 && run % 2 == 0
            }
        })
    });
    if !has_even_run {
        return None;
    }
    let stand_in = ['^', '+', '=', '$', '}']
        .into_iter()
        .find(|candidate| !source.contains(*candidate))?;
    let mut out = String::with_capacity(source.len());
    let mut hidden = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        if !rows.contains(&index) {
            out.push_str(line);
            continue;
        }
        let bytes = line.as_bytes();
        let mut at = 0;
        while at < bytes.len() {
            if bytes[at] != b'\\' {
                let start = at;
                while at < bytes.len() && bytes[at] != b'\\' {
                    at += 1;
                }
                out.push_str(&line[start..at]);
                continue;
            }
            let start = at;
            while at < bytes.len() && bytes[at] == b'\\' {
                at += 1;
            }
            let run = at - start;
            if bytes.get(at) == Some(&b'|') && run % 2 == 0 {
                out.push_str(&line[start..at - 1]);
                hidden.push(out.len());
                out.push(stand_in);
                at += 1;
            } else {
                out.push_str(&line[start..at]);
            }
        }
    }
    Some((out, stand_in, hidden))
}

/// Assess the native parser's occurrences and verify the document the writer produced.
pub(crate) fn assess(source: &str, value: &str) -> Assessment {
    use crate::html_import::ImportFidelity::{Degraded, Dropped, Normalized, Preserved};
    if source.len() > 1_000_000 || source.contains('\0') {
        return Assessment {
            diagnostics: Vec::new(),
            complete: false,
            extra_cell_lines: Vec::new(),
        };
    }
    // Parser offsets refer to this normalized input; normalization never adds a line.
    let mut source = source.replace("\r\n", "\n").replace('\r', "\n");
    if !source.is_empty() && !source.ends_with('\n') {
        source.push('\n');
    }
    let mut hidden_pipe = None;
    if let Some((hidden, stand_in, positions)) = hide_even_pipe_runs(&source) {
        source = hidden;
        hidden_pipe = Some((stand_in, positions));
    }
    let mut line_starts = vec![0];
    line_starts.extend(
        source
            .bytes()
            .enumerate()
            .filter_map(|(index, byte)| (byte == b'\n').then_some(index + 1)),
    );
    let mut options = Options::empty();
    options.insert(
        Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_FOOTNOTES
            | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS,
    );
    let prepared_urls = crate::markdown_gfm_autolinks::prepare_urls(&source, options);
    let parser_source = prepared_urls
        .as_ref()
        .map_or(source.as_str(), |prepared| prepared.0.as_str());
    let bare_urls = prepared_urls
        .as_ref()
        .map_or(&[][..], |prepared| prepared.1.as_slice());
    let mut bare_url_at = 0;
    let strikes = crate::markdown_strike::prepare(parser_source, options);
    let mut reference = |link| strikes.reference(link);
    let parser =
        Parser::new_with_broken_link_callback(&strikes.text, options, Some(&mut reference));
    let mut diagnostics = Vec::new();
    let mut complete = prepared_urls.as_ref().map_or(true, |prepared| prepared.2);
    let mut definition_spans = Vec::new();
    for (_, definition) in parser.reference_definitions().iter() {
        let span = strikes.original_range(definition.span.clone());
        if destination_has_entity(&source[span.clone()]) {
            complete = false;
        }
        definition_spans.push(span);
        diagnostics.push(positioned(
            "reference-definition",
            &line_starts,
            strikes.original_range(definition.span.clone()).start,
            Normalized,
        ));
    }
    let mut events = Vec::new();
    let expected_list_marker = unused_list_marker(&source, value);
    let mut content_spans = Vec::new();
    let mut lists = Vec::new();
    let mut authored_ordered_delimiters = Vec::new();
    let mut link_depth = 0usize;
    let mut code_depth = 0usize;
    let mut code_needs_ending = false;
    let mut html_block = false;
    let mut html_fragments = 0usize;
    let mut table_pipe_escapes = BTreeSet::new();
    if let Some((_, positions)) = &hidden_pipe {
        table_pipe_escapes.extend(positions.iter().map(|at| at - 1));
    }
    let mut table_width = 0usize;
    let mut extra_cell_lines = Vec::new();
    static REWRITE: OnceLock<regex::Regex> = OnceLock::new();
    let rewrite = REWRITE.get_or_init(|| regex::Regex::new(r"&(?:#[xX][\da-fA-F]+|#\d+|[A-Za-z][A-Za-z\d]+);|\\[!\x22#$%&'()*+,\-./:;<=>?@\[\]\\^_`{|}~]").unwrap());
    for (event, range) in parser.into_offset_iter() {
        if strikes.synthetic_comment(&event, &range) {
            continue;
        }
        let event = strikes.restore(event, &range);
        let range = strikes.original_range(range);
        if let Event::Start(Tag::Table(alignments)) = &event {
            table_width = alignments.len();
        }
        if matches!(event, Event::Start(Tag::TableHead | Tag::TableRow)) {
            let end = source[range.start..]
                .find('\n')
                .map_or(source.len(), |offset| range.start + offset);
            let row = &source[range.start..end];
            let (escapes, doubled_boundary) = table_row_pipe_escapes(row, range.start);
            table_pipe_escapes.extend(escapes);
            if !doubled_boundary
                && table_row_cells(row)
                    .iter()
                    .skip(table_width)
                    .any(|cell| !cell.trim().is_empty())
            {
                complete = false;
                extra_cell_lines.push(line_starts.partition_point(|start| *start <= range.start));
            }
            if doubled_boundary
                || (matches!(event, Event::Start(Tag::TableRow)) && !row.contains('|'))
            {
                complete = false;
            }
        }
        if matches!(
            &event,
            Event::Start(
                Tag::Link {
                    link_type: LinkType::Inline,
                    ..
                } | Tag::Image {
                    link_type: LinkType::Inline,
                    ..
                }
            )
        ) && destination_has_entity(&source[range.clone()])
        {
            complete = false;
        }
        let mut paren_list = false;
        if !matches!(event, Event::Start(_) | Event::End(_)) {
            content_spans.push(range.clone());
        }
        if matches!(&event, Event::Text(_)) && code_depth == 0 {
            let input = &source[range.clone()];
            if range.start > 0
                && parser_source[..range.start]
                    .bytes()
                    .rev()
                    .take_while(|byte| *byte == b'\\')
                    .count()
                    % 2
                    == 1
                && input.starts_with(|character: char| character.is_ascii_punctuation())
                && !table_pipe_escapes.contains(&(range.start - 1))
            {
                diagnostics.push(positioned(
                    "escape",
                    &line_starts,
                    range.start - 1,
                    Normalized,
                ));
            }
        }
        if matches!(&event, Event::Text(_)) && link_depth == 0 && code_depth == 0 {
            while bare_urls
                .get(bare_url_at)
                .is_some_and(|link| link.end <= range.start)
            {
                bare_url_at += 1;
            }
            if bare_urls
                .get(bare_url_at)
                .is_some_and(|link| link.start >= range.start && link.end <= range.end)
            {
                let mut cursor = range.start;
                while let Some(link) = bare_urls
                    .get(bare_url_at)
                    .filter(|link| link.start >= cursor && link.end <= range.end)
                {
                    if cursor < link.start {
                        events.push((Event::Text(source[cursor..link.start].into()), cursor));
                    }
                    let label = if link.table {
                        std::borrow::Cow::Owned(source[link.start..link.end].replace("\\|", "|"))
                    } else {
                        std::borrow::Cow::Borrowed(&source[link.start..link.end])
                    };
                    let mut link_events = bare_link_events(&label, link.prefix);
                    if link.table {
                        // The label is already unescaped; the row-level pass must not drop a second backslash.
                        if let Event::Start(Tag::Link { title, .. }) = &mut link_events[0] {
                            *title = TABLE_BARE_LINK.into();
                        }
                    }
                    events.extend(link_events.map(|event| (event, link.start)));
                    diagnostics.push(positioned(
                        "bare-autolink",
                        &line_starts,
                        link.start,
                        Normalized,
                    ));
                    cursor = link.end;
                    bare_url_at += 1;
                }
                if cursor < range.end {
                    events.push((Event::Text(source[cursor..range.end].into()), cursor));
                }
                continue;
            }
        }
        let mut assessment = None;
        match &event {
            Event::Start(tag) => {
                if matches!(
                    tag,
                    Tag::Paragraph
                        | Tag::CodeBlock(_)
                        | Tag::HtmlBlock
                        | Tag::Table(_)
                        | Tag::Heading { .. }
                        | Tag::Link { .. }
                        | Tag::Image { .. }
                ) {
                    content_spans.push(range.clone());
                }
                assessment = match tag {
                    Tag::Paragraph => Some(("paragraph", Preserved)),
                    Tag::Heading { .. } => Some((
                        if is_atx_heading(&source[range.clone()]) {
                            "atx-heading"
                        } else {
                            "setext-heading"
                        },
                        if is_atx_heading(&source[range.clone()]) {
                            Preserved
                        } else {
                            Normalized
                        },
                    )),
                    Tag::BlockQuote(_) => Some(("block-quote", Preserved)),
                    Tag::List(start) => {
                        lists.push(start.is_some());
                        if start.is_some() {
                            let marker = source[range.clone()].trim_start_matches([' ', '\t']);
                            let delimiter = marker
                                .chars()
                                .find(|character| !character.is_ascii_digit())
                                .unwrap_or('.');
                            paren_list = delimiter == ')';
                            authored_ordered_delimiters.push(delimiter);
                        }
                        Some((
                            if start.is_some() {
                                "ordered-list"
                            } else {
                                "bullet-list"
                            },
                            Preserved,
                        ))
                    }
                    Tag::Item => Some(("list-item", Preserved)),
                    Tag::CodeBlock(kind) => {
                        code_depth += 1;
                        code_needs_ending = false;
                        Some((
                            if matches!(kind, CodeBlockKind::Indented) {
                                "indented-code"
                            } else {
                                "fenced-code"
                            },
                            if matches!(kind, CodeBlockKind::Indented) {
                                Normalized
                            } else {
                                Preserved
                            },
                        ))
                    }
                    Tag::Emphasis => Some(("emphasis", Preserved)),
                    Tag::Strong => Some(("strong", Preserved)),
                    Tag::Strikethrough => Some(("strikethrough", Preserved)),
                    Tag::Link { link_type, .. } => {
                        link_depth += 1;
                        Some(match link_type {
                            LinkType::Autolink | LinkType::Email => ("autolink", Normalized),
                            LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut => {
                                ("reference-link", Normalized)
                            }
                            _ => ("link", Preserved),
                        })
                    }
                    Tag::Image { link_type, .. } => {
                        link_depth += 1;
                        Some(match link_type {
                            LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut => {
                                ("reference-link", Normalized)
                            }
                            _ => ("image", Preserved),
                        })
                    }
                    Tag::Table(_) => Some(("table", Preserved)),
                    Tag::TableHead | Tag::TableRow => Some(("table-row", Preserved)),
                    Tag::TableCell => Some(("table-cell", Preserved)),
                    Tag::MetadataBlock(_) | Tag::FootnoteDefinition(_) => {
                        complete = false;
                        None
                    }
                    Tag::HtmlBlock => {
                        html_block = true;
                        Some(("raw-html", Degraded))
                    }
                    _ => {
                        complete = false;
                        None
                    }
                };
            }
            Event::End(TagEnd::HtmlBlock) => {
                html_block = false;
            }
            Event::End(TagEnd::List(_)) => {
                lists.pop();
            }
            Event::End(TagEnd::Link | TagEnd::Image) => {
                link_depth = link_depth.saturating_sub(1);
            }
            Event::End(TagEnd::CodeBlock) => {
                if code_needs_ending {
                    events.push((Event::Text("\n".into()), range.start));
                }
                code_needs_ending = false;
                code_depth = code_depth.saturating_sub(1);
            }
            Event::Text(text) if code_depth > 0 => {
                if !text.is_empty() {
                    code_needs_ending = !text.ends_with('\n');
                }
            }
            Event::Text(text) if code_depth == 0 => {
                let input = &source[range.clone()];

                for matched in rewrite.find_iter(input) {
                    if text.as_ref() == input {
                        continue;
                    }
                    if matched.as_str() == "\\|"
                        && table_pipe_escapes.contains(&(range.start + matched.start()))
                    {
                        continue;
                    }
                    diagnostics.push(positioned(
                        if matched.as_str().starts_with('&') {
                            "entity"
                        } else {
                            "escape"
                        },
                        &line_starts,
                        range.start + matched.start(),
                        Normalized,
                    ));
                }
            }
            Event::Code(_) => {
                assessment = Some(("code-span", Preserved));
            }
            Event::SoftBreak => {
                assessment = Some(("soft-break", Preserved));
            }
            Event::HardBreak => {
                assessment = Some(("hard-break", Normalized));
            }
            Event::Rule => {
                assessment = Some(("thematic-break", Preserved));
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                html_fragments += 1;
                let lower = html.to_lowercase();
                if html_fragments > 128
                    || !value.contains(html.as_ref())
                    || [
                        "<section",
                        "</section",
                        "<h1",
                        "<h2",
                        "<h3",
                        "<h4",
                        "<h5",
                        "<h6",
                        "</h1",
                        "</h2",
                        "</h3",
                        "</h4",
                        "</h5",
                        "</h6",
                        "<input",
                    ]
                    .iter()
                    .any(|tag| lower.contains(tag))
                {
                    complete = false;
                }
                if !html_block {
                    assessment = Some(("raw-html", Degraded));
                }
            }
            Event::TaskListMarker(_) => {
                let reaches = crate::markdown_import::tasklist_extension_reaches(&source, &range);
                let ordered = lists.last() == Some(&true);
                if ordered || !reaches {
                    if ordered && reaches {
                        diagnostics.push(positioned(
                            "ordered-task",
                            &line_starts,
                            range.start,
                            Dropped,
                        ));
                    }
                    events.push((Event::Text(source[range.clone()].into()), range.start));
                    let separator: String = source[range.end..]
                        .chars()
                        .take_while(|character| matches!(character, ' ' | '\t'))
                        .collect();
                    if !separator.is_empty() {
                        events.push((Event::Text(separator.into()), range.end));
                    } else if source[range.end..].starts_with('\n') {
                        events.push((Event::SoftBreak, range.end));
                    }
                    continue;
                }
                diagnostics.push(positioned(
                    "bullet-task",
                    &line_starts,
                    range.start,
                    Preserved,
                ));
                events.push((event, range.start));
                events.push((Event::Text(" ".into()), range.end));
                continue;
            }
            Event::FootnoteReference(_) | Event::InlineMath(_) | Event::DisplayMath(_) => {
                complete = false;
            }
            _ => {}
        }
        if let Some((construct, fidelity)) = assessment {
            diagnostics.push(positioned(construct, &line_starts, range.start, fidelity));
        }
        events.push((event, range.start));
        if paren_list {
            events.push((
                Event::Html(format!("<!--{expected_list_marker}-->").into()),
                range.start,
            ));
        }
    }
    let mut coverage_changes = vec![0_i32; line_starts.len() + 1];
    for span in content_spans.iter().chain(&definition_spans) {
        let first = line_starts
            .partition_point(|start| *start <= span.start)
            .saturating_sub(1);
        let end = line_starts.partition_point(|start| *start < span.end);
        if first < end {
            coverage_changes[first] += 1;
            coverage_changes[end] -= 1;
        }
    }
    let mut coverage_depth = 0;
    let covered_lines: Vec<_> = coverage_changes
        .into_iter()
        .map(|change| {
            coverage_depth += change;
            coverage_depth > 0
        })
        .collect();
    for (index, start) in line_starts
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, start)| *start < source.len())
    {
        if covered_lines[index] {
            continue;
        }
        let end = source[start..]
            .find('\n')
            .map_or(source.len(), |end| start + end + 1);
        let line = &source[start..end];
        if line.contains("]:") {
            complete = false;
        }
    }
    let events = email_events(events, &line_starts, &mut diagnostics);
    if complete {
        let mut reference_events = Vec::with_capacity(events.len());
        let mut unicode_marker = "CARVEURLUNICODEPLACEHOLDER".to_owned();
        while source.contains(&unicode_marker) || value.contains(&unicode_marker) {
            unicode_marker.push('X');
        }
        let mut unicode_chars = Vec::new();
        let mut lone_image_paragraph_end = None;
        let mut in_table = false;
        let mut table_autolink = false;
        for (index, event) in events.iter().enumerate() {
            table_autolink = match event {
                Event::Start(Tag::Link {
                    link_type: LinkType::Autolink | LinkType::Email,
                    title,
                    ..
                }) => in_table && title.as_ref() != TABLE_BARE_LINK,
                Event::End(TagEnd::Link) => false,
                _ => table_autolink,
            };
            if matches!(event, Event::Start(Tag::Table(_))) {
                in_table = true;
            } else if matches!(event, Event::End(TagEnd::Table)) {
                in_table = false;
            }
            let lone_image = matches!(event, Event::Start(Tag::Paragraph))
                && matches!(events.get(index + 1), Some(Event::Start(Tag::Image { .. })))
                && {
                    let mut depth = 0usize;
                    let mut image_end = None;
                    for (offset, inner) in events[index + 1..].iter().enumerate() {
                        match inner {
                            Event::Start(Tag::Image { .. }) => depth += 1,
                            Event::End(TagEnd::Image) => {
                                depth -= 1;
                                if depth == 0 {
                                    image_end = Some(index + 1 + offset);
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    image_end.is_some_and(|end| {
                        matches!(events.get(end + 1), Some(Event::End(TagEnd::Paragraph)))
                    })
                };
            if lone_image {
                lone_image_paragraph_end = events[index + 1..]
                    .iter()
                    .position(|inner| matches!(inner, Event::End(TagEnd::Image)))
                    .map(|offset| index + offset + 2);
            }
            if lone_image || lone_image_paragraph_end == Some(index) {
                continue;
            }
            let mut event = event.clone();
            if let Event::Start(Tag::Link { title, .. }) = &mut event {
                if title.as_ref() == TABLE_BARE_LINK {
                    *title = "".into();
                }
            }
            if table_autolink {
                match &mut event {
                    Event::Start(Tag::Link { dest_url, .. }) => {
                        *dest_url = dest_url.replace("\\|", "|").into();
                    }
                    Event::Text(text) => *text = text.replace("\\|", "|").into(),
                    _ => {}
                }
            }
            if let Some((stand_in, _)) = &hidden_pipe {
                match &mut event {
                    Event::Text(text) | Event::Code(text) | Event::InlineHtml(text) => {
                        *text = text.replace(*stand_in, "|").into();
                    }
                    Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) => {
                        *dest_url = dest_url.replace(*stand_in, "|").into();
                    }
                    _ => {}
                }
            }
            match &mut event {
                Event::Start(
                    Tag::Link {
                        dest_url,
                        link_type,
                        ..
                    }
                    | Tag::Image {
                        dest_url,
                        link_type,
                        ..
                    },
                ) if !matches!(link_type, LinkType::Autolink | LinkType::Email) => {
                    // pulldown's href escaper encodes `[` and `]`, which Carve renders
                    // literally; the sentinels survive it and are restored below. An
                    // authored U+E000/U+E001 can only turn a match into a mismatch.
                    *dest_url = crate::markdown_import::markdown_destination(dest_url, false)
                        .replace('[', BRACKET_OPEN_SENTINEL)
                        .replace(']', BRACKET_CLOSE_SENTINEL)
                        .into();
                }
                Event::Start(Tag::Link {
                    dest_url,
                    link_type: LinkType::Autolink,
                    ..
                }) => {
                    *dest_url = dest_url
                        .replace('[', BRACKET_OPEN_SENTINEL)
                        .replace(']', BRACKET_CLOSE_SENTINEL)
                        .into();
                }
                _ => {}
            }
            if let Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) =
                &mut event
            {
                let mut marked = String::new();
                for ch in dest_url.chars() {
                    if (!ch.is_ascii() && !ch.is_whitespace() && !ch.is_control()
                        || matches!(ch, '\\' | '`' | '^' | '{' | '}' | '"' | '<' | '>')
                        || (ch == '|' && !in_table))
                        && !matches!(ch, '\u{E000}' | '\u{E001}')
                    {
                        marked.push_str(&format!("{unicode_marker}{}END", unicode_chars.len()));
                        unicode_chars.push(ch);
                    } else {
                        marked.push(ch);
                    }
                }
                *dest_url = marked.into();
            }
            reference_events.push(event);
        }
        let mut expected = String::new();
        pulldown_cmark::html::push_html(&mut expected, reference_events.into_iter());
        for (index, ch) in unicode_chars.into_iter().enumerate() {
            let rendered = match ch {
                '"' => "&quot;".to_owned(),
                '<' => "&lt;".to_owned(),
                '>' => "&gt;".to_owned(),
                _ => ch.to_string(),
            };
            expected = expected.replace(&format!("{unicode_marker}{index}END"), &rendered);
        }
        expected = restore_destination_brackets(&expected)
            .replace(
                "<input disabled=\"\" type=\"checkbox\" checked=\"\"/>\n",
                "<input disabled=\"\" type=\"checkbox\" checked=\"\"/>",
            )
            .replace(
                "<input disabled=\"\" type=\"checkbox\"/>\n",
                "<input disabled=\"\" type=\"checkbox\"/>",
            );
        let actual = crate::to_html_with_options(
            value,
            &crate::Options {
                smart_typography: SmartTypographyMode::Source,
                ..crate::Options::default()
            },
        );
        // Mark only parser-authored lists in the expected HTML. Raw HTML
        // attributes remain part of the comparison.
        let document = crate::parse(value);
        complete = authored_ordered_delimiters == ordered_delimiters(&document.children)
            && shape(&expected, Some(&expected_list_marker)) == shape(&actual, None);
    }
    diagnostics.sort_by_key(|(offset, row)| {
        (
            row.path
                .as_ref()
                .and_then(|path| path.strip_prefix("line:"))
                .and_then(|line| line.parse::<usize>().ok())
                .unwrap_or(0),
            *offset,
        )
    });
    let diagnostics: Vec<_> = diagnostics.into_iter().map(|(_, row)| row).collect();
    Assessment {
        diagnostics: if complete { diagnostics } else { Vec::new() },
        complete,
        extra_cell_lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_pipe_shapes_are_assessed_completely() {
        for source in [
            "| <https://example.com/a\\|b> | z |\n|---|---|\n",
            "| a \\| b | z |\n|---|---|\n",
            "| [l](https://e.com/a\\|x) | z |\n|---|---|\n",
            "| <a\\|b@example.com> | z |\n|---|---|\n",
            "| <https://e.com/a\\|b> | z |\n|---|---|\n\ntext <https://e.com/a\\|b>\n",
        ] {
            let result = crate::migrate_markdown(source);
            assert!(
                result
                    .report
                    .diagnostics
                    .iter()
                    .all(|row| row.code != "fidelity-unverified"),
                "{source}"
            );
        }
    }

    #[test]
    fn table_body_cells_past_the_header_are_reported_as_dropped() {
        for source in [
            "| h |\n|---|\n| <https://e.com/a\\|b> | extra |\n",
            "| a | b |\n|---|---|\n| <https://example.com/a|b> | z |\n",
        ] {
            let result = crate::migrate_markdown(source);
            let actual: Vec<_> = result
                .report
                .diagnostics
                .iter()
                .map(|row| (row.code.as_str(), row.path.as_deref(), row.fidelity))
                .collect();
            assert_eq!(
                actual,
                vec![
                    ("fidelity-unverified", None, MigrationFidelity::Dropped),
                    (
                        "markdown-table-extra-cells",
                        Some("line:3"),
                        MigrationFidelity::Dropped
                    ),
                ],
                "{source}"
            );
        }
    }

    #[test]
    fn table_pipe_assessment_matches_row_level_unescaping() {
        let table = [
            ("markdown-table", "line:1", MigrationFidelity::Preserved),
            ("markdown-table-row", "line:1", MigrationFidelity::Preserved),
            (
                "markdown-table-cell",
                "line:1",
                MigrationFidelity::Preserved,
            ),
            ("markdown-table-row", "line:3", MigrationFidelity::Preserved),
            (
                "markdown-table-cell",
                "line:3",
                MigrationFidelity::Preserved,
            ),
        ];
        for (source, extra) in [
            ("| h |\n| --- |\n| a\\| |", None),
            ("| h |\n| --- |\n| `a\\|b` |", Some("markdown-code-span")),
            ("| h |\n| --- |\n| a\\|", None),
            ("| h |\n| --- |\n| a\\|b\\|c |", None),
        ] {
            let mut expected = table.to_vec();
            if let Some(code) = extra {
                expected.push((code, "line:3", MigrationFidelity::Preserved));
            }
            let result = crate::migrate_markdown(source);
            let actual: Vec<_> = result
                .report
                .diagnostics
                .iter()
                .map(|row| {
                    (
                        row.code.as_str(),
                        row.path.as_deref().unwrap_or(""),
                        row.fidelity,
                    )
                })
                .collect();
            assert_eq!(actual, expected, "{source}");
        }
        for source in [
            "| h |\n| --- |\n| a\\\\|",
            "| h | x |\n| --- | --- |\n| a\\\\| b |",
            "| a\\\\| b |\n| --- |\n| x |",
        ] {
            let result = crate::migrate_markdown(source);
            let actual: Vec<_> = result
                .report
                .diagnostics
                .iter()
                .map(|row| (row.code.as_str(), row.path.as_deref(), row.fidelity))
                .collect();
            // cmark-gfm keeps the pipe in the cell after dropping one backslash.
            assert!(
                actual.iter().all(|row| row.0 != "fidelity-unverified"),
                "{source}: {actual:?}"
            );
        }
        let result = crate::migrate_markdown("a\\|b");
        let actual: Vec<_> = result
            .report
            .diagnostics
            .iter()
            .map(|row| {
                (
                    row.code.as_str(),
                    row.path.as_deref().unwrap_or(""),
                    row.fidelity,
                )
            })
            .collect();
        assert_eq!(
            actual,
            vec![
                ("markdown-paragraph", "line:1", MigrationFidelity::Preserved),
                ("markdown-escape", "line:1", MigrationFidelity::Normalized),
            ]
        );
    }

    #[test]
    fn inline_link_with_literal_destination_pipe_is_assessed() {
        let result = crate::migrate_markdown("[c](/a|b)");
        assert_eq!(result.value, "[c](/a|b)\n");
        assert_eq!(result.report.diagnostics.len(), 2);
        for (row, code) in result
            .report
            .diagnostics
            .iter()
            .zip(["markdown-paragraph", "markdown-link"])
        {
            assert_eq!(row.code, code);
            assert_eq!(row.severity, HtmlImportSeverity::Info);
            assert_eq!(row.fidelity, MigrationFidelity::Preserved);
            assert_eq!(row.confidence, MigrationConfidence::Exact);
            assert_eq!(row.path.as_deref(), Some("line:1"));
        }
    }

    #[test]
    fn ordinary_constructs_have_exact_source_locations() {
        for (source, code) in [
            ("# heading", "markdown-atx-heading"),
            ("heading\n=======", "markdown-setext-heading"),
            ("**strong**", "markdown-strong"),
            ("~~gone~~", "markdown-strikethrough"),
            ("a~b~c", "markdown-strikethrough"),
            ("~a~b~", "markdown-strikethrough"),
            ("€~a~b~", "markdown-strikethrough"),
            ("    code", "markdown-indented-code"),
            ("[label](https://example.org)", "markdown-link"),
            ("<https://example.org>", "markdown-autolink"),
            ("x &amp; y", "markdown-entity"),
            ("x\\!", "markdown-escape"),
            ("a  \nb", "markdown-hard-break"),
            ("- [x] done\n", "markdown-bullet-task"),
            ("| A | B |\n| --- | --- |\n| x | y |", "markdown-table"),
        ] {
            let result = crate::migrate_markdown(source);
            let row = result
                .report
                .diagnostics
                .iter()
                .find(|row| row.code == code)
                .expect(source);
            assert_eq!(row.path.as_deref(), Some("line:1"), "{source}");
            assert_eq!(row.confidence, MigrationConfidence::Exact);
            assert!(
                !result
                    .report
                    .diagnostics
                    .iter()
                    .any(|row| row.code == "fidelity-unverified"),
                "{source}"
            );
        }
    }

    #[test]
    fn code_markers_are_content_and_crlf_locations_are_original() {
        let result = crate::migrate_markdown("```\r\n1. [x] **code**\r\n```\r\n\r\n**text**");
        let rows: Vec<_> = result
            .report
            .diagnostics
            .iter()
            .map(|row| (row.code.as_str(), row.path.as_deref()))
            .collect();
        assert_eq!(
            rows,
            vec![
                ("markdown-fenced-code", Some("line:1")),
                ("markdown-paragraph", Some("line:5")),
                ("markdown-strong", Some("line:5"))
            ]
        );
    }

    #[test]
    fn ordered_task_losses_keep_source_lines() {
        let result = crate::migrate_markdown("```\ncode\n```\n\n1. [x] done\n2. [ ] next\n");
        let losses: Vec<_> = result
            .report
            .diagnostics
            .iter()
            .filter(|row| row.fidelity == MigrationFidelity::Dropped)
            .map(|row| (row.code.as_str(), row.path.as_deref()))
            .collect();
        assert_eq!(
            losses,
            vec![
                ("structure-unspellable", Some("line:5")),
                ("structure-unspellable", Some("line:6"))
            ]
        );
        assert!(!result
            .report
            .diagnostics
            .iter()
            .any(|row| row.code == "fidelity-unverified"));
    }

    #[test]
    fn setext_hashtags_and_multiline_html_blocks_have_one_construct_row() {
        let result = crate::migrate_markdown("#hashtag\n===\n");
        assert!(result
            .report
            .diagnostics
            .iter()
            .any(|row| row.code == "markdown-setext-heading"
                && row.fidelity == MigrationFidelity::Normalized));
        assert!(!result
            .report
            .diagnostics
            .iter()
            .any(|row| row.code == "markdown-atx-heading"));
        let result = crate::migrate_markdown("<div>\nx\n</div>\n");
        let raw: Vec<_> = result
            .report
            .diagnostics
            .iter()
            .filter(|row| row.code == "raw-preserved")
            .collect();
        assert_eq!(raw.len(), 1);
        assert_eq!(raw[0].path.as_deref(), Some("line:1"));
    }

    #[test]
    fn duplicate_definitions_remain_unverified_and_code_definitions_are_content() {
        let result = crate::migrate_markdown(
            "[ref]: https://one.example\n[ref]: https://two.example\n\nSee [ref].",
        );
        let assessment = assess(
            "[ref]: https://one.example\n[ref]: https://two.example\n\nSee [ref].",
            &result.value,
        );
        assert!(!assessment.complete);
        assert!(assessment.diagnostics.is_empty());
        assert!(result
            .report
            .diagnostics
            .iter()
            .any(|row| row.code == "fidelity-unverified"));
        for source in ["- a\n  [x]: /u\n", "| a |\n| --- |\n[x]: /u\n"] {
            let result = crate::migrate_markdown(source);
            assert!(!result
                .report
                .diagnostics
                .iter()
                .any(|row| row.code == "markdown-reference-definition"));
        }
        for source in [
            "- [a](/u \"x\n  [t]: /v\n  \")\n",
            "[a](/u \"x\n[t]: /v\n\")\n===\n",
        ] {
            let result = crate::migrate_markdown(source);
            assert!(!result
                .report
                .diagnostics
                .iter()
                .any(|row| row.code == "markdown-reference-definition"));
        }
        for source in ["[x]: /a\n\n- b\n\n  [x]: /b\n", "[x]: /a\n\n> [x]: /b\n"] {
            let result = crate::migrate_markdown(source);
            assert!(result
                .report
                .diagnostics
                .iter()
                .any(|row| row.code == "fidelity-unverified"));
        }
        let multiline = crate::migrate_markdown("[x]: /a\n[x]: /b\n\"t\n[y]: /c\n\"\n");
        assert!(!multiline
            .report
            .diagnostics
            .iter()
            .any(|row| row.code == "markdown-reference-definition"
                && row.path.as_deref() == Some("line:4")));
        assert!(multiline
            .report
            .diagnostics
            .iter()
            .any(|row| row.code == "fidelity-unverified"));
        let duplicate = crate::migrate_markdown("[x]: /a\n\n- [x]: /b\n");
        assert!(duplicate
            .report
            .diagnostics
            .iter()
            .any(|row| row.code == "fidelity-unverified"));
        let code = crate::migrate_markdown("```\n[ref]: https://example.org\n```\n");
        assert_eq!(code.report.diagnostics.len(), 1);
        assert_eq!(code.report.diagnostics[0].code, "markdown-fenced-code");
    }

    #[test]
    fn literal_entity_spellings_are_not_reported_as_decoded_entities() {
        for source in [
            "&bogus;",
            "&notit;",
            "&#11141111;",
            "&#x1234567;",
            "<https://example.org/&copy;>",
        ] {
            let result = crate::migrate_markdown(source);
            assert!(
                !result
                    .report
                    .diagnostics
                    .iter()
                    .any(|row| row.code == "markdown-entity"),
                "{source}"
            );
        }
        let result = crate::migrate_markdown("x\\\\!");
        assert_eq!(
            result
                .report
                .diagnostics
                .iter()
                .filter(|row| row.code == "markdown-escape")
                .count(),
            1
        );
    }

    #[test]
    fn gfm_protocol_links_have_exact_evidence() {
        for source in [
            "ftp://example.test",
            "FTP://example.test",
            "Ftp://example.test",
            "HTTP://example.test",
            "HTTPS://example.test",
            "see ftp://example.test/a",
            "ftp://x",
        ] {
            let result = crate::migrate_markdown(source);
            assert!(
                !result
                    .report
                    .diagnostics
                    .iter()
                    .any(|row| row.code == "fidelity-unverified"),
                "{source:?}"
            );
            let link = result
                .report
                .diagnostics
                .iter()
                .find(|row| row.code == "markdown-bare-autolink")
                .expect(source);
            assert_eq!(
                link.fidelity,
                crate::MigrationFidelity::Normalized,
                "{source:?}"
            );
            assert_eq!(
                link.confidence,
                crate::MigrationConfidence::Exact,
                "{source:?}"
            );
        }
        for source in ["`ftp://example.test`", "```\nftp://example.test\n```"] {
            assert!(
                !crate::migrate_markdown(source)
                    .report
                    .diagnostics
                    .iter()
                    .any(|row| row.code == "fidelity-unverified"),
                "{source:?}"
            );
        }
    }

    #[test]
    fn non_gfm_protocols_and_existing_link_labels_remain_preserved() {
        for source in [
            "sftp://example.test",
            "tftp://example.test",
            "fooftp://example.test",
            "[ftp://example.test](https://target.test)",
            "[https://example.test](https://target.test)",
        ] {
            assert!(
                !crate::migrate_markdown(source)
                    .report
                    .diagnostics
                    .iter()
                    .any(|row| row.code == "fidelity-unverified"),
                "{source:?}"
            );
        }
    }

    #[test]
    fn unsupported_input_and_changed_output_fail_closed() {
        for source in [
            "<code>http://example.org</code>",
            "[^note]\n\n[^note]: note",
            "Text[^1]\n\n[^1]: note\n\n    1) item\n",
            "\0",
        ] {
            assert!(
                crate::migrate_markdown(source)
                    .report
                    .diagnostics
                    .iter()
                    .any(|row| row.code == "fidelity-unverified"),
                "{source:?}"
            );
        }
        assert!(!assess("**strong**", "wrong").complete);
        assert!(!assess("a  b", "a b").complete);
    }

    #[test]
    fn ordered_delimiter_evidence_cannot_be_normalized_away() {
        assert!(assess("1) one\n", "1) one\n").complete);
        assert!(assess("1. one\n", "1. one\n").complete);
        assert!(!assess("1) one\n", "1. one\n").complete);
        assert!(!assess("1. one\n", "1) one\n").complete);
        assert!(assess("10) foo\n    - bar\n", "10) foo\n    - bar\n").complete);
        assert!(!assess("10) foo\n    - bar\n", "10. foo\n    - bar\n").complete);
        assert!(!assess("10) a  b\n    - c\n", "10) a b\n    - c\n").complete);
        assert!(
            assess(
                "<ol data-delim=\")\"><li>one</li></ol>\n",
                "``` =html\n<ol data-delim=\")\"><li>one</li></ol>\n```\n"
            )
            .complete
        );
        assert!(!assess(
            "<ol>\n<li>one</li>\n</ol>\n\n<ol>\n<li>one</li>\n</ol>\n",
            "``` =html\n<ol data-delim=\")\">\n<li>one</li>\n</ol>\n\n<ol>\n<li>one</li>\n</ol>\n```\n"
        ).complete);
        assert!(
            !assess(
                "<ol data-delim=\")\"><li>one</li></ol>\n",
                "``` =html\n<ol><li>one</li></ol>\n```\n"
            )
            .complete
        );
    }
}
