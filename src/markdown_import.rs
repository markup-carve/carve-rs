//! Markdown-to-Carve migration boundary.
//!
//! AST-first, the way [`crate::html_import`] is: the source is parsed to a real
//! syntax tree, walked into a Carve [`Document`], and written by the canonical
//! writer. carve-js and carve-php instead rewrite Markdown source line by line,
//! which is why they keep more source spelling. The tree records the authored
//! bullet and ordered delimiter before the canonical writer runs.
//!
//! Parsing to a tree is what makes the hard parts free. A line rewriter has to
//! carry fence state, a stack of list content columns and CommonMark's lazy
//! continuation rules by hand, and an off-by-one there silently re-bases a
//! fence into the wrong container. Here the parser owns all of it.
//!
//! ```
//! assert_eq!(carve::markdown_to_carve("*em* and **strong**"), "/em/ and *strong*\n");
//! ```

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::collections::{HashMap, HashSet};

use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{NodeData, RcDom};
use serde_json::Value;

use pulldown_cmark::{
    Alignment, CodeBlockKind, Event, HeadingLevel, LinkType, Options, Parser, Tag, TagEnd,
};

use crate::ast::*;
use crate::render_carve;

#[cfg(test)]
#[path = "markdown_import/markdown_native_destination_tests.rs"]
mod native_destination_tests;
#[path = "markdown_import/markdown_native_destinations.rs"]
mod native_destinations;

pub(crate) struct MarkdownImportLoss {
    pub message: String,
    pub line: Option<usize>,
    pub kind: MarkdownLossKind,
}

/// Which report row a loss becomes.
///
/// The code and the fidelity belong to the kind rather than to the producer:
/// `Unspellable` is a shape the syntax cannot spell at all and is dropped,
/// while `RawSpanWhitespaceTrimmed` keeps the span and loses a whitespace run
/// inside it, which is what degraded covers (markup-carve/carve#2804).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarkdownLossKind {
    Unspellable,
    RawSpanWhitespaceTrimmed,
    /// PART 11 §10s: a carrier marker set that records no structure.
    CarrierMarkersDamaged,
    CarrierMarkerDropped,
    RawCodeFallback,
}

/// A raw span whose content ends a line in whitespace, which CARVE-P2-025
/// drops from every content line - a verbatim run crossing a line break
/// included.
/// A carrier marker set the import will not guess at: a marker deleted, two
/// reordered or a set left unbalanced (PART 11 §10s).
pub(crate) const CARRIER_MARKERS_DAMAGED: &str =
    "A carrier marker set does not record a container: the markers imported as \
     the raw HTML they are and no container was reconstructed";

/// Whitespace a decoded character reference put at the head of a block, which
/// Carve has no spelling for there.
///
/// Dropped rather than substituted, which markup-carve/carve#2595 settled: `\ `
/// reads back as U+00A0, and a non-breaking space is not the tab or space the
/// author wrote. A deliberate drop is still a loss, and `--report` names what
/// left (carve-rs#2446).
pub(crate) const LEADING_WHITESPACE_UNSPELLABLE: &str =
    "Dropped whitespace a decoded reference put at the start of a line; \
     Carve spells no leading whitespace on a paragraph";

/// What the reader drops at the end of a content line inside a verbatim run.
const RAW_SPAN_DROPPED_WHITESPACE: [char; 2] = [' ', '\t'];

pub(crate) const RAW_SPAN_WHITESPACE_TRIMMED: &str =
    "A raw span ends a content line in whitespace, which Carve drops; \
     the whitespace did not reach the converted source";

/// A front matter block the importer claimed, and whether its opener named the
/// format.
///
/// A typed opener is front matter unconditionally (CARVE-P2-030), so the
/// reading was declared rather than derived, and the report says `exact` where
/// a bare `---` claimed by the shape test says `inferred`
/// (markup-carve/carve#2806).
pub(crate) struct FrontmatterNotice {
    pub message: String,
    pub typed_opener: bool,
}

thread_local! {
    /// CARVE-P2-031: the destination each converted link had in the Markdown
    /// source, keyed by the destination the conversion wrote. `None` marks two
    /// spellings, which the report cannot tell apart.
    static AUTHORED_DESTINATIONS: std::cell::RefCell<Option<HashMap<String, Option<String>>>> =
        const { std::cell::RefCell::new(None) };
}

/// Run a conversion and collect the destinations it changed, mapped back to
/// the spelling the author wrote.
pub(crate) fn with_authored_destinations<T>(
    run: impl FnOnce() -> T,
) -> (T, HashMap<String, String>) {
    let outer = AUTHORED_DESTINATIONS.with(|cell| cell.replace(Some(HashMap::new())));
    let value = run();
    let recorded = AUTHORED_DESTINATIONS
        .with(|cell| cell.replace(outer))
        .unwrap_or_default();
    let authored = recorded
        .into_iter()
        .filter_map(|(encoded, authored)| authored.filter(|a| *a != encoded).map(|a| (encoded, a)))
        .collect();
    (value, authored)
}

fn record_authored_destination(encoded: &str, authored: &str) {
    AUTHORED_DESTINATIONS.with(|cell| {
        if let Some(map) = cell.borrow_mut().as_mut() {
            map.entry(encoded.to_owned())
                .and_modify(|known| {
                    if known.as_deref() != Some(authored) {
                        *known = None;
                    }
                })
                .or_insert_with(|| Some(authored.to_owned()));
        }
    });
}

/// The destination of an inline link or image as its source wrote it, when
/// it reads back as `parsed`; otherwise `parsed`.
fn authored_inline_destination<'a>(source: &'a str, parsed: &'a str) -> &'a str {
    let Some(rest) = source
        .rfind("](")
        .and_then(|at| source[at + 2..].strip_suffix(')'))
    else {
        return parsed;
    };
    let rest = rest.trim_start_matches([' ', '\t', '\n']);
    let raw = match rest.strip_prefix('<') {
        Some(inner) => inner.split('>').next().unwrap_or(""),
        None => rest.split([' ', '\t', '\n']).next().unwrap_or(""),
    };
    let mut unescaped = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek().is_some_and(char::is_ascii_punctuation) {
            continue;
        }
        unescaped.push(c);
    }
    if unescaped == parsed {
        raw
    } else {
        parsed
    }
}

pub(crate) fn markdown_destination(destination: &str, email: bool) -> String {
    encode_markdown_destination(destination, email)
}

pub(crate) fn markdown_autolink_destination(destination: &str, email: bool) -> String {
    encode_markdown_destination(destination, email)
}

pub(crate) fn markdown_bare_link_destination(destination: &str) -> String {
    encode_markdown_destination(destination, false)
}

fn encode_markdown_destination(destination: &str, email: bool) -> String {
    if is_empty_destination(destination) {
        return String::new();
    }
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(destination.len());
    if email {
        encoded.push_str("mailto:");
    }
    for ch in destination.chars() {
        if ch.is_whitespace() || ch.is_control() {
            let mut buffer = [0; 4];
            for byte in ch.encode_utf8(&mut buffer).bytes() {
                encoded.push('%');
                encoded.push(HEX[(byte >> 4) as usize] as char);
                encoded.push(HEX[(byte & 15) as usize] as char);
            }
        } else {
            encoded.push(ch);
        }
    }
    encoded
}

pub(crate) fn literal_bare_link(label: &str, prefix: &str) -> InlineNode {
    InlineNode::Link(Link {
        attrs: None,
        href: markdown_bare_link_destination(&format!("{prefix}{label}")),
        title: None,
        children: vec![InlineNode::text(label)],
        ref_label: None,
        raw_ref: None,
        resolved_empty_reference: false,
        from_crossref: false,
        from_heading_reference: false,
        pos: None,
    })
}

fn fence_language(info: &str) -> Option<&str> {
    info.trim_matches([' ', '\t'])
        .split([' ', '\t'])
        .next()
        .filter(|word| {
            !word.is_empty()
                && word
                    .bytes()
                    .all(|ch| ch.is_ascii_alphanumeric() || b"_+#/.-".contains(&ch))
        })
}

fn is_empty_destination(destination: &str) -> bool {
    destination.is_empty()
}

/// Convert Markdown to Carve. Use [`try_markdown_to_carve`] to handle a writer
/// refusal without a panic.
///
/// GFM tables, strikethrough and task lists are enabled: they are what real
/// Markdown documents carry, and Carve has a spelling for each.
///
/// Footnotes and YAML frontmatter are enabled for a second reason - leaving
/// them OFF is not neutral, it corrupts. Without footnotes, `[^1]: Note.` is a
/// link-reference definition and `Text[^1]` a shortcut link to it, so the note
/// became the destination: `Text[^1](Note.)`. Without metadata blocks, a
/// `---` fence is a thematic break and the key line beneath it a setext
/// heading, so `title: T` became an `<h2>`. Both were caught by the
/// differential against carve-js, not by reasoning.
///
/// # Panics
///
/// Panics if the canonical writer cannot represent the imported document.
pub fn markdown_to_carve(markdown: &str) -> String {
    try_markdown_to_carve(markdown).expect("the Markdown import cannot be written as Carve")
}

/// Convert Markdown source without hiding a canonical-writer refusal.
pub fn try_markdown_to_carve(markdown: &str) -> Result<String, crate::RenderCarveError> {
    markdown_to_carve_with_losses(markdown).map(|(value, _, _)| value)
}

pub(crate) fn markdown_to_carve_with_losses(
    markdown: &str,
) -> Result<(String, Vec<MarkdownImportLoss>, Vec<FrontmatterNotice>), crate::RenderCarveError> {
    markdown_to_carve_with_losses_mode(markdown, true)
}

pub(crate) fn markdown_to_carve_with_losses_mode(
    markdown: &str,
    rewrite_heading_fragments: bool,
) -> Result<(String, Vec<MarkdownImportLoss>, Vec<FrontmatterNotice>), crate::RenderCarveError> {
    // PART 11 §10s: a carrier marker is lifted to a placeholder before the
    // conversion and written back as the Carve line it is afterwards, so the
    // Markdown reading in between never sees a comment it would carry across as
    // raw HTML.
    markdown_to_carve_with_carriers(markdown, true, rewrite_heading_fragments)
}

#[doc(hidden)]
mod heading_links;
pub use heading_links::migration_raw_heading_ids;
use heading_links::rewrite_same_file_heading_links;
pub(crate) use heading_links::rewrite_same_file_heading_links_reported;

fn protect_escaped_colon_lines(source: &str) -> (String, Option<String>) {
    if !source.contains("\\:") {
        return (source.to_owned(), None);
    }
    let mut token = "CARVEESCAPEDCOLONLINE".to_owned();
    while source.contains(&token) {
        token.push('X');
    }
    let mut out = String::with_capacity(source.len());
    let mut changed = false;
    for line in source.split_inclusive('\n') {
        let prefix = line
            .bytes()
            .take_while(|byte| matches!(byte, b' ' | b'\t' | b'>'))
            .count();
        let rest = &line[prefix..];
        if rest.starts_with("\\: ")
            || rest.starts_with("\\:\t")
            || rest.starts_with("\\:: ")
            || rest.starts_with("\\::\t")
            || matches!(rest.trim_end_matches(['\r', '\n']), "\\:" | "\\::")
        {
            out.push_str(&line[..prefix]);
            out.push_str(&token);
            out.push_str(&rest[2..]);
            changed = true;
        } else {
            out.push_str(line);
        }
    }
    (out, changed.then_some(token))
}

fn markdown_to_carve_with_carriers(
    markdown: &str,
    enabled: bool,
    rewrite_heading_fragments: bool,
) -> Result<(String, Vec<MarkdownImportLoss>, Vec<FrontmatterNotice>), crate::RenderCarveError> {
    let (protected, escaped_colon) =
        if enabled && markdown.contains(crate::carrier_markers::CARRIER_PREFIX) {
            protect_escaped_colon_lines(markdown)
        } else {
            (markdown.to_owned(), None)
        };
    let inline = if enabled {
        crate::inline_carriers::InlineCarriers::prepare(&protected)
    } else {
        crate::inline_carriers::InlineCarriers::plain(&protected)
    };
    let lift = if enabled {
        crate::carrier_markers::lift_carrier_markers(&inline.source)
    } else {
        let mut lift = crate::carrier_markers::lift_carrier_markers("");
        lift.source = markdown.to_owned();
        lift
    };
    if enabled && lift.damaged {
        let (value, mut losses, notices) =
            markdown_to_carve_with_carriers(markdown, false, rewrite_heading_fragments)?;
        losses.push(MarkdownImportLoss {
            message: CARRIER_MARKERS_DAMAGED.to_owned(),
            line: None,
            kind: MarkdownLossKind::CarrierMarkersDamaged,
        });
        return Ok((value, losses, notices));
    }
    let finish =
        |value: String, mut losses: Vec<MarkdownImportLoss>, notices: Vec<FrontmatterNotice>| {
            let value = restore_markdown_abbreviations(value, markdown);
            let source = crate::carrier_markers::restore_carrier_markers(&value, &lift);
            let source = crate::carrier_markers::restore_nested_list_styles(&source, &lift);
            let (restored, dropped) = source
                .as_deref()
                .map(|source| inline.restore(source))
                .unwrap_or((None, Vec::new()));
            if let Some(value) = restored {
                let value = restore_table_email_brackets(value, markdown);
                let value = if let Some(token) = &escaped_colon {
                    value.replace(token, "\\:")
                } else {
                    value
                };
                losses.extend(
                    dropped
                        .into_iter()
                        .map(|(line, message)| MarkdownImportLoss {
                            message,
                            line: Some(line),
                            kind: MarkdownLossKind::CarrierMarkersDamaged,
                        }),
                );
                Ok((
                    if rewrite_heading_fragments {
                        rewrite_same_file_heading_links(value)?
                    } else {
                        value
                    },
                    losses,
                    notices,
                ))
            } else {
                let (value, mut losses, notices) =
                    markdown_to_carve_with_carriers(markdown, false, rewrite_heading_fragments)?;
                losses.push(MarkdownImportLoss {
                    message: CARRIER_MARKERS_DAMAGED.to_owned(),
                    line: None,
                    kind: MarkdownLossKind::CarrierMarkersDamaged,
                });
                Ok((value, losses, notices))
            }
        };
    let mut code_lines = Vec::new();
    let (mut document, mut losses, notices) =
        markdown_to_ast_with_code_lines(&lift.source, Some(&mut code_lines), Some(&lift.token))?;
    losses.extend(
        lift.dropped
            .iter()
            .map(|(line, message)| MarkdownImportLoss {
                message: message.clone(),
                line: Some(*line),
                kind: MarkdownLossKind::CarrierMarkerDropped,
            }),
    );
    if lift.damaged {
        losses.push(MarkdownImportLoss {
            message: CARRIER_MARKERS_DAMAGED.to_owned(),
            line: None,
            kind: MarkdownLossKind::CarrierMarkersDamaged,
        });
    }
    // ONLY ON THE WRITING PATH, as the HTML importer does it: the AST keeps the
    // nesting the source carried, and only a Carve SPELLING has to give it up
    // (carve-rs#2098).
    losses.extend(
        crate::html_import::unwrap_same_kind_spans_for_writing(&mut document)
            .into_iter()
            .map(|message| MarkdownImportLoss {
                message,
                line: None,
                kind: MarkdownLossKind::Unspellable,
            }),
    );
    losses.extend(
        drop_leading_decoded_whitespace(&mut document)
            .into_iter()
            .map(|message| MarkdownImportLoss {
                message,
                line: None,
                kind: MarkdownLossKind::Unspellable,
            }),
    );
    trim_raw_span_line_ends(&mut document);
    let expected_code = document.markdown_code_values();
    if code_lines.len() != expected_code.len() {
        code_lines = vec![None; expected_code.len()];
    }
    match render_carve(&document) {
        Ok(value)
            if expected_code.is_empty()
                || expected_code == crate::parse::parse(&value).markdown_code_values() =>
        {
            finish(value, losses, notices)
        }
        Ok(_) | Err(crate::RenderCarveError::SourceUnspellable(_)) => {
            losses.extend(preserve_html_code_for_writing(
                &mut document,
                false,
                &mut code_lines,
            ));
            let value = match render_carve(&document) {
                Ok(value)
                    if document.markdown_code_values()
                        == crate::parse::parse(&value).markdown_code_values() =>
                {
                    value
                }
                Ok(_) | Err(crate::RenderCarveError::SourceUnspellable(_)) => {
                    losses.extend(preserve_html_code_for_writing(
                        &mut document,
                        true,
                        &mut code_lines,
                    ));
                    render_carve(&document)?
                }
                Err(error) => return Err(error),
            };
            if document.markdown_code_values() != crate::parse::parse(&value).markdown_code_values()
            {
                return Err(crate::RenderCarveError::SourceUnspellable(
                    crate::render_carve_error::SourceUnspellable::new(
                        "code",
                        "the imported code payload does not survive source readback",
                    ),
                ));
            }
            finish(value, losses, notices)
        }
        Err(error) => Err(error),
    }
}

