use super::*;

/// Assign `caption_number.number` per label, in document order.
///
/// Called by the parse and by `ast_json::from_json`, which is why it is visible
/// to the crate: a published number has to describe THIS document, and an
/// ingested tree may have had a captioned element removed since it was written
/// (carve#758). Counters are built fresh on every call, so re-running it on an
/// unedited tree reproduces the same numbering.
pub(crate) fn number_crossref_captions(doc: &mut Document) -> BTreeMap<String, String> {
    let mut caption_counts = BTreeMap::new();
    let mut titles = BTreeMap::new();
    number_captioned_blocks(&mut doc.children, &mut caption_counts, &mut titles);
    for blocks in doc.footnote_defs.values_mut() {
        number_captioned_blocks(blocks, &mut caption_counts, &mut titles);
    }
    titles
}

pub(crate) fn crossref_index_for_document(
    doc: &Document,
    id_opts: HeadingIdOptions,
) -> CrossrefIndex {
    let mut counts = BTreeMap::new();
    let mut titles = BTreeMap::new();
    let mut labels = BTreeMap::new();
    let mut explicit_ids = std::collections::BTreeSet::new();
    collect_explicit_ids(&doc.children, &mut explicit_ids);
    for blocks in doc.footnote_defs.values() {
        collect_explicit_ids(blocks, &mut explicit_ids);
    }
    // This index serves `</#id>` crossrefs only, which DO resolve into a
    // blockquote, so the quoted set it fills is discarded.
    let mut quoted = std::collections::BTreeSet::new();
    // This index serves `</#id>` only, which looks up by id, so the text index
    // it fills is discarded with the quoted set above.
    let mut by_text = BTreeMap::new();
    collect_heading_titles(
        &doc.children,
        &mut HeadingScan {
            counts: &mut counts,
            titles: &mut titles,
            labels: &mut labels,
            quoted: &mut quoted,
            by_text: &mut by_text,
            assigned: None,
            collect_crossrefs: true,
            collect_heading_refs: false,
        },
        id_opts,
        &explicit_ids,
        false,
    );
    for blocks in doc.footnote_defs.values() {
        collect_heading_titles(
            blocks,
            &mut HeadingScan {
                counts: &mut counts,
                titles: &mut titles,
                labels: &mut labels,
                quoted: &mut quoted,
                by_text: &mut by_text,
                assigned: None,
                collect_crossrefs: true,
                collect_heading_refs: false,
            },
            id_opts,
            &explicit_ids,
            false,
        );
    }
    collect_caption_titles(&doc.children, &mut titles);
    for blocks in doc.footnote_defs.values() {
        collect_caption_titles(blocks, &mut titles);
    }
    crossref_index(titles, labels)
}

/// The index a collapsed `[text][]` reference resolves against, built from a
/// parsed document.
pub(crate) fn heading_reference_index_for_document(
    doc: &Document,
    id_opts: HeadingIdOptions,
) -> CrossrefIndex {
    heading_index(&doc.children, &doc.footnote_defs, id_opts, true, true).0
}

pub(in crate::parse) fn heading_index(
    children: &[BlockNode],
    footnote_defs: &BTreeMap<String, Vec<BlockNode>>,
    id_opts: HeadingIdOptions,
    collect_crossrefs: bool,
    collect_heading_refs: bool,
) -> (CrossrefIndex, Vec<String>) {
    let used_notes = crate::render::used_footnote_labels(children, footnote_defs);
    let mut counts = BTreeMap::new();
    let mut titles = BTreeMap::new();
    let mut labels = BTreeMap::new();
    let mut explicit_ids = std::collections::BTreeSet::new();
    collect_explicit_ids(children, &mut explicit_ids);
    for (label, blocks) in footnote_defs {
        if used_notes.contains(label) {
            collect_explicit_ids(blocks, &mut explicit_ids);
        }
    }
    let mut quoted = std::collections::BTreeSet::new();
    let mut by_text = BTreeMap::new();
    let mut assigned = Vec::new();
    collect_heading_titles(
        children,
        &mut HeadingScan {
            counts: &mut counts,
            titles: &mut titles,
            labels: &mut labels,
            quoted: &mut quoted,
            by_text: &mut by_text,
            assigned: Some(&mut assigned),
            collect_crossrefs,
            collect_heading_refs,
        },
        id_opts,
        &explicit_ids,
        false,
    );
    for (label, blocks) in footnote_defs {
        if !used_notes.contains(label) {
            continue;
        }
        collect_heading_titles(
            blocks,
            &mut HeadingScan {
                counts: &mut counts,
                titles: &mut titles,
                labels: &mut labels,
                quoted: &mut quoted,
                by_text: &mut by_text,
                assigned: Some(&mut assigned),
                collect_crossrefs,
                collect_heading_refs,
            },
            id_opts,
            &explicit_ids,
            false,
        );
    }
    let mut index = crossref_index(titles, labels);
    index.quoted = quoted;
    index.by_text = by_text;
    (index, assigned)
}

