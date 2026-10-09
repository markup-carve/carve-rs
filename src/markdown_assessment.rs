use crate::{
    HtmlImportSeverity, MigrationConfidence, MigrationDiagnostic, MigrationFidelity,
    SmartTypographyMode,
};
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use pulldown_cmark::{CodeBlockKind, Event, LinkType, Options, Parser, Tag, TagEnd};
use std::sync::OnceLock;

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

fn shape(html: &str) -> Vec<Shape> {
    fn visit(node: &Handle) -> Vec<Shape> {
        match &node.data {
            NodeData::Text { contents } => vec![Shape::Text(contents.borrow().to_string())],
            NodeData::Comment { contents } => vec![Shape::Comment(contents.to_string())],
            NodeData::Element { name, attrs, .. } => {
                let tag = if name.local.as_ref() == "del" {
                    "s".to_owned()
                } else {
                    name.local.to_string()
                };
                let mut children: Vec<_> = node.children.borrow().iter().flat_map(visit).collect();
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
                attributes.sort();
                vec![Shape::Element(tag, attributes, children)]
            }
            _ => node.children.borrow().iter().flat_map(visit).collect(),
        }
    }
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    visit(&dom.document)
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
    line_starts.extend(source.bytes().enumerate().filter_map(|(index, byte)| (byte == b'\n').then_some(index + 1)));
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
    for (_, definition) in parser.reference_definitions().iter() {
        diagnostics.push(diagnostic(
            "reference-definition",
            &line_starts,
            definition.span.start,
            Normalized,
        ));
    }
    let mut events = Vec::new();
    let mut lists = Vec::new();
    let mut link_depth = 0usize;
    let mut code_depth = 0usize;
    static BARE: OnceLock<regex::Regex> = OnceLock::new();
    static REWRITE: OnceLock<regex::Regex> = OnceLock::new();
    let bare = BARE.get_or_init(|| {
        regex::Regex::new(r"(?:https?://|www\.|[\w.+-]+@[\w.-]+\.[A-Za-z])").unwrap()
    });
    let rewrite = REWRITE.get_or_init(|| regex::Regex::new(r"&(?:#[xX][\da-fA-F]+|#\d+|[A-Za-z][A-Za-z\d]+);|\\[!\x22#$%&'()*+,\-./:;<=>?@\[\]\\^_`{|}~]").unwrap());
    for (event, range) in parser.into_offset_iter() {
        let mut assessment = None;
        match &event {
            Event::Start(tag) => {
                assessment = match tag {
                    Tag::Paragraph => Some(("paragraph", Preserved)),
                    Tag::Heading { .. } => Some((
                        if source[range.clone()].trim_start().starts_with('#') {
                            "atx-heading"
                        } else {
                            "setext-heading"
                        },
                        if source[range.clone()].trim_start().starts_with('#') {
                            Preserved
                        } else {
                            Normalized
                        },
                    )),
                    Tag::BlockQuote(_) => Some(("block-quote", Preserved)),
                    Tag::List(start) => {
                        lists.push(start.is_some());
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
                    Tag::HtmlBlock => None,
                    _ => {
                        complete = false;
                        None
                    }
                };
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
            Event::Text(_) if code_depth == 0 => {
                let input = &source[range.clone()];
                if range.start > 0
                    && source[..range.start].bytes().rev().take_while(|byte| *byte == b'\\').count() % 2 == 1
                    && input.starts_with(|character: char| character.is_ascii_punctuation())
                {
                    diagnostics.push(diagnostic("escape", &line_starts, range.start - 1, Normalized));
                }
                if link_depth == 0 && bare.is_match(input) {
                    complete = false;
                }
                for matched in rewrite.find_iter(input) {
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
                if !value.contains(html.as_ref())
                    || html.to_lowercase().contains("<section")
                    || [
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
                        "</section",
                    ]
                    .iter()
                    .any(|tag| html.to_lowercase().contains(tag))
                    || html.to_lowercase().contains("<input")
                {
                    complete = false;
                }
                assessment = Some(("raw-html", Degraded));
            }
            Event::TaskListMarker(checked) => {
                if lists.last() == Some(&true)
                    && crate::markdown_import::tasklist_extension_reaches(&source, &range)
                {
                    diagnostics.push(diagnostic("ordered-task", &line_starts, range.start, Dropped));
                    events.push(Event::Text(if *checked {
                        "[x] ".into()
                    } else {
                        "[ ] ".into()
                    }));
                    continue;
                }
                diagnostics.push(diagnostic("bullet-task", &line_starts, range.start, Preserved));
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
        complete = shape(&expected) == shape(&actual);
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
    fn unsupported_input_and_changed_output_fail_closed() {
        for source in ["https://example.org", "[^note]\n\n[^note]: note", "\0"] {
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
}