fn restore_markdown_abbreviations(mut value: String, markdown: &str) -> String {
    let mut definitions = BTreeMap::new();
    let mut may_define = true;
    for line in markdown.lines() {
        let Some(rest) = line.strip_prefix("*[") else {
            may_define = line.trim().is_empty();
            continue;
        };
        if !may_define {
            continue;
        }
        let Some((term, expansion)) = rest.split_once("]: ") else {
            may_define = false;
            continue;
        };
        if term.is_empty()
            || !term.bytes().all(|byte| byte.is_ascii_alphanumeric())
            || expansion.is_empty()
        {
            may_define = false;
            continue;
        }
        let expansion = expansion
            .trim_start_matches(' ')
            .trim_end_matches([' ', '\t']);
        let escaped = format!("\\*[{term}]: {expansion}");
        if let Some(at) = value.find(&escaped) {
            value.replace_range(at..at + escaped.len(), &format!("*[{term}]: {expansion}"));
            definitions
                .entry(term.to_owned())
                .or_insert_with(|| expansion.to_owned());
            may_define = true;
        } else {
            may_define = false;
        }
    }
    for (term, expansion) in definitions {
        let html = format!("`<abbr title=\"{expansion}\">`{{=html}}{term}`</abbr>`{{=html}}");
        value = value.replace(&html, &term);
        let pattern = format!(
            "{}([^\"]*){}{}{}",
            regex::escape("`<abbr title=\""),
            regex::escape("\">`{=html}"),
            regex::escape(&term),
            regex::escape("`</abbr>`{=html}")
        );
        if let Ok(pattern) = regex::Regex::new(&pattern) {
            value = pattern
                .replace_all(&value, |captures: &regex::Captures<'_>| {
                    format!("`<abbr title=\"{}\">{term}</abbr>`{{=html}}", &captures[1])
                })
                .into_owned();
        }
    }
    value
}

fn normalize_imported_reference_links(document: &mut Document) {
    use crate::include_walk::{visit_block_children, visit_inline_children, SubtreeVisitor};
    struct Normalize {
        definitions: BTreeMap<String, LinkReferenceDefinition>,
        preserved: BTreeSet<String>,
    }
    impl SubtreeVisitor for Normalize {
        fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
            for block in blocks {
                visit_block_children(block, self);
            }
        }
        fn inlines(&mut self, inlines: &mut Vec<InlineNode>) {
            for inline in inlines {
                if let InlineNode::Link(link) = inline {
                    if let (Some(label), Some(raw)) = (&link.ref_label, &link.raw_ref) {
                        if let Some(definition) = self
                            .definitions
                            .get(&crate::parse::label_key(label).to_uppercase())
                        {
                            let canonical = &definition.label;
                            let text = raw
                                .strip_prefix('[')
                                .and_then(|raw| raw.split_once(']'))
                                .map(|(text, _)| text);
                            let plain = text.is_some_and(|text| {
                                matches!(link.children.as_slice(), [InlineNode::Text(child)] if child.value == text)
                            });
                            // The Carve writer has no spelling for a definition line
                            // whose fields span lines, so its uses are inlined.
                            let spellable = !definition.href.chars().any(char::is_whitespace)
                                && !definition
                                    .title
                                    .as_deref()
                                    .is_some_and(|title| title.contains(['\r', '\n']));
                            if spellable
                                && !canonical.contains('|')
                                && !text.is_some_and(|text| text.contains('|'))
                                && link.href == definition.href
                                && link.title == definition.title
                                && plain
                            {
                                let text = text.unwrap();
                                link.raw_ref = Some(if text == canonical {
                                    format!("[{canonical}][]")
                                } else {
                                    format!("[{text}][{canonical}]")
                                });
                                link.ref_label = Some(canonical.clone());
                                self.preserved
                                    .insert(crate::parse::label_key(canonical).to_uppercase());
                            } else {
                                link.ref_label = None;
                                link.raw_ref = None;
                            }
                        } else if !link.href.is_empty()
                            || link.title.is_some()
                            || link.resolved_empty_reference
                        {
                            link.ref_label = None;
                            link.raw_ref = None;
                        }
                    }
                }
                visit_inline_children(inline, self);
            }
        }
    }
    let labels = document
        .children
        .iter()
        .filter_map(|block| {
            let BlockNode::LinkReferenceDefinition(def) = block else {
                return None;
            };
            Some((
                crate::parse::label_key(&def.label).to_uppercase(),
                def.clone(),
            ))
        })
        .collect();
    let mut normalize = Normalize {
        definitions: labels,
        preserved: BTreeSet::new(),
    };
    normalize.blocks(&mut document.children);
    document.children.retain(|block| {
        !matches!(block, BlockNode::LinkReferenceDefinition(def)
            if !normalize.preserved.contains(&crate::parse::label_key(&def.label).to_uppercase()))
    });
}

fn encode_table_destination_pipes(document: &mut Document) {
    use crate::include_walk::{visit_block_children, visit_inline_children, SubtreeVisitor};
    struct Encode {
        in_table: bool,
    }
    impl SubtreeVisitor for Encode {
        fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
            for block in blocks {
                let previous = self.in_table;
                if matches!(block, BlockNode::Table(_)) {
                    self.in_table = true;
                }
                visit_block_children(block, self);
                self.in_table = previous;
            }
        }
        fn inlines(&mut self, inlines: &mut Vec<InlineNode>) {
            for inline in inlines {
                if self.in_table {
                    match inline {
                        InlineNode::Link(link) => link.href = link.href.replace('|', "%7C"),
                        InlineNode::Image(image) => image.src = image.src.replace('|', "%7C"),
                        InlineNode::AutoLink(link) => link.href = link.href.replace('|', "%7C"),
                        _ => {}
                    }
                }
                visit_inline_children(inline, self);
            }
        }
    }
    Encode { in_table: false }.blocks(&mut document.children);
}

fn preserve_html_code_for_writing(
    document: &mut Document,
    force_all: bool,
    code_lines: &mut Vec<Option<usize>>,
) -> Vec<MarkdownImportLoss> {
    use crate::include_walk::{visit_block_children, visit_inline_children, SubtreeVisitor};
    struct Preserve {
        losses: Vec<MarkdownImportLoss>,
        paragraph: bool,
        heading: bool,
        inline_depth: usize,
        label_depth: usize,
        force_all: bool,
        code_lines: Vec<Option<usize>>,
        code_index: usize,
        kept_lines: Vec<Option<usize>>,
    }
    impl SubtreeVisitor for Preserve {
        fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
            for block in blocks {
                let saved = (
                    self.paragraph,
                    self.heading,
                    self.inline_depth,
                    self.label_depth,
                );
                self.paragraph = matches!(block, BlockNode::Paragraph(_));
                self.heading = matches!(block, BlockNode::Heading(_));
                self.inline_depth = 0;
                self.label_depth = 0;
                visit_block_children(block, self);
                (
                    self.paragraph,
                    self.heading,
                    self.inline_depth,
                    self.label_depth,
                ) = saved;
            }
        }
        fn inlines(&mut self, nodes: &mut Vec<InlineNode>) {
            let standalone = self.inline_depth == 0 && nodes.len() == 1;
            self.inline_depth += 1;
            let length = nodes.len();
            for (index, node) in nodes.iter_mut().enumerate() {
                if let InlineNode::Code(code) = node {
                    let source_line = self.code_lines.get(self.code_index).copied().flatten();
                    self.code_index += 1;
                    self.kept_lines.push(source_line);
                    if code.attrs.is_none()
                        && (self.force_all
                            || code.value.is_empty()
                            || code.value.contains(['\n', '\r']))
                    {
                        // The probe writes the span at the top level; inside a
                        // container no closer ends an empty span's run (carve#3973 X5).
                        if (self.paragraph || self.heading)
                            && self.label_depth == 0
                            && !(code.value.is_empty() && self.inline_depth > 1)
                            && !self.force_all
                        {
                            let mut probe =
                                crate::parse::parse(if self.heading { "# x\n" } else { "x\n" });
                            let children = match &mut probe.children[0] {
                                BlockNode::Paragraph(paragraph) => &mut paragraph.children,
                                BlockNode::Heading(heading) => &mut heading.children,
                                _ => unreachable!(),
                            };
                            *children = vec![InlineNode::Code(code.clone())];
                            if !standalone {
                                if index + 1 == length {
                                    children.insert(0, InlineNode::text("x "));
                                } else {
                                    children.push(InlineNode::text(" x"));
                                }
                            }
                            if let Ok(source) = render_carve(&probe) {
                                if probe.markdown_code_values()
                                    == crate::parse::parse(&source).markdown_code_values()
                                {
                                    continue;
                                }
                            }
                        }
                        let body = code_html(&code.value);
                        self.kept_lines.pop();
                        self.losses.push(MarkdownImportLoss { message: "Preserved a code payload as raw HTML; targets and profiles that escape or omit raw HTML change its code structure and content".into(), line: source_line, kind: MarkdownLossKind::RawCodeFallback });
                        *node = InlineNode::RawInline(RawInline {
                            cell_wraps: None,
                            format: "html".into(),
                            content: body,
                            injected: false,
                            pos: code.pos.clone(),
                        });
                    }
                }
                let labelled = matches!(
                    node,
                    InlineNode::Link(_)
                        | InlineNode::Span(_)
                        | InlineNode::Extension(_)
                        | InlineNode::Footnote(_)
                );
                self.label_depth += usize::from(labelled);
                visit_inline_children(node, self);
                self.label_depth -= usize::from(labelled);
            }
            self.inline_depth -= 1;
        }
    }
    let mut preserve = Preserve {
        losses: Vec::new(),
        paragraph: false,
        heading: false,
        inline_depth: 0,
        label_depth: 0,
        force_all,
        code_lines: std::mem::take(code_lines),
        code_index: 0,
        kept_lines: Vec::new(),
    };
    preserve.blocks(&mut document.children);
    for blocks in document.footnote_defs.values_mut() {
        preserve.blocks(blocks);
    }
    *code_lines = preserve.kept_lines;
    preserve.losses
}

fn preserve_code_text(nodes: &mut Vec<InlineNode>) {
    preserve_code_text_at_depth(nodes, 1);
}

fn preserve_code_text_at_depth(nodes: &mut Vec<InlineNode>, code_depth: usize) {
    use crate::include_walk::{visit_inline_children, SubtreeVisitor};
    struct Preserve {
        code_depth: usize,
    }
    fn freeze_tail(nodes: &mut [InlineNode]) {
        let Some(node @ InlineNode::Text(_)) = nodes.last_mut() else {
            return;
        };
        let InlineNode::Text(text) = node else {
            unreachable!()
        };
        let html = text
            .value
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('\n', "<!---->&#10;<!---->");
        *node = InlineNode::RawInline(RawInline {
            cell_wraps: None,
            format: "html".into(),
            content: html,
            injected: false,
            pos: text.pos.clone(),
        });
    }
    impl SubtreeVisitor for Preserve {
        fn blocks(&mut self, _: &mut Vec<BlockNode>) {}
        fn inlines(&mut self, nodes: &mut Vec<InlineNode>) {
            let mut coalesced = Vec::with_capacity(nodes.len());
            for mut node in std::mem::take(nodes) {
                if matches!(node, InlineNode::SoftBreak(_)) && self.code_depth > 0 {
                    node = InlineNode::text("\n");
                }
                if let InlineNode::Text(mut text) = node {
                    if self.code_depth == 0 {
                        text.value = text.value.replace("\r\n", " ").replace(['\r', '\n'], " ");
                    }
                    if let Some(InlineNode::Text(previous)) = coalesced.last_mut() {
                        previous.value.push_str(&text.value);
                    } else {
                        coalesced.push(InlineNode::Text(text));
                    }
                    continue;
                }
                if self.code_depth > 0 {
                    freeze_tail(&mut coalesced);
                }
                if let InlineNode::RawInline(raw) = &node {
                    if raw.format == "html" {
                        if let Some(tag) = html_tag(&raw.content) {
                            if tag.name.eq_ignore_ascii_case("code") {
                                if tag.closing {
                                    self.code_depth = self.code_depth.saturating_sub(1);
                                } else if !tag.self_closing {
                                    self.code_depth += 1;
                                }
                            }
                        }
                    }
                }
                visit_inline_children(&mut node, self);
                coalesced.push(node);
            }
            if self.code_depth > 0 {
                freeze_tail(&mut coalesced);
            }
            *nodes = coalesced;
        }
    }
    Preserve { code_depth }.inlines(nodes);
}

fn code_html(value: &str) -> String {
    let mut html = String::from("<code>");
    for ch in value.replace("\r\n", "\n").replace('\r', "\n").chars() {
        if ch == '\n' {
            html.push_str("<!---->&#10;<!---->");
        } else if ch.is_ascii_punctuation() || ch == ' ' || ch == '\t' {
            html.push_str(&format!("&#{};", ch as u32));
            if ch == '@' {
                html.push_str("<!---->");
            }
        } else {
            html.push(ch);
        }
    }
    html.push_str("</code>");
    html
}

/// Drop whitespace a decoded character reference left at the head of an inline
/// run, and report every occurrence.
///
/// ONLY ON THE WRITING PATH, as the same-kind span unwrap is: the AST keeps
/// what the source carried, and only a Carve SPELLING has to give it up. The
/// writer already dropped it from a paragraph; it carried it into a quote and a
/// table cell, where the reader drops it instead, so the character reached the
/// document either way as nothing while the written source did not read back as
/// what was written. Stripping it here is one rule for all of them, and it is
/// what makes the row the report carries true everywhere.
///
/// `&nbsp;` is the control: U+00A0 is neither a space nor a tab, it IS
/// spellable at a line's head, it survives, and no row is owed for it.
fn drop_leading_decoded_whitespace(document: &mut Document) -> Vec<String> {
    let mut messages = Vec::new();
    let mut strip = |nodes: &mut Vec<InlineNode>, _: bool| {
        let Some(InlineNode::Text(text)) = nodes.first_mut() else {
            return;
        };
        let kept = text.value.trim_start_matches([' ', '\t']);
        if kept.len() == text.value.len() {
            return;
        }
        text.value = kept.to_owned();
        messages.push(LEADING_WHITESPACE_UNSPELLABLE.to_owned());
        nodes.retain(|node| !matches!(node, InlineNode::Text(t) if t.value.is_empty()));
    };
    for blocks in document.footnote_defs.values_mut() {
        crate::html_import::for_each_inline_run(blocks, &mut strip);
    }
    crate::html_import::for_each_inline_run(&mut document.children, &mut strip);

    messages
}