/// A cloned crossref label, shared by every reference that resolves to it.
///
/// `Rc` rather than a per-reference clone: a document may reference one heading
/// any number of times, and the label is immutable once captured.
pub(crate) type CrossrefLabel = std::rc::Rc<Vec<InlineNode>>;

/// Capture a heading's label for the crossref index: a CLONE OF ITS INLINE
/// NODES, not of its rendered text (PART 9R R4, carve#915).
///
/// The difference is the whole point of the clause. A node carries the SOURCE
/// RUN the author typed; a string carries only the glyphs some renderer chose.
/// Flattening here would destroy the run before any renderer is invoked, so
/// smart typography's SOURCE mode (§8) could not recover it on ANY target and
/// no renderer change would reach the loss -- the label would have been
/// materialized in the wrong subsystem. The same argument covers every other
/// run a renderer may want back: a code span, an escape, an inline literal.
///
/// Two transformations are applied to the clone, both of them rules that hold
/// wherever this label is about to be placed rather than presentation choices:
///
/// - A NESTED cross-reference becomes empty text. Resolution is ONE LEVEL, so a
///   cloned label is never re-expanded; doing it here (rather than at render
///   time) also makes a crossref cycle structurally impossible to follow.
/// - Links never nest, so the clone is cleaned as if it already sat inside an
///   anchor -- which it does on every target that emits one.
///
/// A footnote reference is dropped, which is what the previous flattening did
/// too: the label renders inside the referring paragraph, and a second copy of
/// a `fnref` anchor would publish a duplicate id.
pub(in crate::parse) fn crossref_label_nodes(children: &[InlineNode]) -> CrossrefLabel {
    std::rc::Rc::new(crossref_label_clone(children))
}

/// The core crossref's label: [`derive_display_nodes`] in a link context, since
/// a resolved reference always renders inside the anchor it produces.
pub(crate) fn crossref_label_clone(children: &[InlineNode]) -> Vec<InlineNode> {
    derive_display_nodes(children, true)
}

/// The one derivation every consumer of a heading's display text goes through
/// (PART 9R R4, DERIVED DISPLAY TEXT CLONES THE SAME NODES,
/// markup-carve/carve#957).
///
/// R4 binds every such consumer, not the core crossref alone, and names three: a
/// numbered cross-reference label, an index term's display, a table-of-contents
/// entry. Each answering the follow-on questions on its own is how one rule
/// acquires four readings, so they all call this.
///
/// `inside_link` is the CALLER's context rather than a fact about `children`: a
/// crossref label and a TOC entry are placed inside an `<a>` and pass `true`; an
/// index list item is not an anchor (only the backrefs after the display are) and
/// passes `false`, so an authored link in the term survives.
pub(crate) fn derive_display_nodes(children: &[InlineNode], inside_link: bool) -> Vec<InlineNode> {
    let mut nodes = children.to_vec();
    strip_non_authored(&mut nodes);
    flatten_nested_crossrefs(&mut nodes);
    enforce_no_nesting_inline(nodes, inside_link)
}

