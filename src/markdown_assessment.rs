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
    source: &str,
    offset: usize,
    fidelity: MigrationFidelity,
) -> MigrationDiagnostic {
    let line = source[..offset]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
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
                let tag = name.local.to_string();
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
    let source = source.replace("\r\n", "\n").replace('\r', "\n");
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
            &source,
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
                    Tag::Image { .. } => Some(("image", Preserved)),
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
                        &source,
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
                    || html.to_lowercase().contains("<h1")
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
                    diagnostics.push(diagnostic("ordered-task", &source, range.start, Dropped));
                    events.push(Event::Text(if *checked {
                        "[x] ".into()
                    } else {
                        "[ ] ".into()
                    }));
                    continue;
                }
                assessment = Some(("bullet-task", Preserved));
            }
            Event::FootnoteReference(_) | Event::InlineMath(_) | Event::DisplayMath(_) => {
                complete = false;
            }
            _ => {}
        }
        if let Some((construct, fidelity)) = assessment {
            diagnostics.push(diagnostic(construct, &source, range.start, fidelity));
        }
        events.push(event);
    }
    if complete {
        let mut expected = String::new();
        pulldown_cmark::html::push_html(&mut expected, events.into_iter());
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