/// Drop the whitespace a raw span carries at the end of a content line.
///
/// CARVE-P2-025 drops it on read, so writing it leaves an import that is not a
/// `fmt` fixed point; the loss is already reported when the span is read.
fn trim_raw_span_line_ends(document: &mut Document) {
    use crate::include_walk::{visit_block_children, visit_inline_children, SubtreeVisitor};
    struct Trim;
    impl SubtreeVisitor for Trim {
        fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
            for block in blocks {
                visit_block_children(block, self);
            }
        }
        fn inlines(&mut self, inlines: &mut Vec<InlineNode>) {
            for inline in inlines {
                if let InlineNode::RawInline(raw) = inline {
                    if raw.content.contains('\n') {
                        raw.content = trim_line_ends_before_breaks(&raw.content);
                    }
                }
                visit_inline_children(inline, self);
            }
        }
    }
    for blocks in document.footnote_defs.values_mut() {
        Trim.blocks(blocks);
    }
    Trim.blocks(&mut document.children);
}

fn trim_line_ends_before_breaks(content: &str) -> String {
    let lines: Vec<&str> = content.split('\n').collect();
    let last = lines.len() - 1;
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            if index == last {
                *line
            } else {
                line.trim_end_matches(RAW_SPAN_DROPPED_WHITESPACE)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Convert Markdown source to a Carve [`Document`].
///
/// # Panics
///
/// Panics if Markdown nesting exceeds the renderer's depth ceiling. Use
/// [`try_markdown_to_ast`] to handle that refusal.
pub fn markdown_to_ast(markdown: &str) -> Document {
    try_markdown_to_ast(markdown).expect("the Markdown tree exceeds the render depth ceiling")
}

/// Build a Markdown document, returning a typed refusal when nesting is too deep.
pub fn try_markdown_to_ast(markdown: &str) -> Result<Document, crate::RenderCarveError> {
    markdown_to_ast_with_losses(markdown).map(|(document, _, _)| document)
}

/// A leading `---` block as the source spells it.
struct LeadingBlock {
    /// The canonical format token; a bare opener reads as `yaml` (PART 1).
    format: String,
    content: String,
    /// Everything after the closer.
    body: String,
    is_frontmatter: bool,
    /// Whether the opener carried a format token, lenient space and all.
    typed: bool,
}

/// Read a leading frontmatter block, by PART 1's `frontmatter_open`
/// production. `None` when the source opens no closed block at all.
fn leading_frontmatter_block(markdown: &str) -> Option<LeadingBlock> {
    let source = markdown.replace("\r\n", "\n").replace('\r', "\n");
    let rest = source.strip_prefix("---")?;
    let (opener_tail, rest) = rest.split_once('\n')?;
    let token = crate::parse::frontmatter_format_token(opener_tail)?;
    let typed = !token.is_empty();
    let format = if typed {
        token.to_owned()
    } else {
        "yaml".to_owned()
    };
    // Markdown closes the block on `---` or `...`, either carrying a trailing
    // run of spaces and tabs (PART 1 - PART 2 drops that run). A form feed is
    // content and closes nothing.
    let mut offset = 0;
    let (content_end, body_start) = rest.split_inclusive('\n').find_map(|line| {
        if matches!(line.trim_end_matches(['\n', ' ', '\t']), "---" | "...") {
            return Some((offset, offset + line.len()));
        }
        offset += line.len();
        None
    })?;
    let content = rest[..content_end].trim_end_matches('\n').to_owned();
    Some(LeadingBlock {
        // A TYPED opener is front matter whatever its content: `---FORMAT`
        // says what the block is, so there is nothing left to infer. The shape
        // test governs the BARE `---` alone (markup-carve/carve#2799).
        is_frontmatter: typed || crate::parse::has_mapping_shape(&content),
        format,
        content,
        body: rest[body_start..].to_owned(),
        typed,
    })
}

fn markdown_to_ast_with_losses(
    markdown: &str,
) -> Result<(Document, Vec<MarkdownImportLoss>, Vec<FrontmatterNotice>), crate::RenderCarveError> {
    markdown_to_ast_with_code_lines(markdown, None, None)
}

fn unresolved_footnote_at(source: &str, at: usize) -> Option<(String, usize)> {
    if !source.get(at..)?.starts_with("[^")
        || source[..at]
            .bytes()
            .rev()
            .take_while(|byte| *byte == b'\\')
            .count()
            % 2
            != 0
    {
        return None;
    }
    let label_start = at + 2;
    let width = source[label_start..].find([']', '\r', '\n'])?;
    if width == 0
        || source.as_bytes()[label_start + width] != b']'
        || source[label_start..label_start + width].contains(['\\', '|', '['])
    {
        return None;
    }
    Some((
        source[label_start..label_start + width].to_owned(),
        label_start + width + 1,
    ))
}

fn fallback_footnote_text(
    event: &Event<'_>,
    source: &str,
    at: usize,
    consumed_until: usize,
    frames: &[Frame],
) -> Option<(Option<Footnote>, usize)> {
    if !matches!(event, Event::Text(_))
        || matches!(
            frames.last(),
            Some(
                Frame::CodeBlock { .. }
                    | Frame::HtmlCode { .. }
                    | Frame::RawHtml(_)
                    | Frame::Metadata(_)
            )
        )
    {
        return None;
    }
    if at < consumed_until {
        return Some((None, consumed_until));
    }
    let (label, end) = unresolved_footnote_at(source, at)?;
    Some((
        Some(Footnote {
            attrs: None,
            id: Some(label),
            inline: None,
            number: None,
            ref_id: None,
            pos: None,
        }),
        end,
    ))
}

fn plain_reference_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

fn plain_reference_spelling(raw: &str) -> bool {
    let Some((label, tail)) = raw.strip_prefix('[').and_then(|rest| rest.split_once("][")) else {
        return false;
    };
    let Some(reference) = tail.strip_suffix(']') else {
        return false;
    };
    !label.is_empty()
        && label.bytes().all(plain_reference_byte)
        && (reference.is_empty() || reference.bytes().all(plain_reference_byte))
}

fn carve_reference_spelling(raw: &str) -> bool {
    if raw.contains(['<', '>', '|', '\\', '&']) {
        return false;
    }
    if plain_reference_spelling(raw) {
        return true;
    }
    let document = crate::parse(raw);
    matches!(document.children.as_slice(), [BlockNode::Paragraph(paragraph)]
        if matches!(paragraph.children.as_slice(), [InlineNode::Link(link)]
            if link.href.is_empty() && link.raw_ref.as_deref() == Some(raw)))
}

fn carve_reference_span(source: &str, span: std::ops::Range<usize>) -> bool {
    source
        .get(span.clone())
        .is_some_and(carve_reference_spelling)
        || source
            .get(span.start..span.end.saturating_add(2))
            .is_some_and(|raw| raw.ends_with("[]") && carve_reference_spelling(raw))
}

fn following_defined_reference(
    source: &str,
    end: usize,
    definitions: &pulldown_cmark::Parser<'_>,
) -> bool {
    let mut tail = source.get(end..).unwrap_or_default();
    for _ in 0..2 {
        let Some(rest) = tail.strip_prefix('[') else {
            return false;
        };
        let Some(close) = rest.find(']') else {
            return false;
        };
        let label = &rest[..close];
        if label.contains(['[', '\n', '\r']) {
            return false;
        }
        if definitions.reference_definitions().get(label).is_some() {
            return true;
        }
        tail = &rest[close + 1..];
    }
    false
}

fn markdown_to_ast_with_code_lines(
    markdown: &str,
    code_lines: Option<&mut Vec<Option<usize>>>,
    carrier_token: Option<&str>,
) -> Result<(Document, Vec<MarkdownImportLoss>, Vec<FrontmatterNotice>), crate::RenderCarveError> {
    let without_nuls = if markdown.contains('\0') {
        Cow::Owned(markdown.replace('\0', "\u{fffd}"))
    } else {
        Cow::Borrowed(markdown)
    };
    let leading = leading_frontmatter_block(&without_nuls);
    // A TYPED opener is invisible to pulldown-cmark, which knows only the bare
    // `---` form: it read `---toml` as paragraph text and the closer below as
    // a setext underline, so the table landed inside an `h2` (carve-rs#2388).
    // The block is claimed here instead; the bare form stays on pulldown's own
    // metadata-block path, which is the one every other test covers.
    let claimed = leading.as_ref().filter(|block| block.typed);
    let claimed_lines = claimed.map_or(0, |block| {
        without_nuls
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .chars()
            .filter(|ch| *ch == '\n')
            .count()
            - block.body.chars().filter(|ch| *ch == '\n').count()
    });
    let without_nuls = match claimed {
        Some(block) => Cow::Owned(block.body.clone()),
        None => without_nuls,
    };
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);
    // THE GATE (markup-carve/carve#2799). Metadata blocks stay OFF when the
    // leading `---` block is not mapping-shaped, so pulldown-cmark reads it as
    // CommonMark does - a thematic break and a setext heading - instead of
    // handing the walk a `Tag::MetadataBlock` to claim. Deciding it here is
    // what makes the rejected case come out with CommonMark's meaning; a
    // post-pass over emitted text could only patch the spelling.
    //
    // They stay off for a claimed typed block too, so a `---` opening the
    // BODY cannot be read as a second frontmatter block.
    let gated = claimed.is_some() || leading.as_ref().is_some_and(|block| !block.is_frontmatter);
    if !gated {
        options.insert(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);
    }

    let headings = normalize_heading_closers(&without_nuls, options);
    let source = normalize_table_email_autolinks(&headings, options);
    let mut builder = Builder {
        carrier_token: carrier_token
            .filter(|token| !token.is_empty())
            .map(str::to_owned),
        track_code_lines: code_lines.is_some(),
        ..Builder::default()
    };
    if source.contains("[^") && source.contains('|') {
        for (offset, _) in source.match_indices("carve-import-footnote-") {
            let digits: String = source[offset + "carve-import-footnote-".len()..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if let Ok(number) = digits.parse::<usize>() {
                builder.footnote_reserved.insert(number);
            }
        }
    }
    let mut line_starts = vec![0];
    for (index, byte) in source.bytes().enumerate() {
        if byte == b'\n' || (byte == b'\r' && source.as_bytes().get(index + 1) != Some(&b'\n')) {
            line_starts.push(index + 1);
        }
    }
    let mut table_autolink = false;
    let mut text_end = None;
    let prepared_urls = crate::markdown_gfm_autolinks::prepare_urls(&source, options);
    if prepared_urls.as_ref().is_some_and(|prepared| !prepared.2) {
        builder.losses.push(MarkdownImportLoss {
            message:
                "Could not place GFM bare autolinks safely; retained the original Markdown parse"
                    .into(),
            line: None,
            kind: MarkdownLossKind::Unspellable,
        });
    }
    let parser_source = prepared_urls
        .as_ref()
        .map_or(source.as_ref(), |prepared| prepared.0.as_str());
    let bare_urls = prepared_urls
        .as_ref()
        .map_or(&[][..], |prepared| prepared.1.as_slice());
    let mut bare_url_at = 0;
    let (native_source, native_urls, link_footnotes) =
        native_destinations::prepare_with_footnotes(parser_source, options);
    let footnote_marker = link_footnotes
        .first()
        .map(|note| native_source.as_bytes()[note.start]);
    let strikes = crate::markdown_strike::prepare(&native_source, options);
    let reference_definitions = Parser::new_ext(&strikes.text, options);
    let mut reference = |link: pulldown_cmark::BrokenLink<'_>| {
        let image = strikes.text.as_bytes().get(link.span.start) == Some(&b'!')
            || strikes.text.as_bytes().get(link.span.start.wrapping_sub(1)) == Some(&b'!');
        let unknown = !image
            && matches!(
                link.link_type,
                LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut
            )
            && carve_reference_span(&source, strikes.original_range(link.span.clone()))
            && !following_defined_reference(&strikes.text, link.span.end, &reference_definitions);
        strikes
            .reference(link)
            .or_else(|| unknown.then(|| ("".into(), "".into())))
    };
    let mut parser =
        Parser::new_with_broken_link_callback(&strikes.text, options, Some(&mut reference))
            .into_offset_iter();
    fn push_text_after_entity_lt(
        builder: &mut Builder,
        pending: &mut u8,
        mut value: &str,
        native_code: bool,
    ) {
        if native_code {
            *pending = 0;
            builder.text(value);
            return;
        }
        loop {
            if value.is_empty() {
                break;
            }
            if *pending == 1 && value.starts_with('/') {
                builder.text("/");
                value = &value[1..];
                *pending = 2;
            } else if *pending == 2 && value.starts_with('#') {
                builder.inline(InlineNode::escaped_text("#"));
                value = &value[1..];
                *pending = 0;
            } else {
                *pending = 0;
                break;
            }
        }
        if !value.is_empty() {
            builder.text(value);
        }
    }
    fn push_link_footnotes(
        builder: &mut Builder,
        pending: &mut u8,
        text: &str,
        range: &std::ops::Range<usize>,
        source: &str,
        masks: &[native_destinations::FootnoteMask],
        marker: Option<u8>,
    ) -> bool {
        let Some(marker) = marker else {
            return false;
        };
        if !text.as_bytes().contains(&marker) {
            return false;
        }
        let first = masks.partition_point(|mask| mask.end <= range.start);
        // Search parsed text because entity decoding can shorten text before a marker.
        let relevant: Vec<_> = masks[first..]
            .iter()
            .take_while(|mask| mask.start < range.end)
            .collect();
        if relevant.is_empty() {
            return false;
        }
        let mut segments = Vec::with_capacity(relevant.len());
        let mut cursor = 0;
        for mask in &relevant {
            let Some(offset) = text.as_bytes()[cursor..]
                .iter()
                .position(|byte| *byte == marker)
            else {
                return false;
            };
            let start = cursor + offset;
            let end = start + mask.end - mask.start;
            if text
                .as_bytes()
                .get(start..end)
                .map_or(true, |run| run.iter().any(|byte| *byte != marker))
            {
                return false;
            }
            segments.push((start, end, *mask));
            cursor = end;
        }
        cursor = 0;
        for (start, end, mask) in segments {
            push_text_after_entity_lt(builder, pending, &text[cursor..start], false);
            *pending = 0;
            builder.push(
                Event::FootnoteReference(mask.label.clone().into()),
                &source[mask.start..mask.end],
                false,
            );
            cursor = end;
        }
        push_text_after_entity_lt(builder, pending, &text[cursor..], false);
        true
    }
    let mut used_reference_ids = BTreeSet::new();
    let mut entity_lt_prefix = 0;
    let mut unresolved_footnote_end = 0;
    while let Some((mut event, range)) = parser.next() {
        if let Event::Start(Tag::Link { id, link_type, .. }) = &event {
            if !id.is_empty()
                && matches!(
                    link_type,
                    LinkType::Reference
                        | LinkType::ReferenceUnknown
                        | LinkType::Collapsed
                        | LinkType::CollapsedUnknown
                        | LinkType::Shortcut
                        | LinkType::ShortcutUnknown
                )
            {
                used_reference_ids.insert(id.to_string());
            }
        }
        if strikes.synthetic_comment(&event, &range) {
            continue;
        }
        event = strikes.restore(event, &range);
        let mut range = strikes.original_range(range);
        if let Some(url) = native_urls.get(&range.start) {
            if let Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) =
                &mut event
            {
                *dest_url = url.clone().into();
            }
        }
        if let Some(literal) = crate::markdown_strike::literal_underscore(&source, &event, &range) {
            event = Event::Text(source[literal.clone()].into());
            range = literal;
        }
        builder.current_line =
            claimed_lines + line_starts.partition_point(|start| *start <= range.start);
        if let Some((footnote, end)) = fallback_footnote_text(
            &event,
            &source,
            range.start,
            unresolved_footnote_end,
            &builder.frames,
        ) {
            unresolved_footnote_end = end;
            if let Some(footnote) = footnote {
                builder.inline(InlineNode::Footnote(footnote));
            }
            continue;
        }
        if let Some((kept, rest)) = text_end.take().and_then(|end| {
            let (kept, rest) = stripped_line_end(&source, end)?;
            // An event starting inside the run already carries those characters.
            let inside = range.start >= end
                && range.start < end + kept.len() + rest
                && !matches!(event, Event::HardBreak);
            (!inside).then_some((kept, rest))
        }) {
            builder.push(Event::Text(kept.into()), kept, false);
            if matches!(event, Event::HardBreak) && rest < 2 {
                event = Event::SoftBreak;
            }
        }
        if let Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) = &event {
            let word = info
                .trim_matches([' ', '\t'])
                .split([' ', '\t'])
                .next()
                .unwrap_or("");
            if !word.is_empty() && fence_language(info).is_none() {
                builder.losses.push(MarkdownImportLoss {
                    message: format!("Dropped code-block language {word:?}; Carve cannot spell this language token."),
                    line: Some(builder.current_line),
                    kind: MarkdownLossKind::Unspellable,
                });
            }
        }
        let empty_title = match &event {
            Event::Start(
                Tag::Link {
                    link_type,
                    title,
                    id,
                    ..
                }
                | Tag::Image {
                    link_type,
                    title,
                    id,
                    ..
                },
            ) if title.is_empty() => {
                if *link_type == LinkType::Inline {
                    has_empty_inline_title(
                        &source[range.clone()],
                        builder
                            .frames
                            .iter()
                            .filter(|frame| matches!(frame, Frame::BlockQuote(_)))
                            .count(),
                    )
                } else {
                    strikes.reference_has_title(&range).unwrap_or_else(|| {
                        parser
                            .reference_definitions()
                            .get(id)
                            .is_some_and(|definition| definition.title.is_some())
                    })
                }
            }
            _ => false,
        };
        let in_table = builder
            .frames
            .iter()
            .any(|frame| matches!(frame, Frame::TableCell(_)));
        if in_table {
            match &mut event {
                Event::Start(Tag::Link {
                    link_type: LinkType::Autolink | LinkType::Email,
                    dest_url,
                    ..
                }) => {
                    *dest_url = dest_url.replace("\\|", "|").into();
                    table_autolink = true;
                }
                Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) => {
                    if !source[range.clone()].contains(r"\\\|") {
                        *dest_url = dest_url.replace("\\|", "|").into();
                    }
                }
                Event::Text(text) if table_autolink => *text = text.replace("\\|", "|").into(),
                Event::InlineHtml(html) => *html = html.replace("\\|", "|").into(),
                _ => {}
            }
        }
        if matches!(event, Event::End(TagEnd::Link)) {
            table_autolink = false;
        }
        let task_marker = matches!(event, Event::TaskListMarker(_));
        let ordered = task_marker && builder.in_ordered_item();
        let extension_reaches = task_marker && tasklist_extension_reaches(&source, &range);
        if task_marker && (ordered || !extension_reaches) {
            // NOWHERE TO PUT A BOX, or none to read. Carve spells a checkbox
            // only behind a bullet, so handing the writer one on an ordered
            // item dropped it - and the marker's own characters went with it,
            // which left `1. [x] done` as `1. done` (carve-rs#1886). Where the
            // tasklist extension does not reach at all there is no box to read
            // in the first place (carve-rs#1899). Either way the characters the
            // author wrote stay as the text they were written as.
            if ordered && extension_reaches {
                builder.losses.push(MarkdownImportLoss {
                    message: crate::html_import::ORDERED_TASK_ITEM_UNSPELLABLE.to_owned(),
                    line: Some(builder.current_line),
                    kind: MarkdownLossKind::Unspellable,
                });
            }
            builder.inline(InlineNode::text(&source[range.start..range.end]));
            if let Some(separator) = task_marker_separator(&source, range.end) {
                builder.inline(separator);
            } else if source[range.end..].starts_with(['\n', '\r']) {
                // pulldown swallows the line end after a marker it read, so a
                // label on the NEXT line came back joined to the brackets:
                // `- [ ]` / `  foo` read `[ ]foo` where cmark-gfm reads
                // `[ ]` and a soft break.
                builder.inline(InlineNode::soft_break());
            }
            continue;
        }
        if matches!(&event, Event::Html(_))
            && !builder
                .frames
                .iter()
                .any(|frame| matches!(frame, Frame::ListItem { .. } | Frame::BlockQuote(_)))
        {
            let line_start = source[..range.start]
                .rfind('\n')
                .map_or(0, |newline| newline + 1);
            let indent = &source[line_start..range.start];
            if !indent.is_empty() && indent.chars().all(|ch| matches!(ch, ' ' | '\t')) {
                builder.raw_html(indent);
            }
        }
        let line_text = matches!(
            event,
            Event::Text(_)
                | Event::Code(_)
                | Event::InlineHtml(_)
                | Event::FootnoteReference(_)
                | Event::End(
                    TagEnd::Emphasis
                        | TagEnd::Strong
                        | TagEnd::Strikethrough
                        | TagEnd::Link
                        | TagEnd::Image
                )
        ) && !builder.frames.iter().any(|frame| {
            matches!(
                frame,
                Frame::TableCell(_)
                    | Frame::CodeBlock { .. }
                    | Frame::Metadata(_)
                    | Frame::RawHtml(_)
            )
        });
        if line_text {
            text_end = Some(range.end);
        }
        if matches!(&event, Event::Text(_)) {
            let native_code = builder
                .frames
                .iter()
                .any(|frame| matches!(frame, Frame::HtmlCode { .. }));
            if builder.frames.iter().any(|frame| {
                matches!(
                    frame,
                    Frame::CodeBlock { .. }
                        | Frame::Metadata(_)
                        | Frame::Image { .. }
                        | Frame::RawHtml(_)
                )
            }) {
                entity_lt_prefix = 0;
                builder.push(event, &source[range], empty_title);
                continue;
            }
            if let Event::Text(text) = &event {
                if push_link_footnotes(
                    &mut builder,
                    &mut entity_lt_prefix,
                    text,
                    &range,
                    &source,
                    &link_footnotes,
                    footnote_marker,
                ) {
                    continue;
                }
            }
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
                        push_text_after_entity_lt(
                            &mut builder,
                            &mut entity_lt_prefix,
                            &source[cursor..link.start],
                            native_code,
                        );
                    }
                    entity_lt_prefix = 0;
                    let label = if link.table {
                        std::borrow::Cow::Owned(source[link.start..link.end].replace("\\|", "|"))
                    } else {
                        std::borrow::Cow::Borrowed(&source[link.start..link.end])
                    };
                    builder.inline(literal_bare_link(&label, link.prefix));
                    cursor = link.end;
                    bare_url_at += 1;
                }
                if cursor < range.end {
                    push_text_after_entity_lt(
                        &mut builder,
                        &mut entity_lt_prefix,
                        &source[cursor..range.end],
                        native_code,
                    );
                }
            } else {
                if let Event::Text(text) = &event {
                    if native_code {
                        entity_lt_prefix = 0;
                        builder.push(event, &source[range], empty_title);
                        continue;
                    }
                    let raw = &source[range.clone()];
                    if text.as_ref() == "<" && raw.starts_with('&') {
                        builder.inline(InlineNode::escaped_text("<"));
                        entity_lt_prefix = 1;
                        continue;
                    }
                    if range.start > 0
                        && source.as_bytes()[range.start - 1] == b'\\'
                        && raw.starts_with("</#")
                        && text.starts_with("</#")
                    {
                        entity_lt_prefix = 0;
                        builder.inline(InlineNode::escaped_text("<"));
                        builder.text(&text[1..]);
                        continue;
                    }
                    push_text_after_entity_lt(&mut builder, &mut entity_lt_prefix, text, false);
                    continue;
                }
            }
        } else {
            entity_lt_prefix = 0;
            builder.push(event, &source[range], empty_title);
        }
        if builder.over_depth {
            break;
        }
    }

    if builder.over_depth {
        return Err(crate::RenderDepthError::new("carve", crate::MAX_RENDER_DEPTH).into());
    }
    let mut used_definitions = BTreeMap::new();
    let mut definition_losses = Vec::new();
    for id in &used_reference_ids {
        let Some(def) = parser.reference_definitions().get(id.as_str()) else {
            continue;
        };
        let label = source
            .get(def.span.clone())
            .and_then(|line| line.strip_prefix('['))
            .and_then(|line| line.split_once("]:"))
            .map_or(id.as_str(), |(label, _)| label);
        if !crate::parse::reference_label_has_definition_spelling(label) {
            continue;
        }
        if def.dest.is_empty() && def.title.is_some() {
            definition_losses.push(MarkdownImportLoss {
                message: "An empty reference definition with a title was inlined".to_owned(),
                line: Some(
                    source[..def.span.start]
                        .bytes()
                        .filter(|byte| *byte == b'\n')
                        .count()
                        + 1,
                ),
                kind: MarkdownLossKind::Unspellable,
            });
            continue;
        }
        used_definitions
            .entry(def.span.start)
            .or_insert_with(|| LinkReferenceDefinition {
                label: label.to_owned(),
                href: markdown_destination(&def.dest, false),
                title: def.title.as_ref().map(ToString::to_string),
                attrs: None,
                pos: None,
            });
    }
    let needs_email_walk = builder.needs_email_walk;
    let (mut document, mut losses, locations) = builder.finish();
    losses.extend(definition_losses);
    document.children.extend(
        used_definitions
            .into_values()
            .map(BlockNode::LinkReferenceDefinition),
    );
    crate::ast_json::hydrate_resolved_empty_references(&mut document);
    if needs_email_walk {
        crate::markdown_gfm_autolinks::link_email_text(&mut document);
    }
    encode_table_destination_pipes(&mut document);
    normalize_imported_reference_links(&mut document);
    if let Some(code_lines) = code_lines {
        *code_lines = locations;
    }
    if let Some(block) = claimed {
        document.frontmatter_raw = Some(Frontmatter {
            format: block.format.clone(),
            content: block.content.clone(),
            pos: None,
        });
        document.frontmatter = crate::parse::frontmatter_map(&block.format, &block.content);
    }
    // EVERY conversion is reported (markup-carve/carve#2799), whichever path
    // claimed the block, and the message names the reason it was claimed.
    let typed = claimed.is_some();
    let notices = document
        .frontmatter_raw
        .iter()
        .map(|frontmatter| FrontmatterNotice {
            message: if typed {
                format!(
                    "Converted the leading `---{}` block into frontmatter; a typed opener names the block's format",
                    frontmatter.format
                )
            } else {
                format!(
                    "Converted the leading `---` block into `{}` frontmatter; its first content line has the shape of a mapping",
                    frontmatter.format
                )
            },
            typed_opener: typed,
        })
        .collect();
    crate::render_depth::refuse_if_too_deep(&document, "carve")?;
    Ok((document, losses, notices))
}