/// Reduce a cloned run to what the AUTHOR wrote.
///
/// THE LABEL IS TAKEN BEFORE ANY RENDER-STAGE INJECTION (PART 9R R4). This
/// engine builds the crossref index at RENDER time, after every `before_render`
/// hook has run, so the injections are already in the heading and the pristine
/// reading has to be recovered here rather than obtained by ordering.
///
/// What comes out, and what leaving it in would cost:
///
/// - A `section-number` SPAN and a PERMALINK ANCHOR, both named by R4. The
///   anchor renders an `<a>` inside the resolved `</#id>`'s own `<a>`.
/// - A FOOTNOTE REFERENCE and a CITATION GROUP: pointers rather than display
///   text, so a second copy publishes a duplicate id and steals the backlink.
///   The author's raw `[@key]` run goes back in the citation's place.
/// - An `:index[term]` MARKER - the COUNTED CARRIER only, which is invisible by
///   §8.1. The AUTHORED `index` extension stays: with the extension off it
///   degrades to visible text (§8.3), so dropping it would lose authored text.
/// - An ABBREVIATION (R3). Cloning the resolved node republishes the whole
///   `<abbr title="…">` per derived site, past the budget the body renderer
///   bounds it with; the author wrote the short form.
pub(in crate::parse) fn strip_non_authored(nodes: &mut Vec<InlineNode>) {
    nodes.retain(|node| match node {
        InlineNode::Footnote(_) => false,
        InlineNode::RawInline(r) => !r.injected,
        InlineNode::Span(s) => !s.injected,
        InlineNode::Extension(e) => e.name != INDEX_MARKER_CARRIER,
        _ => true,
    });
    for node in nodes.iter_mut() {
        match node {
            InlineNode::Abbreviation(a) => {
                *node = InlineNode::Text(Text {
                    value: std::mem::take(&mut a.abbr),
                    pos: a.pos.clone(),
                });
                continue;
            }
            InlineNode::CitationGroup(g) => {
                *node = InlineNode::Text(Text {
                    value: std::mem::take(&mut g.raw),
                    pos: g.pos.clone(),
                });
                continue;
            }
            _ => {}
        }
        match node {
            InlineNode::Emphasis(e) => strip_non_authored(&mut e.children),
            InlineNode::Span(s) => strip_non_authored(&mut s.children),
            InlineNode::Link(l) => strip_non_authored(&mut l.children),
            InlineNode::Extension(e) => strip_non_authored(&mut e.children),
            InlineNode::CriticInsert(c) => strip_non_authored(&mut c.children),
            InlineNode::CriticDelete(c) => strip_non_authored(&mut c.children),
            InlineNode::CriticSubstitute(c) => {
                strip_non_authored(&mut c.old);
                strip_non_authored(&mut c.new);
            }
            _ => {}
        }
    }
}

/// Class on the `section-number` span `headingNumbers` injects into a heading.
/// The strip does NOT key on it (see [`Span::injected`]); it is named once, and
/// the extension reads it from here so the class and its documentation cannot
/// drift apart.
pub(crate) const SECTION_NUMBER_CLASS: &str = "section-number";
/// The counted carrier `index` rewrites a body marker to in `before_render`.
pub(crate) const INDEX_MARKER_CARRIER: &str = "carve-index-marker";

/// Replace every `</#id>` inside a cloned label with empty text. Resolution is
/// ONE LEVEL, so a cloned label is never re-expanded; doing it here rather than
/// at render time also makes a crossref cycle structurally impossible to follow.
pub(in crate::parse) fn flatten_nested_crossrefs(nodes: &mut [InlineNode]) {
    for node in nodes.iter_mut() {
        match node {
            InlineNode::CrossRef(_) => {
                *node = InlineNode::Text(Text {
                    value: String::new(),
                    pos: None,
                })
            }
            InlineNode::Emphasis(e) => flatten_nested_crossrefs(&mut e.children),
            InlineNode::Span(s) => flatten_nested_crossrefs(&mut s.children),
            InlineNode::Link(l) => flatten_nested_crossrefs(&mut l.children),
            InlineNode::Extension(e) => flatten_nested_crossrefs(&mut e.children),
            InlineNode::CriticInsert(c) => flatten_nested_crossrefs(&mut c.children),
            InlineNode::CriticDelete(c) => flatten_nested_crossrefs(&mut c.children),
            InlineNode::CriticSubstitute(c) => {
                flatten_nested_crossrefs(&mut c.old);
                flatten_nested_crossrefs(&mut c.new);
            }
            _ => {}
        }
    }
}

