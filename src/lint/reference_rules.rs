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
    let targets = CaseTargets::new(doc, options);
    let mut fragment_links = Vec::new();
    while let Some((inline, depth)) = inlines.pop() {
        if let Some(pos) = inline.pos() {
            inline_spans.push((to_byte(pos.start_offset), to_byte(pos.end_offset)));
        }
        match inline {
            InlineNode::CrossRef(reference) if reference.href.is_none() => {
                let target = &reference.target;
                let case_only = targets.crossref(target);
                let message = match id_kinds.get(target) {
                    Some((id, kind)) => format!(
                        "Cross-reference </#{target}> names the id \"{id}\", which is on a {kind}; a cross-reference reaches only headings and numbered captions, so it renders as the literal text \"</#{target}>\". Link to it with [text](#{id})."
                    ),
                    None if !case_only.is_empty() => format!(
                        "Cross-reference </#{target}> matches no id; {}, and cross-references are case-sensitive, so it renders as the literal text \"</#{target}>\".",
                        differ_only_in_case(&case_only)
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
            InlineNode::Link(link) if link.ref_label.is_some() && link.href.is_empty() => {
                let case_only = targets.reference(link);
                let message = if case_only.is_empty() {
                    "Reference link has no matching definition or heading.".into()
                } else {
                    format!(
                        "Reference link {} matches no definition or heading; {}, and reference labels are case-sensitive, so it renders as literal text.",
                        link.raw_ref.as_deref().unwrap_or_default(),
                        differ_only_in_case(&case_only)
                    )
                };
                report(
                    link.pos.clone(),
                    "unresolved-reference-link",
                    message,
                    to_byte,
                    out,
                );
            }
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

/// Every lookup compares case exactly; this fold only finds the near misses a
/// diagnostic or `fmt --migrate` names.
fn fold_id(id: &str) -> String {
    id.chars().flat_map(char::to_lowercase).collect()
}

/// A target a reference misses only by case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CaseMatch {
    Id(String),
    Label(String),
    Heading(String),
}

impl CaseMatch {
    pub(super) fn spelling(&self) -> &str {
        match self {
            Self::Id(s) | Self::Label(s) | Self::Heading(s) => s,
        }
    }
}

fn differ_only_in_case(matches: &[CaseMatch]) -> String {
    let noun = |m: &CaseMatch| match m {
        CaseMatch::Id(_) => "id",
        CaseMatch::Label(_) => "label",
        CaseMatch::Heading(_) => "heading",
    };
    let quoted = |m: &CaseMatch| format!("\"{}\"", m.spelling());
    match matches {
        [only] => format!("the {} {} differs only in case", noun(only), quoted(only)),
        [first, rest @ ..] if rest.iter().all(|m| noun(m) == noun(first)) => format!(
            "the {}s {} differ only in case",
            noun(first),
            matches.iter().map(quoted).collect::<Vec<_>>().join(" and ")
        ),
        _ => format!(
            "{} differ only in case",
            matches
                .iter()
                .map(|m| format!("the {} {}", noun(m), quoted(m)))
                .collect::<Vec<_>>()
                .join(" and ")
        ),
    }
}

/// The targets a reference can reach, for finding the ones it misses by case.
pub(super) struct CaseTargets {
    crossref_ids: Vec<String>,
    labels: Vec<String>,
    headings: Vec<String>,
}

impl CaseTargets {
    pub(super) fn new(doc: &Document, options: &Options<'_>) -> Self {
        let id_opts = options.heading_id_options();
        let crossref_ids = crate::parse::crossref_index_for_document(doc, id_opts)
            .ids()
            .map(str::to_owned)
            .collect();
        let headings = crate::parse::heading_reference_index_for_document(doc, id_opts)
            .heading_texts()
            .map(str::to_owned)
            .collect();
        let mut labels = BTreeSet::new();
        let mut blocks: Vec<_> = doc.children.iter().map(|b| (b, 0)).collect();
        for body in doc.footnote_defs.values() {
            blocks.extend(body.iter().map(|b| (b, 0)));
        }
        let mut inlines = Vec::new();
        while let Some((block, depth)) = blocks.pop() {
            if let BlockNode::LinkReferenceDefinition(def) = block {
                labels.insert(label_key(&def.label));
            }
            push_block_children(block, depth, &mut blocks, &mut inlines);
        }
        Self {
            crossref_ids,
            labels: labels.into_iter().collect(),
            headings,
        }
    }

    /// Ids a `</#target>` misses only by case.
    pub(super) fn crossref(&self, target: &str) -> Vec<CaseMatch> {
        let folded = fold_id(target);
        self.crossref_ids
            .iter()
            .filter(|id| *id != target && fold_id(id) == folded)
            .map(|id| CaseMatch::Id(id.clone()))
            .collect()
    }

    /// Definition labels, and for a collapsed `[text][]` heading texts, that an
    /// unresolved reference link misses only by case.
    pub(super) fn reference(&self, link: &Link) -> Vec<CaseMatch> {
        let Some(label) = link.ref_label.as_deref() else {
            return Vec::new();
        };
        let key = label_key(label);
        let folded = fold_id(&key);
        let mut found: Vec<_> = self
            .labels
            .iter()
            .filter(|l| **l != key && fold_id(l) == folded)
            .map(|l| CaseMatch::Label(l.clone()))
            .collect();
        if crate::parse::is_collapsed_reference(link) {
            let text = crate::parse::normalize_heading_label(label);
            let folded = fold_id(&text);
            found.extend(
                self.headings
                    .iter()
                    .filter(|h| **h != text && fold_id(h) == folded)
                    .map(|h| CaseMatch::Heading(h.clone())),
            );
        }
        found
    }

    /// Definition labels an unresolved reference image misses only by case. An
    /// image never resolves against the heading index, so headings are not
    /// candidates.
    fn image_reference(&self, label: &str) -> Vec<CaseMatch> {
        let key = label_key(label);
        let folded = fold_id(&key);
        self.labels
            .iter()
            .filter(|l| **l != key && fold_id(l) == folded)
            .map(|l| CaseMatch::Label(l.clone()))
            .collect()
    }
}

/// The source edits `fmt --migrate` makes: each reference whose ONE
/// case-insensitive match is spelled differently takes that spelling. A
/// reference with several such matches is left for lint to report.
pub(super) fn case_only_edits(
    source: &str,
    doc: &Document,
    options: &Options<'_>,
    to_byte: &dyn Fn(usize) -> usize,
) -> Vec<(usize, usize, String)> {
    let targets = CaseTargets::new(doc, options);
    let mut blocks: Vec<_> = doc.children.iter().map(|b| (b, 0)).collect();
    for body in doc.footnote_defs.values() {
        blocks.extend(body.iter().map(|b| (b, 0)));
    }
    let mut inlines = Vec::new();
    while let Some((block, depth)) = blocks.pop() {
        push_block_children(block, depth, &mut blocks, &mut inlines);
    }
    let mut edits = Vec::new();
    while let Some((inline, depth)) = inlines.pop() {
        // (pos, the source the node spans, its name part, the rewrite, the target)
        let edit = match inline {
            InlineNode::CrossRef(reference) if reference.href.is_none() => {
                let target = reference.target.as_str();
                match targets.crossref(target).as_slice() {
                    [only] => Some((
                        reference.pos.as_ref(),
                        format!("</#{target}>"),
                        target,
                        format!("</#{}>", only.spelling()),
                        only.spelling().to_owned(),
                    )),
                    _ => None,
                }
            }
            InlineNode::Link(link) if link.ref_label.is_some() && link.href.is_empty() => {
                let label = link.ref_label.as_deref().unwrap_or_default();
                let raw = link.raw_ref.clone().unwrap_or_default();
                // `raw_ref` may end in a trailing attribute block, which stays.
                let rewrite = |needle: String, replacement: String| {
                    let at = if crate::parse::is_collapsed_reference(link) {
                        raw.starts_with(&needle).then_some(0)
                    } else {
                        raw.rfind(&needle)
                    }?;
                    let rest = &raw[at + needle.len()..];
                    (rest.is_empty() || rest.starts_with('{'))
                        .then(|| format!("{}{replacement}{rest}", &raw[..at]))
                };
                match targets.reference(link).as_slice() {
                    [only] => {
                        let spelling = only.spelling();
                        let new = if crate::parse::is_collapsed_reference(link) {
                            rewrite(format!("[{label}][]"), format!("[{spelling}][]"))
                        } else {
                            rewrite(format!("][{label}]"), format!("][{spelling}]"))
                        };
                        new.map(|new| {
                            (
                                link.pos.as_ref(),
                                raw.clone(),
                                label,
                                new,
                                spelling.to_owned(),
                            )
                        })
                    }
                    _ => None,
                }
            }
            // A multiline label misses for a reason other than case.
            InlineNode::Image(image)
                if image.src.is_empty()
                    && image
                        .ref_label
                        .as_deref()
                        .is_some_and(|label| !label.contains(['\r', '\n'])) =>
            {
                let label = image.ref_label.as_deref().unwrap_or_default();
                let raw = image.raw_ref.clone().unwrap_or_default();
                match targets.image_reference(label).as_slice() {
                    [only] => {
                        let spelling = only.spelling();
                        // Only a plain alt proves the bracket is the reference's
                        // own; a trailing attribute block stays.
                        let collapsed = format!("![{label}][]");
                        let explicit = format!("![{}][{label}]", image.alt);
                        let head = if label == image.alt && raw.starts_with(&collapsed) {
                            Some((collapsed, format!("![{spelling}][]")))
                        } else if raw.starts_with(&explicit) {
                            Some((explicit, format!("![{}][{spelling}]", image.alt)))
                        } else {
                            None
                        };
                        head.and_then(|(head, new_head)| {
                            let rest = &raw[head.len()..];
                            (rest.is_empty() || rest.starts_with('{')).then(|| {
                                (
                                    image.pos.as_ref(),
                                    raw.clone(),
                                    label,
                                    format!("{new_head}{rest}"),
                                    spelling.to_owned(),
                                )
                            })
                        })
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        // Only a case change (plus the whitespace and NFC normalization lookup
        // applies) is safe to write back: it cannot open new markup.
        if let Some((Some(pos), old, name, new, spelling)) = edit {
            let (start, end) = (to_byte(pos.start_offset), to_byte(pos.end_offset));
            if source.get(start..end) == Some(old.as_str())
                && fold_id(&crate::parse::normalize_heading_label(name))
                    == fold_id(&crate::parse::normalize_heading_label(&spelling))
            {
                edits.push((start, end, new));
            }
        }
        push_inline_children(inline, depth, &mut inlines);
    }
    edits
}

/// The first element carrying each id, named by node kind.
fn explicit_id_kinds(doc: &Document) -> BTreeMap<String, (String, String)> {
    let mut kinds = BTreeMap::new();
    let mut visit = |node_type: &'static str, attrs: &Attrs, _: Option<Pos>| {
        if let Some(id) = &attrs.id {
            kinds
                .entry(id.clone())
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
    let (ids, linked) = rendered_ids(source, options);
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
        // A link the render leaves out reaches no reader, so it goes nowhere for
        // a reason this rule does not own.
        if !linked.contains(fragment) && !linked.contains(&decoded) {
            continue;
        }
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
/// ids inside raw HTML count exactly as a browser sees them, and the fragments
/// of the links that render.
fn rendered_ids(source: &str, options: &Options<'_>) -> (BTreeSet<String>, BTreeSet<String>) {
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
    let mut linked = BTreeSet::new();
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
                if attr_name == "href" && &*name.local == "a" {
                    if let Some(rest) = attr.value.strip_prefix('#') {
                        let fragment = rest.split(":~:").next().unwrap_or_default();
                        linked.insert(fragment.to_owned());
                        if let Some(decoded) = percent_decode(fragment) {
                            linked.insert(decoded);
                        }
                    }
                }
            }
            pending.push(child.clone());
        }
    }
    (ids, linked)
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