/// The vertical tabs and form feeds pulldown-cmark stripped from the end of a
/// line along with its spaces and tabs, and how many spaces and tabs follow
/// them. CommonMark strips only spaces and tabs (4.8 paragraphs, 6.7 soft line
/// breaks), and only two spaces make a hard break (6.6), so both characters
/// are content.
fn stripped_line_end(source: &str, text_end: usize) -> Option<(&str, usize)> {
    let tail = &source[text_end..];
    let run = tail
        .bytes()
        .take_while(|byte| matches!(byte, b' ' | b'\t' | 0x0b | 0x0c))
        .count();
    let kept = tail[..run].rfind(['\u{b}', '\u{c}'])? + 1;
    matches!(tail.as_bytes().get(run), None | Some(b'\n' | b'\r'))
        .then(|| (&tail[..kept], run - kept))
}

/// The nesting the importer will BUILD, in AST levels (PART 9 §25).
///
/// Two levels above the renderers' ceiling. The extra room accounts for frame
/// nesting that does not add an AST level. A refusal stops the event loop and
/// returns an error before a partial document can escape.
///
/// The cap is here because this importer is the only one with nothing else to
/// bound it. The Carve parser caps its own nesting, the HTML importer answers
/// `HtmlImportError::DepthLimit`, ingest has its JSON depth budget - Markdown
/// nesting is limited only by the input, and this builder folds frames instead of
/// recursing, so it cheerfully built a tree that could not afterwards be walked,
/// cloned or even FREED without overflowing the stack and aborting the process
/// (carve-rs#1877). Derived `Clone` and `Drop` recurse over the tree and have no
/// ceiling to consult, so the only place to stop it is before it exists.
const MAX_IMPORT_LEVELS: usize = crate::render::MAX_RENDER_DEPTH + 2;

fn restore_table_email_brackets(value: String, markdown: &str) -> String {
    if !markdown.contains("\\|") || !value.contains("mailto:") {
        return value;
    }
    static AFTER_CLOSER: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static IN_REFERENCE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let after_closer = AFTER_CLOSER.get_or_init(|| {
        regex::Regex::new(r"\]\[(\[[^\]\n]*\]\(mailto:[^)\n]*%7C[^)\n]*\))\[\]")
            .expect("table email bracket regex")
    });
    let value = after_closer
        .replace_all(&value, |caps: &regex::Captures<'_>| {
            format!(r"]\[{}[\]", &caps[1])
        })
        .into_owned();
    let in_reference = IN_REFERENCE.get_or_init(|| {
        regex::Regex::new(r"\\\[([^\]\n]+)\]\[(\[[^\]\n]*\]\(mailto:[^)\n]*%7C[^)\n]*\))\]")
            .expect("table email reference regex")
    });
    in_reference
        .replace_all(&value, |caps: &regex::Captures<'_>| {
            format!(r"[{}]\[{}\]", &caps[1], &caps[2])
        })
        .into_owned()
}