pub(in crate::parse) fn crossref_index(
    titles: BTreeMap<String, String>,
    labels: BTreeMap<String, CrossrefLabel>,
) -> CrossrefIndex {
    CrossrefIndex {
        titles,
        labels,
        by_text: BTreeMap::new(),
        quoted: std::collections::BTreeSet::new(),
    }
}

/// Heading-id lookup table for `</#id>` cross-references: exact id -> title.
/// Every lookup compares case exactly (PART 9R R1, `CARVE-P9R-010`).
#[derive(Default)]
pub(crate) struct CrossrefIndex {
    titles: BTreeMap<String, String>,
    /// Id -> the label a resolved reference RENDERS: the target heading's own
    /// inline nodes, cloned (PART 9R R4). `titles` is the same label flattened
    /// to text, which is all a slug or a text lookup needs; a renderer needs
    /// the nodes, because only they still carry what the author typed.
    ///
    /// A CAPTION id has a title but no entry here: its label is LABEL + NUMBER
    /// ("Figure 1"), text that no node of the document ever held.
    labels: BTreeMap<String, CrossrefLabel>,
    /// Normalized heading TEXT -> that heading's id, in document order (first
    /// occurrence wins). PART 11 R1 keys the implicit `[label][]` index by "the
    /// document's headings keyed by their rendered plain text"; looking the
    /// label up among the IDS agrees with that only while the id is the slug of
    /// the text, and stops the moment an author sets one explicitly
    /// (carve-rs#477).
    by_text: BTreeMap<String, String>,
    /// Ids belonging to a heading with a blockquote ancestor. They resolve as
    /// `</#id>` crossrefs like any other, and are DECLINED as implicit
    /// `[label][]` reference targets (PART 11 R1).
    quoted: std::collections::BTreeSet<String>,
}

impl CrossrefIndex {
    pub(in crate::parse) fn extend_titles(&mut self, titles: BTreeMap<String, String>) {
        for (id, title) in titles {
            self.titles.entry(id).or_insert(title);
        }
    }
    /// Resolve for an IMPLICIT `[label][]` reference, which declines a heading
    /// with a blockquote ancestor (PART 11 R1). Quoted text names the quoted
    /// document's headings, not this one's, and a quotation is the one
    /// container whose wording the author does not control. Declining does not
    /// make the heading unreachable: `</#id>` still addresses it, by id rather
    /// than by wording.
    pub(crate) fn resolve_ref(&self, target: &str) -> Option<(&str, &str)> {
        // R1 matches the heading TEXT only (whitespace collapsed, NFC, exact
        // case). Slugging the label would let id transforms fold it.
        let id = self.by_text.get(&normalize_heading_label(target))?;
        if self.quoted.contains(id) {
            return None;
        }
        let (id, title) = self.titles.get_key_value(id)?;
        Some((id.as_str(), title.as_str()))
    }

    /// Whether R1'S TEXT INDEX answers for `target`, which is `resolve_ref`'s
    /// only key.
    ///
    /// A collapsed reference has to publish WHICH key answered (PART 12 §3a,
    /// markup-carve/carve#962), and `resolve_ref` cannot say: it returns the
    /// heading either way. The conditions are the ones `resolve_ref` applies,
    /// and are kept beside it for that reason.
    pub(crate) fn answers_by_text(&self, target: &str) -> bool {
        self.by_text
            .get(&normalize_heading_label(target))
            .is_some_and(|id| !self.quoted.contains(id) && self.titles.contains_key(id))
    }

    /// Every id a `</#id>` cross-reference can reach.
    pub(crate) fn ids(&self) -> impl Iterator<Item = &str> {
        self.titles.keys().map(String::as_str)
    }

    /// Every heading text a collapsed `[text][]` reference can reach, keyed the
    /// way R1 compares them (whitespace collapsed, NFC).
    pub(crate) fn heading_texts(&self) -> impl Iterator<Item = &str> {
        self.by_text
            .iter()
            .filter(|(_, id)| !self.quoted.contains(*id) && self.titles.contains_key(*id))
            .map(|(text, _)| text.as_str())
    }

    /// The cloned inline nodes a resolved reference to `id` renders, when the
    /// target is a HEADING. `None` for a caption id, whose label is
    /// LABEL + NUMBER rather than any node of the document; the caller falls
    /// back to `resolve`'s title for that.
    pub(crate) fn label(&self, id: &str) -> Option<CrossrefLabel> {
        self.labels.get(id).cloned()
    }

