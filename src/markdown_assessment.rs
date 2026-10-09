use crate::{
    HtmlImportSeverity, MigrationConfidence, MigrationDiagnostic, MigrationFidelity,
    SmartTypographyMode,
};
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use pulldown_cmark::{CodeBlockKind, Event, LinkType, Options, Parser, Tag, TagEnd};
use std::{collections::BTreeSet, sync::OnceLock};

pub(crate) struct Assessment {
    pub diagnostics: Vec<MigrationDiagnostic>,
    pub complete: bool,
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

/// Assess the native parser's occurrences and verify the document the writer produced.
pub(crate) fn assess(source: &str, value: &str) -> Assessment {
    use crate::html_import::ImportFidelity::{Degraded, Dropped, Normalized, Preserved};
    if source.len() > 1_000_000 || source.contains('\0') {
        return Assessment {
            diagnostics: Vec::new(),
            complete: false,
        };
    }
    // Parser offsets refer to this normalized input; normalization never adds a line.
    let mut source = source.replace("\r\n", "\n").replace('\r', "\n");
    if !source.is_empty() && !source.ends_with('\n') {
        source.push('\n');
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
    let parser = Parser::new_ext(&source, options);
    let mut diagnostics = Vec::new();
    let mut complete = true;
    let mut definition_spans = Vec::new();
    for (_, definition) in parser.reference_definitions().iter() {
        definition_spans.push(definition.span.clone());
        diagnostics.push(diagnostic(
            "reference-definition",
            &line_starts,
            definition.span.start,
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
    let mut html_block = false;
    let mut html_fragments = 0usize;
    static BARE: OnceLock<regex::Regex> = OnceLock::new();
    static REWRITE: OnceLock<regex::Regex> = OnceLock::new();
    let bare = BARE.get_or_init(|| {
        regex::Regex::new(r"(?:https?://|www\.|[\w.+-]+@[\w.-]+\.[A-Za-z])").unwrap()
    });
    let rewrite = REWRITE.get_or_init(|| regex::Regex::new(r"&(?:#[xX][\da-fA-F]+|#\d+|[A-Za-z][A-Za-z\d]+);|\\[!\x22#$%&'()*+,\-./:;<=>?@\[\]\\^_`{|}~]").unwrap());
    for (event, range) in parser.into_offset_iter() {
        let mut paren_list = false;
        if !matches!(event, Event::Start(_) | Event::End(_)) {
            content_spans.push(range.clone());
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
                    Tag::Image { link_type, .. } => Some(match link_type {
                        LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut => {
                            ("reference-link", Normalized)
                        }
                        _ => ("image", Preserved),
                    }),
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
            Event::End(TagEnd::Link) => {
                link_depth = link_depth.saturating_sub(1);
            }
            Event::End(TagEnd::CodeBlock) => {
                code_depth = code_depth.saturating_sub(1);
            }
            Event::Text(text) if code_depth == 0 => {
                let input = &source[range.clone()];
                if range.start > 0
                    && source[..range.start]
                        .bytes()
                        .rev()
                        .take_while(|byte| *byte == b'\\')
                        .count()
                        % 2
                        == 1
                    && input.starts_with(|character: char| character.is_ascii_punctuation())
                {
                    diagnostics.push(diagnostic(
                        "escape",
                        &line_starts,
                        range.start - 1,
                        Normalized,
                    ));
                }
                if link_depth == 0 && bare.is_match(input) {
                    complete = false;
                }
                for matched in rewrite.find_iter(input) {
                    if text.as_ref() == input {
                        continue;
                    }
                    diagnostics.push(diagnostic(
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
                        diagnostics.push(diagnostic(
                            "ordered-task",
                            &line_starts,
                            range.start,
                            Dropped,
                        ));
                    }
                    events.push(Event::Text(source[range.clone()].into()));
                    let separator: String = source[range.end..]
                        .chars()
                        .take_while(|character| matches!(character, ' ' | '\t'))
                        .collect();
                    if !separator.is_empty() {
                        events.push(Event::Text(separator.into()));
                    } else if source[range.end..].starts_with('\n') {
                        events.push(Event::SoftBreak);
                    }
                    continue;
                }
                diagnostics.push(diagnostic(
                    "bullet-task",
                    &line_starts,
                    range.start,
                    Preserved,
                ));
                events.push(event);
                events.push(Event::Text(" ".into()));
                continue;
            }
            Event::FootnoteReference(_) | Event::InlineMath(_) | Event::DisplayMath(_) => {
                complete = false;
            }
            _ => {}
        }
        if let Some((construct, fidelity)) = assessment {
            diagnostics.push(diagnostic(construct, &line_starts, range.start, fidelity));
        }
        events.push(event);
        if paren_list {
            events.push(Event::Html(format!("<!--{expected_list_marker}-->").into()));
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
    if complete {
        let mut expected = String::new();
        pulldown_cmark::html::push_html(&mut expected, events.into_iter());
        expected = expected
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
    diagnostics.sort_by_key(|row| {
        row.path
            .as_ref()
            .and_then(|path| path.strip_prefix("line:"))
            .and_then(|line| line.parse::<usize>().ok())
            .unwrap_or(0)
    });
    Assessment {
        diagnostics: if complete { diagnostics } else { Vec::new() },
        complete,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_constructs_have_exact_source_locations() {
        for (source, code) in [
            ("# heading", "markdown-atx-heading"),
            ("heading\n=======", "markdown-setext-heading"),
            ("**strong**", "markdown-strong"),
            ("~~gone~~", "markdown-strikethrough"),
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
    fn unsupported_input_and_changed_output_fail_closed() {
        for source in [
            "https://example.org",
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