// pulldown tests email syntax before removing a table's pipe escape.
fn normalize_table_email_autolinks<'a>(source: &'a str, options: Options) -> Cow<'a, str> {
    if !source.contains("\\|") || !source.contains('@') || !source.contains('<') {
        return Cow::Borrowed(source);
    }
    let mut cells = Vec::new();
    let mut opaque: Vec<(std::ops::Range<usize>, bool)> = Vec::new();
    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        if matches!(event, Event::Start(Tag::TableCell)) {
            cells.push(range.clone());
        }
        if matches!(
            event,
            Event::Code(_)
                | Event::Html(_)
                | Event::InlineHtml(_)
                | Event::Start(Tag::Link { .. } | Tag::Image { .. })
        ) {
            let code = matches!(event, Event::Code(_));
            if let Some(previous) = opaque
                .last_mut()
                .filter(|previous| previous.0.end >= range.start)
            {
                previous.0.end = previous.0.end.max(range.end);
                previous.1 &= code;
            } else {
                opaque.push((range, code));
            }
        }
    }
    static EMAIL: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let email = EMAIL.get_or_init(|| regex::Regex::new(r"<((?:[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]|\\\|)+)@([a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*)>").expect("email autolink regex"));
    let mut cell = 0;
    let mut protected = 0;
    let mut output = String::new();
    let mut copied = 0;
    for caps in email.captures_iter(source) {
        let value = caps.get(0).unwrap();
        if !caps[1].contains("\\|") {
            continue;
        }
        while cells
            .get(cell)
            .is_some_and(|range| range.end <= value.start())
        {
            cell += 1;
        }
        while opaque
            .get(protected)
            .is_some_and(|(range, _)| range.end <= value.start())
        {
            protected += 1;
        }
        if !cells
            .get(cell)
            .is_some_and(|range| range.start <= value.start() && value.end() <= range.end)
        {
            continue;
        }
        let mut blocked = false;
        while let Some((range, code)) = opaque
            .get(protected)
            .filter(|(range, _)| range.start < value.end())
        {
            if !(*code && value.start() <= range.start && range.end <= value.end()) {
                blocked = true;
            }
            if range.end > value.end() {
                break;
            }
            protected += 1;
        }
        if blocked {
            continue;
        }
        let mut before = value.start();
        while before > 0 && source.as_bytes()[before - 1] == b'\\' {
            before -= 1;
        }
        if (value.start() - before) % 2 != 0 {
            continue;
        }
        let address = format!("{}@{}", caps[1].replace("\\|", "|"), &caps[2]);
        let mut label = String::new();
        for ch in address.chars() {
            if ch.is_ascii_punctuation() {
                label.push('\\');
            }
            label.push(ch);
        }
        output.push_str(&source[copied..value.start()]);
        output.push_str(&format!(
            "[{label}]({})",
            markdown_destination(&address, true)
                .replace('|', "%7C")
                .replace('&', "%26")
        ));
        copied = value.end();
    }
    if copied == 0 {
        Cow::Borrowed(source)
    } else {
        output.push_str(&source[copied..]);
        Cow::Owned(output)
    }
}

fn normalize_heading_closers<'a>(source: &'a str, options: Options) -> Cow<'a, str> {
    if !source.contains('\t') {
        return Cow::Borrowed(source);
    }
    let mut tabs = Vec::new();
    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        if !matches!(event, Event::Start(Tag::Heading { .. })) {
            continue;
        }
        let bytes = source.as_bytes();
        let mut end = range.end;
        while end > range.start && matches!(bytes[end - 1], b' ' | b'\t' | b'\n' | b'\r') {
            end -= 1;
        }
        let mut hashes = end;
        while hashes > range.start && bytes[hashes - 1] == b'#' {
            hashes -= 1;
        }
        if hashes > range.start && hashes < end && matches!(bytes[hashes - 1], b' ' | b'\t') {
            if bytes[hashes - 1] == b'\t' {
                tabs.push(hashes - 1);
            }
            for (offset, byte) in bytes[end..range.end].iter().enumerate() {
                if *byte == b'\t' {
                    tabs.push(end + offset);
                }
            }
        }
    }
    if tabs.is_empty() {
        return Cow::Borrowed(source);
    }
    let mut normalized = source.to_owned();
    for tab in tabs.into_iter().rev() {
        normalized.replace_range(tab..tab + 1, " ");
    }
    Cow::Owned(normalized)
}

/// Whether cmark-gfm's tasklist extension reaches this bracket pair.
///
/// pulldown reads a box off any bullet item whose content opens with one; the
/// extension is narrower, and cmark-gfm 0.29.0.gfm.13 is the reader the
/// importers answer to (markup-carve/carve#2187). Three conditions, each
/// measured against it rather than recalled (markup-carve/carve-php#2366):
///
/// - ONE container marker on the line before the pair. A second marker there -
///   another item's or a quote's - puts the list out of reach, so `- - [ ] foo`
///   and `> - [ ] foo` are text while the more deeply nested `- a` /
///   `  - [ ] foo` is a box. Indentation alone never does.
/// - A literal space, `x` or `X` inside the pair. `[<TAB>]` is not a state.
/// - Whitespace after the pair ON THE SAME LINE. `- [ ]` at a line end is text,
///   and so is `- [ ]` with its label on the next line.
pub(crate) fn tasklist_extension_reaches(source: &str, marker: &std::ops::Range<usize>) -> bool {
    // pulldown's range opens at the marker's leading indentation rather than at
    // the bracket, so the bracket is found inside it.
    let Some(bracket) = source[marker.start..marker.end]
        .find('[')
        .map(|offset| marker.start + offset)
    else {
        return false;
    };
    let line_start = source[..bracket]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    let after_indent = source[line_start..bracket].trim_start_matches([' ', '\t']);
    let Some(after_marker) = strip_one_list_marker(after_indent) else {
        return false;
    };
    if after_marker.is_empty() || !after_marker.chars().all(|ch| matches!(ch, ' ' | '\t')) {
        return false;
    }
    let mut pair = source[bracket..].chars();
    pair.next();
    if !matches!(pair.next(), Some(' ' | 'x' | 'X')) {
        return false;
    }
    matches!(source[marker.end..].chars().next(), Some(' ' | '\t'))
}

/// What follows one bullet or ordered marker at the head of `line`, or `None`
/// when `line` does not open with one.
fn strip_one_list_marker(line: &str) -> Option<&str> {
    if let Some(rest) = line.strip_prefix(['-', '*', '+']) {
        return Some(rest);
    }
    let digits = line.len()
        - line
            .trim_start_matches(|ch: char| ch.is_ascii_digit())
            .len();
    (digits > 0)
        .then(|| line[digits..].strip_prefix(['.', ')']))
        .flatten()
}

/// The run of spaces or tabs pulldown swallows after a task marker.
///
/// A box renders its own separator, so the marker's own reading is the only one
/// that has to put it back: anything that keeps the marker as ordinary content
/// keeps what followed it as well.
fn task_marker_separator(source: &str, end: usize) -> Option<InlineNode> {
    let separator: String = source[end..]
        .chars()
        .take_while(|ch| matches!(ch, ' ' | '\t'))
        .collect();
    (!separator.is_empty()).then(|| InlineNode::text(separator))
}

/// A container under construction.
///
/// A start event pushes a frame and its end pops one, folding the finished node
/// into the frame beneath - so nesting comes from the parser rather than from
/// tracking indentation. Each frame owns the children it collects, which is why
/// a block frame and an inline frame are different variants rather than one
/// frame plus two side stacks that could drift out of step.
/// `<del class="critic-delete">` and nothing wider: one double-quoted class
/// attribute and no other, which is the shape PART 11 section 8c emits for a
/// deletion (markup-carve/carve#2845).
fn is_critic_delete_open(value: &str) -> bool {
    let Some(inner) = value
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
    else {
        return false;
    };
    let inner = inner.trim_end();
    let Some(attrs) = inner
        .get(..3)
        .filter(|name| name.eq_ignore_ascii_case("del"))
        .map(|_| inner[3..].trim_start())
    else {
        return false;
    };
    let Some(rest) = attrs
        .get(..5)
        .filter(|key| key.eq_ignore_ascii_case("class"))
        .map(|_| attrs[5..].trim_start())
    else {
        return false;
    };
    rest.strip_prefix('=')
        .map(str::trim_start)
        .is_some_and(|value| value == "\"critic-delete\"")
}

enum Frame {
    Paragraph(Vec<InlineNode>),
    Heading(u8, Vec<InlineNode>),
    BlockQuote(Vec<BlockNode>),
    List {
        ordered: bool,
        start: Option<usize>,
        delim: Option<char>,
        bullet_char: Option<char>,
        items: Vec<ListItem>,
        /// A list is loose when any item holds more than one block, which
        /// CommonMark decides by blank lines between items; the parser has
        /// already applied that rule, so this reads the result off the tree.
        tight: bool,
    },
    ListItem {
        checked: Option<bool>,
        children: Vec<BlockNode>,
        /// Set when a paragraph opens DIRECTLY inside this item, which is how
        /// the parser spells looseness: a tight item emits its text with no
        /// paragraph around it. It cannot be inferred from the finished item,
        /// because this builder wraps that bare text in a paragraph of its own
        /// - so by the time the item closes, both shapes look alike.
        loose: bool,
        /// A TIGHT item's inline run, held until the item's content column is
        /// closed by a block or by the item itself.
        ///
        /// Tightness IS the absence of `Start(Paragraph)`, so these nodes
        /// arrive with no inline frame open and there is nothing to collect
        /// them into. The run is one paragraph, so it is buffered whole rather
        /// than folded a node at a time - it lives in the FRAME because a
        /// nested list opens a second item while the first one's run is still
        /// unflushed, and one buffer on the builder would mix the two.
        pending: Vec<InlineNode>,
    },
    CodeBlock {
        lang: Option<String>,
        content: String,
    },
    /// A block-level HTML element, which the parser opens once and then fills a
    /// line at a time. It is a frame rather than a buffer on the builder so the
    /// finished raw block folds into whatever container the element sits in,
    /// the same way every other block does.
    RawHtml(String),
    /// A paired HTML tag with a native Carve inline representation.
    HtmlEmphasis {
        tag: String,
        kind: EmphasisKind,
        children: Vec<InlineNode>,
    },
    HtmlCode {
        nested_code_depth: usize,
        line: usize,
        tag: String,
        open: String,
        native: bool,
        children: Vec<InlineNode>,
    },
    HtmlInsert {
        tag: String,
        children: Vec<InlineNode>,
    },
    /// `<del class="critic-delete">`, which PART 11 section 8c spells a
    /// deletion with so it stops colliding with the bare `<del>` a `strike`
    /// falls back to (markup-carve/carve#2845).
    HtmlDelete {
        tag: String,
        open: String,
        children: Vec<InlineNode>,
    },
    Emphasis(EmphasisKind, Vec<InlineNode>),
    Link {
        href: String,
        title: Option<String>,
        children: Vec<InlineNode>,
        autolink: Option<(String, String)>,
        ref_label: Option<String>,
        raw_ref: Option<String>,
    },
    Image {
        src: String,
        title: Option<String>,
        alt: String,
    },
    Table {
        alignments: Vec<Alignment>,
        rows: Vec<TableRow>,
    },
    TableRow {
        header: bool,
        cells: Vec<TableCell>,
    },
    TableCell(Vec<InlineNode>),
    FootnoteDef {
        label: String,
        children: Vec<BlockNode>,
        outer_levels: (usize, usize),
    },
    Metadata(String),
}

/// The AST levels a finished frame adds beneath itself: (block, inline).
///
/// EXHAUSTIVE on purpose. A container counted as zero here is a hole in
/// [`MAX_IMPORT_LEVELS`], and the hole would only show as an abort on input deep
/// enough to reach it.
fn levels_added(frame: &Frame) -> (usize, usize) {
    match frame {
        // An item's blocks sit one level under the LIST, so the list owns that
        // level and the item frame adds none of its own.
        Frame::BlockQuote(_)
        | Frame::List { .. }
        | Frame::Table { .. }
        | Frame::FootnoteDef { .. } => (1, 0),
        Frame::Emphasis(..)
        | Frame::Link { .. }
        | Frame::HtmlEmphasis { .. }
        | Frame::HtmlCode { .. }
        | Frame::HtmlInsert { .. }
        | Frame::HtmlDelete { .. } => (0, 1),
        // A paragraph, heading or cell OPENS an inline sequence rather than
        // nesting inside one, and the rest hold text.
        Frame::Paragraph(_)
        | Frame::Heading(..)
        | Frame::ListItem { .. }
        | Frame::CodeBlock { .. }
        | Frame::RawHtml(_)
        | Frame::Image { .. }
        | Frame::TableRow { .. }
        | Frame::TableCell(_)
        | Frame::Metadata(_) => (0, 0),
    }
}

#[derive(Default)]
struct Builder {
    carrier_token: Option<String>,
    frames: Vec<Frame>,
    /// Block and inline nesting the open frames account for, held against
    /// [`MAX_IMPORT_LEVELS`]. Maintained by `push_frame` / `pop_frame` alone, so
    /// no call site can move the stack without moving these with it.
    block_levels: usize,
    inline_levels: usize,
    /// Set when a frame was refused at the cap. The event loop stops and the
    /// importer returns an error instead of finishing a partial document.
    over_depth: bool,
    /// Top-level blocks, once every frame above them has closed.
    blocks: Vec<BlockNode>,
    footnote_defs: BTreeMap<String, Vec<BlockNode>>,
    /// Footnote numbers in order of first reference, which is the order the
    /// Carve parser assigns and therefore the one a round trip must reproduce.
    footnote_numbers: BTreeMap<String, usize>,
    footnote_labels: BTreeMap<String, String>,
    footnote_reserved: BTreeSet<usize>,
    footnote_serial: usize,
    frontmatter: Option<Frontmatter>,
    losses: Vec<MarkdownImportLoss>,
    current_line: usize,
    /// HTML code tags still open in the current inline run.
    html_code_depth: usize,
    needs_email_walk: bool,
    track_code_lines: bool,
    code_lines: BTreeMap<Option<String>, Vec<Option<usize>>>,
    /// The admonition kind the open top-level quote's first line names
    /// (CARVE-P2-033).
    alert_kind: Option<&'static str>,
}

impl Builder {
    fn carrier_placeholder(&self, html: &str) -> Option<(String, bool)> {
        let content = html.trim().strip_prefix("<!-- ")?.strip_suffix(" -->")?;
        let math = content.ends_with(" math");
        let content = content.strip_suffix(" math").unwrap_or(content);
        let index = content
            .strip_prefix(self.carrier_token.as_deref()?)?
            .strip_suffix('Z')?;
        index.parse::<usize>().ok()?;
        Some((content.to_owned(), math))
    }

    fn native_code(&mut self, value: String, line: usize) {
        if self.track_code_lines
            && !self
                .frames
                .iter()
                .any(|frame| matches!(frame, Frame::Image { .. }))
        {
            let owner = self.frames.iter().rev().find_map(|frame| match frame {
                Frame::FootnoteDef { label, .. } => Some(label.clone()),
                _ => None,
            });
            self.code_lines.entry(owner).or_default().push(Some(line));
        }
        self.inline(InlineNode::code(value, None));
    }

    fn footnote_label(&mut self, label: &str) -> String {
        if !label.contains('|') {
            return label.to_owned();
        }
        if let Some(renamed) = self.footnote_labels.get(label) {
            return renamed.clone();
        }
        self.footnote_serial = self.footnote_serial.max(1);
        while self.footnote_reserved.contains(&self.footnote_serial) {
            self.footnote_serial += 1;
        }
        let renamed = format!("carve-import-footnote-{}", self.footnote_serial);
        self.footnote_serial += 1;
        self.footnote_labels
            .insert(label.to_owned(), renamed.clone());
        renamed
    }

