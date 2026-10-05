use std::collections::{BTreeMap, BTreeSet};

use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{NodeData, RcDom};

use crate::ast::*;
use crate::extension::Options;
use crate::render_depth::{push_block_children, push_inline_children};

use super::LintWarning;

pub(super) fn collect(
    source: &str,
    doc: &Document,
    options: &Options<'_>,
    to_byte: &dyn Fn(usize) -> usize,
    out: &mut Vec<LintWarning>,
) {
    let mut blocks: Vec<_> = doc.children.iter().map(|b| (b, 0)).collect();
    for body in doc.footnote_defs.values() {
        blocks.extend(body.iter().map(|b| (b, 0)));
    }
    blocks.reverse();
    let mut inlines = Vec::new();
    let mut headings = Vec::new();
    let mut inline_spans = Vec::new();
    let mut ignored = BTreeSet::new();
    while let Some((block, depth)) = blocks.pop() {
        match block {
            BlockNode::Heading(heading) => headings.push(heading),
            BlockNode::CodeBlock(code) => ignore(code.pos.clone(), &mut ignored),
            BlockNode::RawBlock(raw) => ignore(raw.pos.clone(), &mut ignored),
            BlockNode::Comment(comment) => ignore(comment.pos.clone(), &mut ignored),
            BlockNode::Figure(figure) => {
                if matches!(&*figure.target, FigureTarget::CodeBlock(_)) {
                    ignore(figure.pos.clone(), &mut ignored);
                }
            }
            _ => {}
        }
        let first_child = blocks.len();
        push_block_children(block, depth, &mut blocks, &mut inlines);
        blocks[first_child..].reverse();
    }
    let mut explicit_ids = BTreeSet::new();
    for heading in headings {
        let attrs = heading.attrs.as_ref();
        // Parsing stamps generated ids without an authored order slot.
        // Keep that distinction when checking duplicate explicit ids.
        let explicit = attrs
            .filter(|a| a.order.contains(&AttrSlot::Id))
            .and_then(|a| a.id.as_ref());
        let base = explicit.cloned().unwrap_or_else(|| {
            crate::parse::slugify_parse(
                &crate::render::plain_inlines(&heading.children),
                options.heading_id_options(),
            )
        });
        let collision = match explicit {
            Some(id) => !explicit_ids.insert(id),
            None => attrs
                .and_then(|a| a.id.as_ref())
                .is_some_and(|id| id != &base),
        };
        if collision {
            report(
                heading.pos.clone(),
                "duplicate-heading-id",
                format!("Heading id \"{base}\" collides with another heading or an explicit id."),
                to_byte,
                out,
            );
        }
    }
    let definitions: BTreeMap<_, _> = doc
        .footnote_defs
        .keys()
        .map(|label| (label_key(label), label))
        .collect();
    let mut referenced = BTreeSet::new();
    let id_kinds = explicit_id_kinds(doc);
    let mut fragment_links = Vec::new();
    while let Some((inline, depth)) = inlines.pop() {
        if let Some(pos) = inline.pos() {
            inline_spans.push((to_byte(pos.start_offset), to_byte(pos.end_offset)));
        }
        match inline {
            InlineNode::CrossRef(reference) if reference.href.is_none() => {
                let target = &reference.target;
                let message = match id_kinds.get(&fold_id(target)) {
                    Some((id, kind)) => format!(
                        "Cross-reference </#{target}> names the id \"{id}\", which is on a {kind}; a cross-reference reaches only headings and numbered captions, so it renders as the literal text \"</#{target}>\". Link to it with [text](#{id})."
                    ),
                    None => format!("Cross-reference </#{target}> has no matching heading id."),
                };
                report(
                    reference.pos.clone(),
                    "broken-crossref",
                    message,
                    to_byte,
                    out,
                );
            }
            InlineNode::Link(link) if link.ref_label.is_some() && link.href.is_empty() => report(
                link.pos.clone(),
                "unresolved-reference-link",
                "Reference link has no matching definition or heading.".into(),
                to_byte,
                out,
            ),
            InlineNode::Link(link) if link.href.starts_with('#') && !link.from_crossref => {
                fragment_links.push((link.href.as_str(), link.pos.clone()));
            }
            InlineNode::Footnote(note) => {
                if let Some(label) = &note.id {
                    if let Some(&key) = definitions.get(&label_key(label)) {
                        referenced.insert(key.clone());
                    } else {
                        report(
                            note.pos.clone(),
                            "unresolved-footnote",
                            format!("Footnote reference [^{label}] has no matching definition."),
                            to_byte,
                            out,
                        );
                    }
                }
            }
            _ => {}
        }
        push_inline_children(inline, depth, &mut inlines);
    }
    check_fragment_links(source, options, &fragment_links, to_byte, out);
    let mut sites = BTreeMap::<String, Pos>::new();
    let mut first_keys = BTreeMap::<String, String>::new();
    let definition =
        regex::Regex::new(r"^(?:[ \t]*>[ ]?)*[ \t]*(?:[-+*] |[0-9]+[.)] |: )*\[\^([^]\r\n]+)\]:")
            .unwrap();
    inline_spans.sort_unstable();
    let mut span_index = 0;
    let mut byte = 0;
    let mut codepoints = 0;
    let endings = regex::Regex::new(r"\r\n|\r|\n").unwrap();
    let ends: Vec<_> = endings
        .find_iter(source)
        .map(|m| (m.start(), m.end()))
        .chain(std::iter::once((source.len(), source.len())))
        .collect();
    for (index, (content_end, line_end)) in ends.into_iter().enumerate() {
        let line = &source[byte..content_end];
        let line_no = index + 1;
        if !ignored.contains(&line_no) {
            if let Some(captures) = definition.captures(line) {
                let label = captures[1].to_owned();
                if definitions.contains_key(&label_key(&label)) {
                    let marker = line.find("[^").unwrap();
                    while span_index < inline_spans.len()
                        && inline_spans[span_index].1 <= byte + marker
                    {
                        span_index += 1;
                    }
                    let is_inline = inline_spans
                        .get(span_index)
                        .is_some_and(|&(start, end)| start <= byte + marker && byte + marker < end);
                    if is_inline {
                        codepoints += source[byte..line_end].chars().count();
                        byte = line_end;
                        continue;
                    }
                    let end = captures.get(0).unwrap().end();
                    let pos = Pos {
                        start_line: line_no,
                        end_line: line_no,
                        start_column: line[..marker].chars().count() + 1,
                        end_column: line[..end].chars().count() + 1,
                        start_offset: codepoints + line[..marker].chars().count(),
                        end_offset: codepoints + line[..end].chars().count(),
                        file: None,
                    };
                    let key = label_key(&label);
                    if sites.contains_key(&label) {
                        report(Some(pos.clone()), "duplicate-footnote-definition", format!("Duplicate footnote definition [^{label}] is ignored; the first definition wins."), to_byte, out);
                    } else if let Some(first) = first_keys.get(&key) {
                        report(Some(pos.clone()), "footnote-labels-differ-only-in-whitespace", format!("Footnote labels [^{label}] and [^{first}] differ only in whitespace; the first definition wins."), to_byte, out);
                    }
                    first_keys.entry(key).or_insert_with(|| label.clone());
                    sites.entry(label).or_insert(pos);
                }
            }
        }
        codepoints += source[byte..line_end].chars().count();
        byte = line_end;
    }
    for label in doc.footnote_defs.keys() {
        if !referenced.contains(label) {
            let pos = sites
                .get(label)
                .cloned()
                .or_else(|| doc.footnote_def_pos.get(label).cloned());
            report(pos, "unused-footnote-definition", format!("Footnote definition [^{label}] is never referenced and is omitted from rendered output."), to_byte, out);
        }
    }
}

