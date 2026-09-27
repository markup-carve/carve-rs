use std::collections::{BTreeMap, BTreeSet};

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
    while let Some((inline, depth)) = inlines.pop() {
        if let Some(pos) = inline.pos() {
            inline_spans.push((to_byte(pos.start_offset), to_byte(pos.end_offset)));
        }
        match inline {
            InlineNode::CrossRef(reference) if reference.href.is_none() => report(
                reference.pos.clone(),
                "broken-crossref",
                format!(
                    "Cross-reference </#{}> has no matching heading id.",
                    reference.target
                ),
                to_byte,
                out,
            ),
            InlineNode::Link(link) if link.ref_label.is_some() && link.href.is_empty() => report(
                link.pos.clone(),
                "unresolved-reference-link",
                "Reference link has no matching definition or heading.".into(),
                to_byte,
                out,
            ),
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