    /// Whether the item being filled belongs to an ORDERED list.
    ///
    /// The NEAREST list frame, not the outermost one: a bullet task list nested
    /// inside an ordered item still spells its boxes.
    fn in_ordered_item(&self) -> bool {
        self.frames.iter().rev().find_map(|frame| match frame {
            Frame::List { ordered, .. } => Some(*ordered),
            _ => None,
        }) == Some(true)
    }

    fn push(&mut self, event: Event<'_>, source: &str, empty_title: bool) {
        match event {
            Event::Start(tag) => self.start(tag, source, empty_title),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.text(&text),
            Event::Code(code) => self.native_code(code.to_string(), self.current_line),
            // A soft break is a newline the author wrote inside a paragraph;
            // Carve keeps it, so the writer can re-wrap at the same place.
            Event::SoftBreak => self.inline(InlineNode::soft_break()),
            Event::HardBreak => self.inline(InlineNode::hard_break()),
            Event::Rule => self.block(BlockNode::ThematicBreak(ThematicBreak::default())),
            // An HTML BLOCK becomes a raw block, matching carve-js and
            // carve-php. Markdown's own contract is that block HTML is HTML,
            // and dropping it to text silently loses a `<div>` wrapper the
            // author wrote on purpose. Whether that raw block may render is a
            // PROFILE decision (`apply_profile` strips it), which is where a
            // policy about untrusted input belongs - not silently inside one
            // engine's importer while the other two keep it.
            Event::Html(html) => {
                // pulldown normalizes away up to three leading spaces on an
                // HTML-block line, while carve-js preserves those source
                // bytes. The offset iterator still exposes them.
                let value = if source.trim_start() == html.as_ref() {
                    source
                } else {
                    &html
                };
                self.raw_html(value)
            }
            Event::InlineHtml(html) => {
                if let Some((token, _)) = self.carrier_placeholder(&html) {
                    self.text(&token);
                } else {
                    self.inline_html(&html);
                }
            }
            Event::FootnoteReference(label) => {
                let label = label.to_string();
                let next = self.footnote_numbers.len() + 1;
                let number = *self
                    .footnote_numbers
                    .entry(label.to_string())
                    .or_insert(next);
                self.inline(InlineNode::Footnote(Footnote {
                    attrs: None,
                    id: Some(label.to_string()),
                    inline: None,
                    number: Some(number),
                    ref_id: None,
                    pos: None,
                }));
            }
            Event::TaskListMarker(checked) => {
                if let Some(Frame::ListItem { checked: slot, .. }) = self.frames.last_mut() {
                    *slot = Some(checked);
                }
            }
            _ => {}
        }
    }

    fn inline_html(&mut self, value: &str) {
        if self
            .frames
            .iter()
            .any(|frame| matches!(frame, Frame::Image { .. }))
        {
            self.text(value);
            return;
        }

        if value == "<!---->" && self.html_code_depth > 0 {
            let children = match self.frames.last_mut() {
                Some(Frame::Paragraph(children))
                | Some(Frame::Heading(_, children))
                | Some(Frame::Emphasis(_, children))
                | Some(Frame::HtmlEmphasis { children, .. })
                | Some(Frame::HtmlCode { children, .. })
                | Some(Frame::HtmlInsert { children, .. })
                | Some(Frame::HtmlDelete { children, .. })
                | Some(Frame::Link { children, .. })
                | Some(Frame::TableCell(children))
                | Some(Frame::ListItem {
                    pending: children, ..
                }) => Some(children),
                _ => None,
            };
            if let Some(InlineNode::Text(text)) = children.and_then(|nodes| nodes.last_mut()) {
                if text.value.ends_with('\r') {
                    text.value.pop();
                    text.value.push('\n');
                }
            }
            self.raw_inline(value.to_owned());
            return;
        }
        let Some(tag) = html_tag(value) else {
            self.raw_inline(value.to_string());
            return;
        };

        if tag.name.eq_ignore_ascii_case("a")
            && tag.closing
            && self
                .frames
                .iter()
                .any(|frame| matches!(frame, Frame::Link { .. }))
        {
            let children = match self.frames.last_mut() {
                Some(Frame::Link { children, .. })
                | Some(Frame::Emphasis(_, children))
                | Some(Frame::HtmlEmphasis { children, .. })
                | Some(Frame::HtmlInsert { children, .. })
                | Some(Frame::HtmlDelete { children, .. }) => Some(children),
                _ => None,
            };
            if let Some(children) = children {
                if let Some(open) = children.iter().rposition(|node| {
                    matches!(node, InlineNode::RawInline(raw)
                        if html_tag(&raw.content).is_some_and(|tag|
                            tag.name.eq_ignore_ascii_case("a") && !tag.closing && !tag.self_closing))
                }) {
                    children.remove(open);
                    self.losses.push(MarkdownImportLoss {
                        message: "Flattened an HTML link inside a Markdown link label; its text was kept".into(),
                        line: Some(self.current_line),
                        kind: MarkdownLossKind::Unspellable,
                    });
                    return;
                }
            }
        }

        if tag.name.eq_ignore_ascii_case("code") {
            if tag.closing {
                self.html_code_depth = self.html_code_depth.saturating_sub(1);
            } else if !tag.self_closing {
                self.html_code_depth += 1;
            }
        }
        if tag.closing {
            if tag.name.eq_ignore_ascii_case("code") {
                if let Some(Frame::HtmlCode {
                    nested_code_depth, ..
                }) = self
                    .frames
                    .iter_mut()
                    .rev()
                    .find(|frame| matches!(frame, Frame::HtmlCode { .. }))
                {
                    if *nested_code_depth > 0 {
                        *nested_code_depth -= 1;
                        self.raw_inline(value.to_string());
                        return;
                    }
                }
            }
            let closes_top = match self.frames.last() {
                Some(Frame::HtmlEmphasis { tag: open, .. })
                | Some(Frame::HtmlCode { tag: open, .. })
                | Some(Frame::HtmlInsert { tag: open, .. })
                | Some(Frame::HtmlDelete { tag: open, .. }) => open.eq_ignore_ascii_case(tag.name),
                _ => false,
            };
            if closes_top {
                self.close_matched_html();
            } else {
                self.raw_inline(value.to_string());
            }
            return;
        }

        if tag.self_closing || is_void_html_tag(tag.name) {
            self.raw_inline(value.to_string());
            return;
        }

        let name = tag.name.to_ascii_lowercase();
        if name == "code" {
            if let Some(Frame::HtmlCode {
                nested_code_depth, ..
            }) = self
                .frames
                .iter_mut()
                .rev()
                .find(|frame| matches!(frame, Frame::HtmlCode { .. }))
            {
                *nested_code_depth += 1;
                self.raw_inline(value.to_string());
                return;
            }
        }
        // The one attributed tag that converts: section 8c writes a deletion as
        // `<del class="critic-delete">` and the Carve construct carries that
        // class itself, so nothing drops (markup-carve/carve#2845).
        if name == "del" && is_critic_delete_open(value) {
            self.push_frame(Frame::HtmlDelete {
                tag: name,
                open: value.to_string(),
                children: Vec::new(),
            });
            return;
        }
        // Only a BARE native tag converts to a Carve construct. An attributed
        // tag (`<b class="x">`) becomes a raw-inline span holding the start tag
        // and nothing else, so its attributes survive verbatim while the text
        // after its `>` stays in the document (markup-carve/carve-rs#2409).
        if !tag.bare && name != "code" {
            self.raw_inline(value.to_string());
            return;
        }
        let frame = match name.as_str() {
            "b" | "strong" => Frame::HtmlEmphasis {
                tag: name,
                kind: EmphasisKind::Strong,
                children: Vec::new(),
            },
            "i" | "em" => Frame::HtmlEmphasis {
                tag: name,
                kind: EmphasisKind::Italic,
                children: Vec::new(),
            },
            "mark" => Frame::HtmlEmphasis {
                tag: name,
                kind: EmphasisKind::Highlight,
                children: Vec::new(),
            },
            "sup" => Frame::HtmlEmphasis {
                tag: name,
                kind: EmphasisKind::Super,
                children: Vec::new(),
            },
            "sub" => Frame::HtmlEmphasis {
                tag: name,
                kind: EmphasisKind::Sub,
                children: Vec::new(),
            },
            "del" | "s" => Frame::HtmlEmphasis {
                tag: name,
                kind: EmphasisKind::Strike,
                children: Vec::new(),
            },
            "ins" => Frame::HtmlInsert {
                tag: name,
                children: Vec::new(),
            },
            "code" => Frame::HtmlCode {
                nested_code_depth: 0,
                line: self.current_line,
                tag: name,
                open: value.to_string(),
                native: tag.bare,
                children: Vec::new(),
            },
            _ => {
                self.raw_inline(value.to_string());
                return;
            }
        };
        self.push_frame(frame);
    }

    fn raw_inline(&mut self, content: String) {
        self.report_raw_span_trailing_whitespace(&content);
        self.inline(InlineNode::RawInline(RawInline {
            cell_wraps: None,
            format: "html".to_string(),
            content,
            injected: false,
            pos: None,
        }));
    }

    /// Report every whitespace run a raw span would leave at the end of a
    /// content line.
    ///
    /// CARVE-P2-025 drops such a run from every content line and a verbatim
    /// run crossing a line break is no exception, so the bytes are written and
    /// never read back (markup-carve/carve#2804). Whitespace anywhere else in
    /// the span survives and is not reported.
    fn report_raw_span_trailing_whitespace(&mut self, content: &str) {
        let newlines = content.matches('\n').count();
        if newlines == 0 {
            return;
        }
        // `current_line` is the line the event that OPENED the span starts on,
        // which is the first line of its content.
        let start = self.current_line;
        for (index, line) in content.split('\n').enumerate() {
            if index == newlines {
                break;
            }
            if line.ends_with(RAW_SPAN_DROPPED_WHITESPACE) {
                self.losses.push(MarkdownImportLoss {
                    message: RAW_SPAN_WHITESPACE_TRIMMED.to_owned(),
                    line: Some(start + index),
                    kind: MarkdownLossKind::RawSpanWhitespaceTrimmed,
                });
            }
        }
    }

    /// Collect one line of a block-level HTML element into the frame the
    /// element opened.
    fn raw_html(&mut self, value: &str) {
        match self.frames.last_mut() {
            Some(Frame::RawHtml(content)) => content.push_str(value),
            // The parser wraps every block-HTML line in an `HtmlBlock`, so a
            // line with no frame open is not a shape this reaches today. It
            // keeps the source as text rather than dropping it, which is what
            // inline HTML does with the same content.
            _ => self.text(value),
        }
    }

    /// Text lands in whatever is open: a code block collects it verbatim, an
    /// image's alt is a plain string on the node, everything else takes a node.
    fn text(&mut self, value: &str) {
        self.needs_email_walk |= value.contains('@');
        let inside_html_code = self.html_code_depth > 0;
        match self.frames.last_mut() {
            Some(Frame::CodeBlock { content, .. }) | Some(Frame::Metadata(content)) => {
                content.push_str(value)
            }
            Some(Frame::HtmlCode { children, .. }) => {
                if let Some(InlineNode::Text(text)) = children.last_mut() {
                    text.value.push_str(value);
                } else {
                    children.push(InlineNode::text(value));
                }
            }
            Some(Frame::Image { alt, .. }) => alt.push_str(value),
            _ => {
                let text = if inside_html_code {
                    value.to_string()
                } else {
                    value.replace(['\r', '\n'], " ")
                };
                self.inline(InlineNode::text(text));
            }
        }
    }

    fn start(&mut self, tag: Tag<'_>, source: &str, empty_title: bool) {
        if matches!(
            tag,
            Tag::Paragraph
                | Tag::Heading { .. }
                | Tag::BlockQuote(_)
                | Tag::List(_)
                | Tag::Item
                | Tag::CodeBlock(_)
                | Tag::HtmlBlock
                | Tag::Table(_)
                | Tag::TableHead
                | Tag::TableRow
                | Tag::TableCell
                | Tag::FootnoteDefinition(_)
                | Tag::MetadataBlock(_)
        ) {
            self.close_unclosed_html();
            if let Some(Frame::ListItem {
                pending, children, ..
            }) = self.frames.last_mut()
            {
                flush_inline_run(pending, children, self.needs_email_walk);
            }
            self.html_code_depth = 0;
        }
        if matches!(tag, Tag::Paragraph) {
            if let Some(Frame::ListItem { loose, .. }) = self.frames.last_mut() {
                *loose = true;
            }
        }

        let frame = match tag {
            Tag::Paragraph => Frame::Paragraph(Vec::new()),
            Tag::Heading { level, .. } => Frame::Heading(heading_level(level), Vec::new()),
            Tag::BlockQuote(_) => {
                if self.frames.is_empty() {
                    self.alert_kind = crate::markdown_github_alerts::first_line_kind(source);
                }
                Frame::BlockQuote(Vec::new())
            }
            Tag::List(start) => Frame::List {
                ordered: start.is_some(),
                start: start.map(|start| start as usize),
                delim: start.and_then(|_| {
                    let marker = source.trim_start_matches([' ', '\t']);
                    let digits = marker
                        .bytes()
                        .take_while(|byte| byte.is_ascii_digit())
                        .count();
                    (digits > 0 && marker.as_bytes().get(digits) == Some(&b')')).then_some(')')
                }),
                bullet_char: start.is_none().then(|| {
                    source
                        .trim_start_matches([' ', '\t'])
                        .chars()
                        .next()
                        .map(|marker| if marker == '+' { '-' } else { marker })
                        .unwrap_or('-')
                }),
                items: Vec::new(),
                tight: true,
            },
            Tag::Item => Frame::ListItem {
                checked: None,
                children: Vec::new(),
                loose: false,
                pending: Vec::new(),
            },
            Tag::CodeBlock(kind) => Frame::CodeBlock {
                lang: match kind {
                    // Only the first word of the info string is the language;
                    // the rest is the author's metadata, which Carve's fence
                    // has no slot for.
                    CodeBlockKind::Fenced(info) => fence_language(&info).map(str::to_string),
                    CodeBlockKind::Indented => None,
                },
                content: String::new(),
            },
            Tag::HtmlBlock => Frame::RawHtml(String::new()),
            Tag::Emphasis => Frame::Emphasis(EmphasisKind::Italic, Vec::new()),
            Tag::Strong => Frame::Emphasis(EmphasisKind::Strong, Vec::new()),
            Tag::Strikethrough => Frame::Emphasis(EmphasisKind::Strike, Vec::new()),
            Tag::Link {
                dest_url,
                title,
                link_type,
                id,
            } => Frame::Link {
                href: {
                    let href = if matches!(link_type, LinkType::Autolink | LinkType::Email) {
                        markdown_autolink_destination(&dest_url, link_type == LinkType::Email)
                    } else {
                        markdown_destination(&dest_url, false)
                    };
                    let authored = if link_type == LinkType::Inline {
                        authored_inline_destination(source, &dest_url)
                    } else {
                        &dest_url
                    };
                    record_authored_destination(&href, authored);
                    href
                },
                title: (!title.is_empty() || empty_title).then(|| title.to_string()),
                children: Vec::new(),
                autolink: matches!(link_type, LinkType::Autolink | LinkType::Email)
                    .then(|| source.strip_prefix('<')?.strip_suffix('>'))
                    .flatten()
                    .and_then(|text| {
                        crate::parse::autolink_href(text).map(|href| (href, text.to_owned()))
                    }),
                ref_label: (!id.is_empty()
                    && matches!(
                        link_type,
                        LinkType::Reference
                            | LinkType::ReferenceUnknown
                            | LinkType::Collapsed
                            | LinkType::CollapsedUnknown
                            | LinkType::Shortcut
                            | LinkType::ShortcutUnknown
                    ))
                .then(|| id.to_string()),
                raw_ref: (!id.is_empty()
                    && matches!(
                        link_type,
                        LinkType::Reference
                            | LinkType::ReferenceUnknown
                            | LinkType::Collapsed
                            | LinkType::CollapsedUnknown
                            | LinkType::Shortcut
                            | LinkType::ShortcutUnknown
                    ))
                .then(|| {
                    if matches!(link_type, LinkType::Collapsed | LinkType::CollapsedUnknown) {
                        format!("[{id}][]")
                    } else {
                        source.to_owned()
                    }
                }),
            },
            Tag::Image {
                dest_url, title, ..
            } => Frame::Image {
                src: {
                    let src = markdown_destination(&dest_url, false);
                    record_authored_destination(
                        &src,
                        authored_inline_destination(source, &dest_url),
                    );
                    src
                },
                title: (!title.is_empty() || empty_title).then(|| title.to_string()),
                alt: String::new(),
            },
            Tag::Table(alignments) => Frame::Table {
                alignments,
                rows: Vec::new(),
            },
            Tag::TableHead => Frame::TableRow {
                header: true,
                cells: Vec::new(),
            },
            Tag::TableRow => Frame::TableRow {
                header: false,
                cells: Vec::new(),
            },
            Tag::TableCell => Frame::TableCell(Vec::new()),
            Tag::FootnoteDefinition(label) => {
                let label = self.footnote_label(&label);
                if self.track_code_lines {
                    self.code_lines.insert(Some(label.clone()), Vec::new());
                }
                Frame::FootnoteDef {
                    label,
                    children: Vec::new(),
                    outer_levels: (0, 0),
                }
            }
            Tag::MetadataBlock(_) => Frame::Metadata(String::new()),
            // Nothing else is enabled, so nothing reaches here; an unopened
            // frame would desync the stack on the matching end event, hence a
            // frame rather than a skip.
            _ => Frame::Paragraph(Vec::new()),
        };

        self.push_frame(frame);
    }