/// Cross-references resolve case-insensitively, so the lookup folds the same way.
fn fold_id(id: &str) -> String {
    id.chars().flat_map(char::to_lowercase).collect()
}

/// The first element carrying each id, keyed by folded id, named by node kind.
fn explicit_id_kinds(doc: &Document) -> BTreeMap<String, (String, String)> {
    let mut kinds = BTreeMap::new();
    let mut visit = |node_type: &'static str, attrs: &Attrs, _: Option<Pos>| {
        if let Some(id) = &attrs.id {
            kinds
                .entry(fold_id(id))
                .or_insert_with(|| (id.clone(), node_type.replace('_', " ")));
        }
    };
    super::walk_blocks(&doc.children, &mut visit);
    for body in doc.footnote_defs.values() {
        super::walk_blocks(body, &mut visit);
    }
    kinds
}

fn check_fragment_links(
    source: &str,
    options: &Options<'_>,
    links: &[(&str, Option<Pos>)],
    to_byte: &dyn Fn(usize) -> usize,
    out: &mut Vec<LintWarning>,
) {
    let names: Vec<_> = options.extensions.iter().map(|ext| ext.name()).collect();
    // Lint cannot know which ids another extension generates.
    if links.is_empty()
        || names
            .iter()
            .any(|name| !matches!(*name, "semantic-span" | "citations"))
    {
        return;
    }
    let citations = names.contains(&"citations");
    let ids = rendered_ids(source, options);
    let mut ids_by_fold = BTreeMap::new();
    for id in &ids {
        ids_by_fold.entry(fold_id(id)).or_insert(id.as_str());
    }
    for (href, pos) in links {
        // A browser strips a `:~:` text directive before it looks the id up.
        let fragment = href[1..].split(":~:").next().unwrap_or_default();
        if fragment.is_empty() {
            continue;
        }
        let decoded = percent_decode(fragment).unwrap_or_else(|| fragment.to_owned());
        if ids.contains(fragment) || ids.contains(&decoded) {
            continue;
        }
        // HTML scrolls `#top` to the start of the page without any element.
        if decoded.eq_ignore_ascii_case("top") {
            continue;
        }
        if citations && (decoded.starts_with("ref-") || decoded.starts_with("cite-")) {
            continue;
        }
        let message = match ids_by_fold.get(&fold_id(&decoded)) {
            Some(real) => format!("Link to \"{href}\" matches no id; the id \"{real}\" differs only in case, and fragment links are case-sensitive, so the link goes nowhere."),
            None => format!("Link to \"{href}\" matches no id in this document, so the link goes nowhere."),
        };
        report(pos.clone(), "broken-fragment-link", message, to_byte, out);
    }
}