    /// Resolve a cross-reference target to its `(id, title)`, matching case
    /// exactly.
    pub(crate) fn resolve(&self, target: &str) -> Option<(&str, &str)> {
        let (id, title) = self.titles.get_key_value(target)?;
        Some((id.as_str(), title.as_str()))
    }
}

/// The comparison PART 11 R1 specifies for the implicit heading fallback: the
/// label and the heading text are both trimmed, their internal whitespace runs
/// collapsed to one space, NFC-normalized, and then compared with exact case.
pub(crate) fn normalize_heading_label(s: &str) -> String {
    s.split(' ')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .nfc()
        .collect()
}

/// Promote a paragraph whose sole child is a direct or resolved image to a
/// block-level image, matching the standalone inline-image rule
/// (`detect_block_image`) and carve-php. Recurses into container blocks.
/// Length (in bytes) of a leading `^` + one-or-more whitespace caption marker
/// (`RE_CAPTION = /^\^\s+/`), or `None` when the text does not open a caption.
/// A caption line mirrors a heading's first line (`detect_heading`): `^` +
/// one-or-more literal spaces (the grammar delimiter is a space, not a tab) +
/// non-empty content. Returns the caption text with leading spaces skipped and
/// trailing whitespace trimmed. None when there is no space after `^`, the
/// delimiter is a tab, or the content is empty (`^ ` alone).
pub(in crate::parse) fn caption_content(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    if bytes.first() != Some(&b'^') || bytes.get(1) != Some(&b' ') {
        return None;
    }
    let mut start = 1;
    while start < bytes.len() && bytes[start] == b' ' {
        start += 1;
    }
    // Verbatim content (see detect_heading): a caption folds continuation lines
    // like a paragraph, so first-line trailing is interior; only the final
    // assembled caption is trailing-stripped (§756). The gate still tests a
    // trailing-stripped view so `^ ` / `^  ` / `^ \t` are not captions.
    let text = &line[start..];
    if trim_ascii_end(text).is_empty() {
        return None;
    }
    Some(text)
}

/// Byte length of a caption marker (`^` + one-or-more spaces) at the START of an
/// inline Text node, used when splitting a reference-image figure caption off
/// its leading text. Mirrors `caption_content`'s delimiter: a space, not a tab.
/// Content-emptiness is decided separately (`caption_first_line_has_content`),
/// because the caption's content may live in a following inline node (`^ *b*`,
/// where the marker node is just `"^ "` and `*b*` is an Emphasis sibling).
pub(in crate::parse) fn caption_marker_len(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.first() != Some(&b'^') || bytes.get(1) != Some(&b' ') {
        return None;
    }
    let mut n = 1;
    while n < bytes.len() && bytes[n] == b' ' {
        n += 1;
    }
    Some(n)
}

/// Whether a string carries caption content: at least one non-ASCII-whitespace
/// byte. A non-breaking space (U+00A0) and any other non-ASCII byte count as
/// content, matching the direct-caption path (`caption_content` trims only
/// ASCII whitespace) and carve-php's byte-mode `\S`. `str::trim` is
/// Unicode-aware and would wrongly drop NBSP, so test bytes directly.
pub(in crate::parse) fn has_caption_content(s: &str) -> bool {
    s.bytes().any(|b| !b.is_ascii_whitespace())
}

/// Whether a `[Image, SoftBreak, "^ …", …]` paragraph's caption carries any
/// content on its FIRST line: text after the `^ ` marker on the marker node, or
/// any following inline node before the first soft break. Rejects an empty
/// first-line caption (`^ ` with content only on later folded lines, or none).
pub(in crate::parse) fn caption_first_line_has_content(children: &[InlineNode]) -> bool {
    if let InlineNode::Text(t) = &children[2] {
        if let Some(n) = caption_marker_len(&t.value) {
            if has_caption_content(&t.value[n..]) {
                return true;
            }
        }
    }
    for child in &children[3..] {
        match child {
            InlineNode::SoftBreak(_) => break,
            InlineNode::Text(t) if !has_caption_content(&t.value) => continue,
            _ => return true,
        }
    }
    false
}