    /// Push a frame, unless it would nest past [`MAX_IMPORT_LEVELS`].
    ///
    /// A refusal sets `over_depth` instead of pushing, which stops the event loop
    /// - so the matching end event never arrives and the stack stays balanced.
    fn push_frame(&mut self, mut frame: Frame) {
        if let Frame::FootnoteDef { outer_levels, .. } = &mut frame {
            *outer_levels = (self.block_levels, self.inline_levels);
            self.block_levels = 1;
            self.inline_levels = 0;
            self.frames.push(frame);
            return;
        }
        let (block, inline) = levels_added(&frame);
        if self.block_levels + block > MAX_IMPORT_LEVELS
            || self.inline_levels + inline > MAX_IMPORT_LEVELS
        {
            self.over_depth = true;
            return;
        }
        self.block_levels += block;
        self.inline_levels += inline;
        self.frames.push(frame);
    }

    fn pop_frame(&mut self) -> Option<Frame> {
        let frame = self.frames.pop()?;
        if let Frame::FootnoteDef { outer_levels, .. } = &frame {
            (self.block_levels, self.inline_levels) = *outer_levels;
            return Some(frame);
        }
        let (block, inline) = levels_added(&frame);
        self.block_levels -= block;
        self.inline_levels -= inline;
        Some(frame)
    }

    fn end(&mut self, _tag: TagEnd) {
        self.close_unclosed_html();
        self.close();
    }

    fn close_unclosed_html(&mut self) {
        while matches!(
            self.frames.last(),
            Some(
                Frame::HtmlEmphasis { .. }
                    | Frame::HtmlCode { .. }
                    | Frame::HtmlInsert { .. }
                    | Frame::HtmlDelete { .. }
            )
        ) {
            self.close();
        }
    }

    fn close(&mut self) {
        let Some(mut frame) = self.pop_frame() else {
            return;
        };
        if self.needs_email_walk {
            let inside_link = self
                .frames
                .iter()
                .any(|frame| matches!(frame, Frame::Link { .. }));
            match &mut frame {
                Frame::Paragraph(children)
                | Frame::Heading(_, children)
                | Frame::TableCell(children) => {
                    crate::markdown_gfm_autolinks::link_email_inlines(children, inside_link);
                }
                _ => {}
            }
        }
        match &mut frame {
            Frame::Paragraph(children)
            | Frame::Heading(_, children)
            | Frame::TableCell(children) => {
                preserve_code_text_at_depth(children, 0);
                self.html_code_depth = 0;
            }
            _ => {}
        }

        match frame {
            Frame::Paragraph(children) => self.block(BlockNode::Paragraph(Paragraph {
                attrs: None,
                children,
                at_content_column: true,
                block_image: false,
                pos: None,
            })),
            Frame::Heading(level, children) if children.is_empty() => {
                self.block(BlockNode::RawBlock(RawBlock {
                    attrs: None,
                    format: "html".to_string(),
                    content: format!("<h{level}></h{level}>"),
                    pos: None,
                }));
            }
            Frame::Heading(level, children) => self.block(BlockNode::Heading(Heading {
                attrs: None,
                level,
                children,
                pos: None,
            })),
            Frame::BlockQuote(children) => {
                let quote = BlockQuote {
                    attrs: None,
                    children,
                    // Markdown has one spelling, so an import carries none.
                    fenced: false,
                    pos: None,
                };
                let alert = if self.frames.is_empty() {
                    self.alert_kind.take()
                } else {
                    None
                };
                self.block(match alert {
                    Some(kind) => crate::markdown_github_alerts::admonition(kind, quote),
                    None => BlockNode::BlockQuote(quote),
                })
            }
            Frame::List {
                ordered,
                start,
                delim,
                bullet_char,
                items,
                tight,
            } => self.block(BlockNode::List(List {
                attrs: None,
                ordered,
                // A list that starts at 1 is the default, and recording it
                // makes the writer spell out a start the author did not.
                start: start.filter(|start| *start != 1),
                ol_type: None,
                bare_marker: false,
                delim,
                bullet_char,
                tight,
                items,
                pos: None,
            })),
            Frame::ListItem {
                checked,
                mut children,
                loose,
                mut pending,
            } => {
                self.html_code_depth = 0;
                flush_inline_run(&mut pending, &mut children, self.needs_email_walk);
                let item = ListItem {
                    attrs: None,
                    checked,
                    // GFM has `[ ]` and `[x]` only.
                    task_state: None,
                    children,
                    pos: None,
                };
                if let Some(Frame::List { items, tight, .. }) = self.frames.last_mut() {
                    *tight = *tight && !loose;
                    items.push(item);
                }
            }
            Frame::CodeBlock { lang, mut content } => {
                if !content.is_empty() && !content.ends_with('\n') {
                    content.push('\n');
                }
                self.block(BlockNode::CodeBlock(CodeBlock {
                    attrs: None,
                    lang,
                    title: None,
                    label: None,
                    content,
                    pos: None,
                }));
            }
            // The frame has already been popped, so the raw block folds into
            // the container the element sits in - a quote, a list item or a
            // footnote definition - instead of landing at the top of the
            // document ahead of the container it was written inside.
            Frame::RawHtml(content) => {
                if let Some((token, math)) = self.carrier_placeholder(&content) {
                    // A math target must not merge with its attribute or caption placeholders.
                    let block = if math {
                        BlockNode::Comment(Comment {
                            content: token,
                            block: false,
                            delimited: false,
                            pos: None,
                        })
                    } else {
                        BlockNode::Paragraph(Paragraph {
                            children: vec![InlineNode::text(token)],
                            at_content_column: true,
                            ..Default::default()
                        })
                    };
                    self.block(block);
                    return;
                }
                let content = content.trim_end_matches('\n').to_string();
                // carve-js renders a block-level HTML element inline only when it
                // is a tight item's SOLE content (right after the marker, nothing
                // before it); a block element on a continuation line, after the
                // item's own text, stays a block. Match that: the item must have
                // collected neither a block nor any pending inline run yet.
                let inline_in_tight_item = matches!(
                    self.frames.last(),
                    Some(Frame::ListItem { loose: false, children, pending, .. })
                        if children.is_empty() && pending.is_empty()
                );
                if inline_in_tight_item {
                    self.raw_inline(content);
                } else {
                    self.block(BlockNode::RawBlock(RawBlock {
                        attrs: None,
                        format: "html".to_string(),
                        content,
                        pos: None,
                    }));
                }
            }
            // A native inline-HTML frame reaching close() here was NOT closed by
            // its own end tag - a paragraph or the document ended first. Its open
            // tag never paired, so it is not a Carve construct: emit the bare open
            // tag as a raw-inline span and let its collected content stand, the
            // way carve-js leaves an unclosed `<b>` as `` `<b>`{=html} `` text.
            Frame::HtmlEmphasis { tag, children, .. } | Frame::HtmlInsert { tag, children, .. } => {
                self.raw_inline(format!("<{tag}>"));
                for child in children {
                    self.inline(child);
                }
            }
            Frame::HtmlCode {
                line,
                open,
                mut children,
                ..
            } => {
                self.losses.push(MarkdownImportLoss { message: "Preserved HTML code markup as raw HTML; targets and profiles that escape or omit raw HTML change its code structure and content".into(), line: Some(line), kind: MarkdownLossKind::RawCodeFallback });
                let closing_line = self.current_line;
                self.current_line = line;
                self.raw_inline(open);
                self.current_line = closing_line;
                if self.needs_email_walk {
                    let inside_link = self
                        .frames
                        .iter()
                        .any(|frame| matches!(frame, Frame::Link { .. }));
                    crate::markdown_gfm_autolinks::link_email_inlines(&mut children, inside_link);
                }
                preserve_code_text(&mut children);
                for child in children {
                    self.inline(child);
                }
            }
            Frame::HtmlDelete { open, children, .. } => {
                self.raw_inline(open);
                for child in children {
                    self.inline(child);
                }
            }
            // Markdown emphasis IS Carve emphasis; only the spelling differs,
            // and the spelling belongs to the writer.
            Frame::Emphasis(kind, children) => self.inline(InlineNode::Emphasis(Emphasis {
                attrs: None,
                kind,
                children,
                pos: None,
            })),
            Frame::Link {
                href,
                autolink: Some((autolink_href, text)),
                title,
                ..
            } if title.is_none() => {
                if href == autolink_href || !text.bytes().any(|byte| matches!(byte, b'[' | b']')) {
                    self.inline(InlineNode::AutoLink(AutoLink {
                        attrs: None,
                        href: autolink_href,
                        text,
                        pos: None,
                    }));
                } else {
                    self.inline(InlineNode::Link(Link {
                        attrs: None,
                        href,
                        title: None,
                        children: vec![InlineNode::text(text)],
                        ref_label: None,
                        raw_ref: None,
                        resolved_empty_reference: false,
                        from_crossref: false,
                        from_heading_reference: false,
                        pos: None,
                    }));
                }
            }
            Frame::Link {
                href,
                title,
                children,
                ref_label,
                raw_ref,
                ..
            } => self.inline(InlineNode::Link(Link {
                attrs: None,
                href,
                title,
                children,
                ref_label,
                raw_ref,
                resolved_empty_reference: false,
                from_crossref: false,
                from_heading_reference: false,
                pos: None,
            })),
            Frame::Image { src, title, alt } => self.inline(InlineNode::Image(Image {
                attrs: None,
                src,
                alt,
                title,
                ref_label: None,
                raw_ref: None,
                resolved_empty_reference: false,
                pos: None,
            })),
            Frame::Table { rows, .. } if !rows.is_empty() => self.block(BlockNode::Table(Table {
                source_format: None,
                attrs: None,
                caption: None,
                short_caption: None,
                columns: Vec::new(),
                rows,
                row_groups: None,
                separator_widths: None,
                pos: None,
            })),
            Frame::Table { .. } => {}
            Frame::TableRow { header, cells } => {
                // An all-blank header with no alignment is the scaffolding the
                // Markdown writer adds to a headerless table (CARVE-P11-056).
                // A blank body row is a row (CARVE-P9-082) and is kept.
                if header
                    && !cells.is_empty()
                    && cells.iter().all(|cell| {
                        cell.children.is_empty()
                            && cell.align.is_none()
                            && cell.valign.is_none()
                            && cell.attrs.is_none()
                    })
                {
                    let cell = if cells.len() == 1 { "cell" } else { "cells" };
                    self.losses.push(MarkdownImportLoss { message: format!(
                        "Dropped an empty header row of {} {cell}; a headerless GFM table has no Carve header",
                        cells.len()
                    ), line: Some(self.current_line), kind: MarkdownLossKind::Unspellable });
                    return;
                }
                let row = TableRow {
                    cells,
                    attrs: None,
                    continuation_lines: None,
                    pos: None,
                };
                if let Some(Frame::Table { rows, .. }) = self.frames.last_mut() {
                    rows.push(row);
                }
            }
            Frame::TableCell(children) => {
                // Alignment is a property of the COLUMN in Markdown and of the
                // CELL in Carve, so it is read at the moment the cell closes,
                // when its column index is the row's current cell count.
                let (header, column) = match self.frames.last() {
                    Some(Frame::TableRow { header, cells }) => (*header, cells.len()),
                    _ => (false, 0),
                };
                let align = self
                    .frames
                    .iter()
                    .rev()
                    .find_map(|frame| match frame {
                        Frame::Table { alignments, .. } => Some(alignments),
                        _ => None,
                    })
                    .and_then(|alignments| match alignments.get(column) {
                        Some(Alignment::Left) => Some(TableAlign::Left),
                        Some(Alignment::Center) => Some(TableAlign::Center),
                        Some(Alignment::Right) => Some(TableAlign::Right),
                        _ => None,
                    });
                let cell = TableCell {
                    colspan: None,
                    rowspan: None,
                    header,
                    span: None,
                    align,
                    valign: None,
                    attrs: None,
                    children,
                    blocks: None,
                    pos: None,
                };
                if let Some(Frame::TableRow { cells, .. }) = self.frames.last_mut() {
                    cells.push(cell);
                }
            }
            // A definition is not a block in the document: Carve holds it in a
            // map keyed by label, so a note may be written anywhere and still
            // render at the end.
            Frame::FootnoteDef {
                label, children, ..
            } => {
                self.footnote_defs.insert(label, children);
            }
            Frame::Metadata(content) => {
                self.frontmatter = Some(Frontmatter {
                    format: "yaml".to_string(),
                    content: content.trim_end_matches('\n').to_string(),
                    pos: None,
                });
            }
        }
    }

    /// Close a native inline-HTML frame that its OWN matching end tag closed,
    /// building the Carve construct. The caller has confirmed the top frame is
    /// the one the end tag pairs with; anything reaching the generic `close()`
    /// was left unpaired and falls back to raw there instead.
    /// Take the inline run's trailing node when it is a deletion, so the
    /// insertion that just closed can fold it into one substitution.
    fn take_trailing_critic_delete(&mut self) -> Option<Vec<InlineNode>> {
        let children = match self.frames.last_mut() {
            Some(Frame::Paragraph(children))
            | Some(Frame::Heading(_, children))
            | Some(Frame::Emphasis(_, children))
            | Some(Frame::HtmlEmphasis { children, .. })
            | Some(Frame::HtmlInsert { children, .. })
            | Some(Frame::HtmlDelete { children, .. })
            | Some(Frame::Link { children, .. })
            | Some(Frame::TableCell(children))
            | Some(Frame::ListItem {
                pending: children, ..
            }) => children,
            _ => return None,
        };
        if !matches!(children.last(), Some(InlineNode::CriticDelete(_))) {
            return None;
        }
        match children.pop() {
            Some(InlineNode::CriticDelete(delete)) => Some(delete.children),
            _ => None,
        }
    }