/// The ids the rendered HTML carries, read off the output so generated ids and
/// ids inside raw HTML count exactly as a browser sees them.
fn rendered_ids(source: &str, options: &Options<'_>) -> BTreeSet<String> {
    let mut render_options = Options {
        lowercase_heading_ids: options.lowercase_heading_ids,
        ascii_heading_ids: options.ascii_heading_ids,
        ..Options::default()
    };
    // Only id-neutral extensions reach here; rendering with them drops what
    // they drop, such as an unused citation definition.
    render_options.extensions.clone_from(&options.extensions);
    let html = crate::to_html_with_options(source, &render_options);
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    let mut ids = BTreeSet::new();
    // Iterative: raw HTML nesting is not bounded by the renderer's depth cap.
    let mut pending = vec![dom.document.clone()];
    while let Some(node) = pending.pop() {
        for child in node.children.borrow().iter() {
            let NodeData::Element { name, attrs, .. } = &child.data else {
                continue;
            };
            for attr in attrs.borrow().iter() {
                let attr_name = &*attr.name.local;
                if attr_name == "id" || (attr_name == "name" && &*name.local == "a") {
                    ids.insert(attr.value.to_string());
                }
            }
            pending.push(child.clone());
        }
    }
    ids
}

/// `decodeURIComponent`: `None` for a malformed escape or non-UTF-8 result.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn label_key(label: &str) -> String {
    crate::parse::label_key(label)
}

fn ignore(pos: Option<Pos>, lines: &mut BTreeSet<usize>) {
    if let Some(pos) = pos {
        lines.extend(pos.start_line..=pos.end_line);
    }
}

fn report(
    pos: Option<Pos>,
    rule: &'static str,
    message: String,
    to_byte: &dyn Fn(usize) -> usize,
    out: &mut Vec<LintWarning>,
) {
    let (line, column, start, end) = pos
        .map(|p| {
            (
                p.start_line,
                p.start_column,
                to_byte(p.start_offset),
                to_byte(p.end_offset),
            )
        })
        .unwrap_or((1, 1, 0, 0));
    out.push(LintWarning {
        line,
        column,
        start,
        end,
        rule,
        message,
    });
}