    fn close_matched_html(&mut self) {
        match self.pop_frame() {
            Some(Frame::HtmlEmphasis { kind, children, .. }) => {
                self.inline(InlineNode::Emphasis(Emphasis {
                    attrs: None,
                    kind,
                    children,
                    pos: None,
                }));
            }
            Some(Frame::HtmlCode {
                line,
                open,
                native,
                mut children,
                ..
            }) => {
                if self.needs_email_walk {
                    let inside_link = self
                        .frames
                        .iter()
                        .any(|frame| matches!(frame, Frame::Link { .. }));
                    crate::markdown_gfm_autolinks::link_email_inlines(&mut children, inside_link);
                }
                let mut content = String::new();
                fn append_plain_code(children: &[InlineNode], content: &mut String) -> bool {
                    children.iter().all(|child| match child {
                        InlineNode::Text(text) => {
                            content.push_str(&text.value);
                            true
                        }
                        InlineNode::SoftBreak(_) => {
                            content.push('\n');
                            true
                        }
                        InlineNode::RawInline(raw)
                            if raw.format == "html" && raw.content == "<!---->" =>
                        {
                            true
                        }
                        InlineNode::Span(span) if span.injected && span.attrs.is_none() => {
                            append_plain_code(&span.children, content)
                        }
                        _ => false,
                    })
                }
                let plain = append_plain_code(&children, &mut content);
                if plain && native {
                    self.native_code(content.replace("\r\n", "\n").replace('\r', "\n"), line);
                } else {
                    self.losses.push(MarkdownImportLoss { message: "Preserved HTML code markup as raw HTML; targets and profiles that escape or omit raw HTML change its code structure and content".into(), line: Some(line), kind: MarkdownLossKind::RawCodeFallback });
                    let closing_line = self.current_line;
                    self.current_line = line;
                    self.raw_inline(open);
                    self.current_line = closing_line;
                    let mut children = children;
                    preserve_code_text(&mut children);
                    for child in children {
                        self.inline(child);
                    }
                    self.raw_inline("</code>".into());
                }
            }
            Some(Frame::HtmlInsert { children, .. }) => {
                // A deletion immediately followed by an insertion is ONE
                // construct, not two: section 8c writes a substitution as that
                // pair, and leaving them apart would read it back as a deletion
                // beside an unrelated insertion (markup-carve/carve#2845).
                if let Some(old) = self.take_trailing_critic_delete() {
                    self.inline(InlineNode::CriticSubstitute(CriticSubstitute {
                        old,
                        new: children,
                        pos: None,
                    }));
                    return;
                }
                self.inline(InlineNode::CriticInsert(CriticInsert {
                    children,
                    attrs: None,
                    pos: None,
                }));
            }
            Some(Frame::HtmlDelete { open, children, .. }) => {
                // A body that would close or re-open the braced construct stays
                // the raw span the tag arrived as, rather than leaning on the
                // writer's escapes to hold it together.
                if breaks_out_of_a_critic_body(&children) {
                    self.raw_inline(open);
                    for child in children {
                        self.inline(child);
                    }
                    self.raw_inline("</del>".to_string());
                    return;
                }
                self.inline(InlineNode::CriticDelete(CriticDelete {
                    children,
                    attrs: None,
                    pos: None,
                }));
            }
            Some(other) => self.push_frame(other),
            None => {}
        }
    }

    fn block(&mut self, mut node: BlockNode) {
        match self.frames.last_mut() {
            // A block ends the item's content column, so whatever inline run
            // was still open closes as its own paragraph FIRST. Without this a
            // nested list would sort ahead of the text the item opened with.
            Some(Frame::ListItem {
                children, pending, ..
            }) => {
                if !pending.is_empty() {
                    self.html_code_depth = 0;
                }
                flush_inline_run(pending, children, self.needs_email_walk);
                separate_from_previous_list(children.last(), &mut node);
                children.push(node);
            }
            Some(Frame::BlockQuote(children)) | Some(Frame::FootnoteDef { children, .. }) => {
                separate_from_previous_list(children.last(), &mut node);
                children.push(node)
            }
            _ => {
                separate_from_previous_list(self.blocks.last(), &mut node);
                self.blocks.push(node)
            }
        }
    }

    fn inline(&mut self, node: InlineNode) {
        match self.frames.last_mut() {
            Some(Frame::Paragraph(children))
            | Some(Frame::Heading(_, children))
            | Some(Frame::Emphasis(_, children))
            | Some(Frame::HtmlEmphasis { children, .. })
            | Some(Frame::HtmlCode { children, .. })
            | Some(Frame::HtmlInsert { children, .. })
            | Some(Frame::HtmlDelete { children, .. })
            | Some(Frame::Link { children, .. })
            | Some(Frame::TableCell(children)) => children.push(node),
            // A TIGHT item spells its content with no paragraph around it, so
            // the run arrives here. It is buffered and closed whole - one
            // paragraph for the item, not one per node.
            Some(Frame::ListItem { pending, .. }) => pending.push(node),
            // An image's alt is a plain string on the node, so a construct
            // inside it contributes its TEXT. Turning it into a node of its own
            // would put it outside the image entirely, which is where it used
            // to go.
            Some(Frame::Image { alt, .. }) => alt.push_str(&inline_text(&node)),
            // Inline content with no inline container open is content the
            // author wrote outside any block; it becomes a paragraph rather
            // than being dropped.
            _ => self.block(BlockNode::Paragraph(Paragraph {
                attrs: None,
                children: vec![node],
                at_content_column: true,
                block_image: false,
                pos: None,
            })),
        }
    }

    fn finish(mut self) -> (Document, Vec<MarkdownImportLoss>, Vec<Option<usize>>) {
        // Truncated input can leave frames open; closing them keeps the content
        // rather than discarding a half-built tree.
        while !self.frames.is_empty() {
            self.close();
        }

        let frontmatter = self
            .frontmatter
            .as_ref()
            .map(|frontmatter| parse_frontmatter(&frontmatter.content))
            .unwrap_or_default();

        if !self.footnote_labels.is_empty()
            || self
                .footnote_numbers
                .keys()
                .any(|label| label.contains('|'))
        {
            struct RenameFootnotes<'a>(&'a BTreeMap<String, String>);
            impl crate::include_walk::SubtreeVisitor for RenameFootnotes<'_> {
                fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
                    for block in blocks {
                        crate::include_walk::visit_block_children(block, self);
                    }
                }
                fn inlines(&mut self, nodes: &mut Vec<InlineNode>) {
                    for node in nodes {
                        if let InlineNode::Footnote(note) = node {
                            if let Some(label) = &note.id {
                                if let Some(renamed) = self.0.get(label) {
                                    note.id = Some(renamed.clone());
                                } else if label.contains('|') {
                                    *node = InlineNode::text(format!("[^{label}]"));
                                }
                            }
                        }
                        crate::include_walk::visit_inline_children(node, self);
                    }
                }
            }
            use crate::include_walk::SubtreeVisitor as _;
            let mut rename = RenameFootnotes(&self.footnote_labels);
            rename.blocks(&mut self.blocks);
            for blocks in self.footnote_defs.values_mut() {
                rename.blocks(blocks);
            }
        }
        let document = Document {
            frontmatter,
            frontmatter_raw: self.frontmatter,
            footnote_defs: self.footnote_defs,
            footnote_def_pos: BTreeMap::new(),
            children: self.blocks,
            source_len: 0,
            ingest_payload_len: 0,
        };
        let mut code_lines = self.code_lines.remove(&None).unwrap_or_default();
        for label in document.footnote_defs.keys() {
            code_lines.extend(
                self.code_lines
                    .remove(&Some(label.clone()))
                    .unwrap_or_default(),
            );
        }
        (document, self.losses, code_lines)
    }
}

/// Read the flat `key: value` pairs a Carve document exposes alongside the raw
/// block. Deliberately not a YAML parser - the raw block keeps the source, so
/// anything structured survives there and only the scalars are lifted, which is
/// what the Carve parser's own frontmatter handling does.
/// Close a tight list item's collected inline run as the ONE paragraph it is.
///
/// `pulldown-cmark` spells a tight item by emitting its inlines with no
/// `Start(Paragraph)` around them - that absence IS the tightness - so the run
/// arrives with nothing to collect it into. The content still needs a
/// paragraph; what it does not need is one per node, which is what wrapping
/// each arriving node separately produced (markup-carve/carve-rs#969).
/// A bullet list right after another one takes the other bullet, as carve-js
/// writes it: with the same character the two would read as one list.
fn separate_from_previous_list(previous: Option<&BlockNode>, node: &mut BlockNode) {
    let (Some(BlockNode::List(previous)), BlockNode::List(list)) = (previous, node) else {
        return;
    };
    if previous.ordered || list.ordered {
        return;
    }
    let bullet = |list: &List| list.bullet_char.unwrap_or('-');
    if bullet(previous) == bullet(list) {
        list.bullet_char = Some(if bullet(list) == '-' { '*' } else { '-' });
    }
}

fn flush_inline_run(
    pending: &mut Vec<InlineNode>,
    children: &mut Vec<BlockNode>,
    needs_email_walk: bool,
) {
    if pending.is_empty() {
        return;
    }
    if needs_email_walk {
        crate::markdown_gfm_autolinks::link_email_inlines(pending, false);
    }
    preserve_code_text_at_depth(pending, 0);
    children.push(BlockNode::Paragraph(Paragraph {
        attrs: None,
        children: std::mem::take(pending),
        at_content_column: true,
        block_image: false,
        pos: None,
    }));
}

/// The text a construct inside an image's alt contributes to it.
///
/// `alt` is a plain string on the node, so an emphasis, a code span or a link
/// written inside `![...]` has no node to become. CommonMark flattens it the
/// same way - `![a *b* c](i.png)` carries `alt="a b c"` - so the text is what
/// is kept. A break contributes its authored newline to the image alt scalar.
fn critic_body_probe_text(node: &InlineNode, out: &mut String) {
    match node {
        // Delimiters inside verbatim nodes cannot close the surrounding deletion.
        // A separator also prevents delimiters on either side from pairing.
        InlineNode::Code(_) | InlineNode::LiteralInline(_) | InlineNode::RawInline(_) => {
            out.push(' ')
        }
        InlineNode::Emphasis(emphasis) => {
            for child in &emphasis.children {
                critic_body_probe_text(child, out);
            }
        }
        InlineNode::Link(link) => {
            for child in &link.children {
                critic_body_probe_text(child, out);
            }
        }
        InlineNode::Span(span) => {
            for child in &span.children {
                critic_body_probe_text(child, out);
            }
        }
        _ => out.push_str(&inline_text(node)),
    }
}

/// A deletion body that would close or re-open the braced construct it is about
/// to be written into (markup-carve/carve#2845).
fn breaks_out_of_a_critic_body(children: &[InlineNode]) -> bool {
    let mut text = String::new();
    for child in children {
        critic_body_probe_text(child, &mut text);
    }
    text.contains(['{', '}', '\\', '\n']) || text.contains("~>")
}

fn inline_text(node: &InlineNode) -> String {
    match node {
        InlineNode::Text(text) => text.value.clone(),
        InlineNode::Code(code) => code.value.clone(),
        InlineNode::Emphasis(emphasis) => inline_run_text(&emphasis.children),
        InlineNode::Link(link) => inline_run_text(&link.children),
        InlineNode::Span(span) => inline_run_text(&span.children),
        InlineNode::Image(image) => image.alt.clone(),
        InlineNode::SoftBreak(_) | InlineNode::HardBreak(_) => "\n".into(),
        _ => String::new(),
    }
}

fn inline_run_text(nodes: &[InlineNode]) -> String {
    nodes.iter().map(inline_text).collect()
}

fn parse_frontmatter(content: &str) -> BTreeMap<String, String> {
    content
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            let key = key.trim();
            (!key.is_empty() && !key.starts_with('#'))
                .then(|| (key.to_string(), value.trim().to_string()))
        })
        .collect()
}

fn has_empty_inline_title(source: &str, quote_depth: usize) -> bool {
    let Some(source) = source
        .trim_end_matches([' ', '\t', '\r', '\n'])
        .strip_suffix(')')
    else {
        return false;
    };
    let source = source.trim_end_matches([' ', '\t', '\r', '\n']);
    let Some(prefix) = ["\"\"", "''", "()"]
        .iter()
        .find_map(|marker| source.strip_suffix(marker))
    else {
        return false;
    };
    let prefix = if quote_depth > 0 && prefix.contains('\n') {
        Cow::Owned(
            prefix
                .split('\n')
                .enumerate()
                .map(|(index, mut line)| {
                    if index > 0 {
                        for _ in 0..quote_depth {
                            let trimmed = line.trim_start_matches([' ', '\t']);
                            if let Some(rest) = trimmed.strip_prefix('>') {
                                line = rest.strip_prefix([' ', '\t']).unwrap_or(rest);
                            } else {
                                break;
                            }
                        }
                    }
                    line
                })
                .collect::<Vec<_>>()
                .join("\n"),
        )
    } else {
        Cow::Borrowed(prefix)
    };
    prefix
        .as_bytes()
        .last()
        .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
        && !prefix
            .trim_end_matches([' ', '\t', '\r', '\n'])
            .ends_with('(')
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[derive(Clone, Copy)]
struct HtmlTag<'a> {
    name: &'a str,
    closing: bool,
    self_closing: bool,
    bare: bool,
}

/// Read just enough of an HTML fragment to pair the events pulldown-cmark
/// emits. Declarations, comments and processing instructions deliberately do
/// not look like tags here: each is a complete standalone raw fragment.
fn html_tag(fragment: &str) -> Option<HtmlTag<'_>> {
    let bytes = fragment.as_bytes();
    if bytes.first() != Some(&b'<') || bytes.last() != Some(&b'>') {
        return None;
    }

    let mut cursor = 1;
    let closing = bytes.get(cursor) == Some(&b'/');
    if closing {
        cursor += 1;
    }
    if matches!(bytes.get(cursor), Some(b'!') | Some(b'?')) {
        return None;
    }
    let start = cursor;
    while bytes
        .get(cursor)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b':' | b'_'))
    {
        cursor += 1;
    }
    if cursor == start {
        return None;
    }

    Some(HtmlTag {
        name: &fragment[start..cursor],
        closing,
        self_closing: fragment[..fragment.len() - 1].trim_end().ends_with('/'),
        // A bare open tag is `<name>` with nothing between the name and `>` - no
        // attributes, no interior whitespace. Only a bare native tag converts to
        // a Carve construct; an attributed one is kept verbatim as raw HTML so
        // the attributes are not silently dropped, matching carve-js.
        bare: !closing && bytes.get(cursor) == Some(&b'>'),
    })
}

fn is_void_html_tag(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

#[cfg(test)]
#[path = "markdown_import/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "markdown_import/destination_encoding_tests.rs"]
mod destination_encoding_tests;

#[cfg(test)]
#[path = "markdown_import/decoded_line_ending_tests.rs"]
mod decoded_line_ending_tests;
