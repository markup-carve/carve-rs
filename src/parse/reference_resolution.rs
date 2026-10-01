use super::{is_collapsed_reference, merge_attrs, plain_inlines_parse, CrossrefIndex};
use crate::ast::{Attrs, BlockNode, Document, FigureTarget, Image, InlineNode, Link};
use std::cell::RefCell;
use std::collections::BTreeMap;

#[derive(Clone)]
pub(super) struct LinkDef {
    pub(super) raw_label: Option<String>,
    pub(super) href: String,
    pub(super) title: Option<String>,
    /// Zero-based index of the line the definition was written on. Kept so PART
    /// 12 §10's node can carry a `pos`, and so the hoisted definitions come out
    /// in SOURCE order rather than the label order of the map (carve-rs#631).
    /// `None` for a definition that did not come from a source line.
    pub(super) line: Option<usize>,
    /// A TRAILING attribute block on the definition line. PART 9R's symbol
    /// table is `label -> (url, title?, attrs?)`, and R1 transfers these to
    /// every link that resolves the label (carve#604).
    pub(super) attrs: Option<Attrs>,
}

pub(crate) fn label_key(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut pending_space = false;
    for ch in label.chars() {
        if matches!(ch, ' ' | '\t' | '\n' | '\u{000c}' | '\r') {
            pending_space = !out.is_empty();
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            out.push(ch);
        }
    }
    out
}

/// Borrow labels whose spelling is already the normalized map key.
fn borrowed_label_key(label: &str) -> std::borrow::Cow<'_, str> {
    let mut previous_space = true;
    for ch in label.chars() {
        if matches!(ch, '\t' | '\n' | '\u{000c}' | '\r') || (ch == ' ' && previous_space) {
            return std::borrow::Cow::Owned(label_key(label));
        }
        previous_space = ch == ' ';
    }
    if previous_space && !label.is_empty() {
        std::borrow::Cow::Owned(label_key(label))
    } else {
        std::borrow::Cow::Borrowed(label)
    }
}

#[cfg(test)]
mod borrowed_reference_keys {
    use super::*;

    #[test]
    fn borrowed_keys_match_owned_normalization() {
        for label in [
            "",
            "r0",
            "a b",
            "😀",
            "a\u{a0}b",
            " leading",
            "trailing ",
            "a  b",
            "a\tb",
            "a\nb",
            "a\rb",
            "a\u{c}b",
        ] {
            let key = borrowed_label_key(label);
            assert_eq!(key.as_ref(), label_key(label));
            if matches!(label, "" | "r0" | "a b" | "😀" | "a\u{a0}b") {
                assert!(matches!(key, std::borrow::Cow::Borrowed(_)));
            }
        }
    }
}

fn is_single_line_label(label: &str) -> bool {
    !label.contains(['\r', '\n'])
}

struct ActiveLinkDefs {
    defs: BTreeMap<String, LinkDef>,
    needs_late_resolution: bool,
}

thread_local! {
    /// Definition lookup available while inline nodes are constructed.
    ///
    /// A stack keeps nested/re-entrant parses isolated. Moving the map into the
    /// context avoids cloning it; the completed parse takes it back out.
    static ACTIVE_LINK_DEFS: RefCell<Vec<ActiveLinkDefs>> = const { RefCell::new(Vec::new()) };
}

/// A definition the streaming scanner can render without attribute handling.
pub(super) fn active_plain_link_destination(reference: &str) -> Option<(String, Option<String>)> {
    ACTIVE_LINK_DEFS.with(|active| {
        let active = active.borrow();
        let def = active
            .last()?
            .defs
            .get(borrowed_label_key(reference).as_ref())?;
        if def.attrs.is_some() {
            return None;
        }
        Some((def.href.clone(), def.title.clone()))
    })
}

pub(super) fn with_active_link_defs<T>(
    defs: BTreeMap<String, LinkDef>,
    parse: impl FnOnce() -> T,
) -> (T, BTreeMap<String, LinkDef>, bool) {
    ACTIVE_LINK_DEFS.with(|active| {
        active.borrow_mut().push(ActiveLinkDefs {
            defs,
            needs_late_resolution: false,
        });
    });
    struct ActiveLinkDefsGuard(bool);

    impl Drop for ActiveLinkDefsGuard {
        fn drop(&mut self) {
            if self.0 {
                ACTIVE_LINK_DEFS.with(|active| {
                    active.borrow_mut().pop();
                });
            }
        }
    }

    // Parsing may be called from user extension code. Keep the thread-local
    // stack balanced even if that code unwinds through us.
    let mut guard = ActiveLinkDefsGuard(true);
    let parsed = parse();
    let active = ACTIVE_LINK_DEFS.with(|active| {
        active
            .borrow_mut()
            .pop()
            .expect("a definition context was installed for this parse")
    });
    guard.0 = false;
    (parsed, active.defs, active.needs_late_resolution)
}

/// A reference is RECORDED here and resolved after the parse, never during it.
///
/// Resolving while the inline is built is what forces every definition to be
/// known before parsing starts, which is what the line-based definition
/// pre-passes exist to arrange - and they cannot answer "is a paragraph open"
/// without asking the block parser, which is quadratic. Deferring is the first
/// step of removing them: `resolve_reference_links` already resolves the whole
/// tree with the same `apply_link_def`, and a collapsed reference already had
/// to wait for the heading index, so the late walk was never optional.
pub(super) fn resolve_active_link(link: &mut Link) {
    if link.ref_label.is_none() {
        return;
    }
    ACTIVE_LINK_DEFS.with(|active| {
        if let Some(context) = active.borrow_mut().last_mut() {
            context.needs_late_resolution = true;
        }
    });
}

/// Recorded, not resolved - see [`resolve_active_link`]. An image has no
/// heading fallback, so before deferral it needed no late walk; it needs one now
/// for the same reason a link does.
pub(super) fn resolve_active_image(image: &mut Image) {
    if image.ref_label.is_none() {
        return;
    }
    ACTIVE_LINK_DEFS.with(|active| {
        if let Some(context) = active.borrow_mut().last_mut() {
            context.needs_late_resolution = true;
        }
    });
}

fn apply_link_def(link: &mut Link, def: &LinkDef) {
    link.href.clone_from(&def.href);
    link.title.clone_from(&def.title);
    if let Some(def_attrs) = &def.attrs {
        let own = link.attrs.take();
        let mut merged = Some(def_attrs.clone());
        if let Some(own) = own {
            merge_attrs(&mut merged, own);
        }
        link.attrs = merged;
    }
}

fn apply_image_def(image: &mut Image, def: &LinkDef) {
    image.src.clone_from(&def.href);
    image.title.clone_from(&def.title);
    if let Some(def_attrs) = &def.attrs {
        let own = image.attrs.take();
        let mut merged = Some(def_attrs.clone());
        if let Some(own) = own {
            merge_attrs(&mut merged, own);
        }
        image.attrs = merged;
    }
}

pub(super) fn resolve_reference_links(
    doc: &mut Document,
    defs: &BTreeMap<String, LinkDef>,
    heading_index: &CrossrefIndex,
) {
    for block in &mut doc.children {
        resolve_reference_links_block(block, defs, heading_index);
    }
    for blocks in doc.footnote_defs.values_mut() {
        for block in blocks {
            resolve_reference_links_block(block, defs, heading_index);
        }
    }
}

fn resolve_reference_links_block(
    block: &mut BlockNode,
    defs: &BTreeMap<String, LinkDef>,
    heading_index: &CrossrefIndex,
) {
    match block {
        BlockNode::Heading(h) => {
            resolve_reference_links_inline(&mut h.children, defs, heading_index)
        }
        BlockNode::Paragraph(p) => {
            resolve_reference_links_inline(&mut p.children, defs, heading_index)
        }
        BlockNode::List(l) => {
            for item in &mut l.items {
                for child in &mut item.children {
                    resolve_reference_links_block(child, defs, heading_index);
                }
            }
        }
        BlockNode::BlockQuote(b) => {
            for child in &mut b.children {
                resolve_reference_links_block(child, defs, heading_index);
            }
        }
        BlockNode::LineBlock(b) => {
            for child in &mut b.children {
                resolve_reference_links_block(child, defs, heading_index);
            }
        }
        BlockNode::Table(t) => {
            if let Some(caption) = &mut t.caption {
                resolve_reference_links_inline(caption, defs, heading_index);
            }
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    resolve_reference_links_inline(&mut cell.children, defs, heading_index);
                }
            }
        }
        BlockNode::Admonition(a) => {
            for child in &mut a.children {
                resolve_reference_links_block(child, defs, heading_index);
            }
        }
        BlockNode::FigureGroup(g) => {
            for child in &mut g.children {
                resolve_reference_links_block(child, defs, heading_index);
            }
            if let Some(caption) = &mut g.caption {
                resolve_reference_links_inline(caption, defs, heading_index);
            }
        }
        BlockNode::Directive(d) => {
            for child in &mut d.children {
                resolve_reference_links_block(child, defs, heading_index);
            }
        }
        BlockNode::Div(d) => {
            for child in &mut d.children {
                resolve_reference_links_block(child, defs, heading_index);
            }
        }
        BlockNode::DefinitionList(d) => {
            for item in &mut d.items {
                for term in &mut item.terms {
                    resolve_reference_links_inline(term, defs, heading_index);
                }
                for definition in &mut item.definitions {
                    for child in definition {
                        resolve_reference_links_block(child, defs, heading_index);
                    }
                }
            }
        }
        BlockNode::Figure(f) => {
            resolve_reference_links_inline(&mut f.caption, defs, heading_index);
            match &mut *f.target {
                FigureTarget::BlockQuote(b) => {
                    for child in &mut b.children {
                        resolve_reference_links_block(child, defs, heading_index);
                    }
                }
                FigureTarget::Table(t) => {
                    if let Some(caption) = &mut t.caption {
                        resolve_reference_links_inline(caption, defs, heading_index);
                    }
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            resolve_reference_links_inline(&mut cell.children, defs, heading_index);
                        }
                    }
                }
                FigureTarget::Image(_)
                | FigureTarget::CodeBlock(_)
                | FigureTarget::Paragraph(_) => {}
            }
        }
        _ => {}
    }
}

fn resolve_reference_links_inline(
    nodes: &mut [InlineNode],
    defs: &BTreeMap<String, LinkDef>,
    heading_index: &CrossrefIndex,
) {
    for node in nodes {
        match node {
            InlineNode::Link(l) => {
                if let Some(label) = &l.ref_label {
                    // Every branch below KEEPS the node: an unresolved reference
                    // is still a link (PART 12 §3a), so the only question is
                    // whether a destination gets filled in. Reverting it to text
                    // is what §3a forbids - it discarded the fact that the
                    // author wrote a reference, and it did so only on the HTML
                    // path, so one document had two shapes (carve#486).
                    if !l.href.is_empty() {
                        // Explicit definitions are resolved while the inline is
                        // built. A late walk may still be needed for a different
                        // collapsed heading reference in the same document; do
                        // not merge this definition's attributes twice there.
                    } else if let Some(def) = defs
                        .get(borrowed_label_key(label).as_ref())
                        .filter(|_| is_single_line_label(label))
                    {
                        // PART 12 §3a, A RESOLVED REFERENCE KEEPS ITS
                        // DESTINATION: `ref` and `raw_ref` stay BESIDE `href`,
                        // the same way §5 has footnote numbering added
                        // alongside rather than in place of the reference.
                        // Clearing them made `[a][]` and `[a](/url)` the same
                        // tree - the distinction the clause protects
                        // (carve#597). Every renderer already asks whether the
                        // DESTINATION is empty, so nothing downstream reads the
                        // label as "unresolved".
                        apply_link_def(l, def);
                    } else if is_collapsed_reference(l) {
                        let derived = plain_inlines_parse(&l.children);
                        let key = if heading_index.answers_by_text(label) {
                            None
                        } else if derived != *label && heading_index.answers_by_text(&derived) {
                            Some(derived)
                        } else {
                            // The slug fallback answered, or nothing did. The
                            // slug is not one of R1's two keys, so there is no
                            // derived key to publish and the authored spelling
                            // stands.
                            None
                        };
                        let lookup = key.as_deref().unwrap_or(label);
                        if let Some((actual_id, _)) = heading_index.resolve_ref(lookup) {
                            let actual_id = actual_id.to_string();
                            l.href = format!("#{actual_id}");
                            l.title = None;
                            l.from_heading_reference = true;
                            if let Some(key) = key {
                                l.ref_label = Some(key);
                            }
                        }
                    }
                    // A reference tail FRAMES this link's text; it does not
                    // seal it. The text is ordinary inline content, so a
                    // reference written inside it resolves like any other
                    // (corpus 313, markup-carve/carve#1196) - `[t[x][r2]][r]`
                    // renders `x` as a link, the same as the inline-destination
                    // spelling `[t[x][r2]](/u)` already did here.
                    //
                    // AFTER this node's own tail, not before: the heading-index
                    // fallback above derives its lookup key from the children's
                    // plain text, and that key is the text the AUTHOR wrote.
                    resolve_reference_links_inline(&mut l.children, defs, heading_index);
                } else {
                    resolve_reference_links_inline(&mut l.children, defs, heading_index);
                }
            }
            InlineNode::Emphasis(e) => {
                resolve_reference_links_inline(&mut e.children, defs, heading_index);
            }
            InlineNode::Span(s) => {
                resolve_reference_links_inline(&mut s.children, defs, heading_index);
            }
            InlineNode::Extension(e) => {
                resolve_reference_links_inline(&mut e.children, defs, heading_index);
            }
            // AN INLINE NOTE'S CONTENT IS ORDINARY INLINE CONTENT
            // (markup-carve/carve#1203). PART 9 §16 disables FOOTNOTE
            // recognition inside a note and says nothing about references, so a
            // reference written there resolves like any other. This walk had no
            // arm for it, so `^[see [t][r]]` reached the reader as literal text
            // while `*[t][r]*` one node over resolved.
            //
            // The crossref pass a few hundred lines up already descends here,
            // which is why `^[see </#h>]` worked and this did not: one rule,
            // two walks, and only one of them complete.
            InlineNode::Footnote(f) => {
                if let Some(inline) = &mut f.inline {
                    resolve_reference_links_inline(inline, defs, heading_index);
                }
            }
            // The same gap, measured rather than assumed: both critic ranges
            // hold inline children and both left a reference inside them
            // unresolved. `CriticSubstitute` and `CriticComment` are NOT here
            // because they hold strings rather than children - there is nothing
            // to descend into, and an arm for them could not fail.
            InlineNode::CriticInsert(c) => {
                resolve_reference_links_inline(&mut c.children, defs, heading_index);
            }
            InlineNode::CriticDelete(c) => {
                resolve_reference_links_inline(&mut c.children, defs, heading_index);
            }
            InlineNode::CriticSubstitute(c) => {
                resolve_reference_links_inline(&mut c.old, defs, heading_index);
                resolve_reference_links_inline(&mut c.new, defs, heading_index);
            }
            InlineNode::CitationGroup(g) => {
                for item in &mut g.items {
                    if let Some(prefix) = &mut item.prefix {
                        resolve_reference_links_inline(prefix, defs, heading_index);
                    }
                    if let Some(locator) = &mut item.locator {
                        resolve_reference_links_inline(locator, defs, heading_index);
                    }
                }
            }
            InlineNode::Image(img) => {
                if let Some(label) = &img.ref_label {
                    if !img.src.is_empty() {
                        // Already resolved from the active definition context;
                        // see the equivalent link branch above.
                    } else if let Some(def) = defs
                        .get(borrowed_label_key(label).as_ref())
                        .filter(|_| is_single_line_label(label))
                    {
                        // PART 12 §3a - see the note on the link branch above.
                        // AN IMAGE REFERENCE RESOLVES THE SAME ENTRY -
                        // NORMATIVE. It looks the label up in the same table and
                        // takes the same three fields, so a definition's
                        // attributes reach the image exactly as they reach a
                        // link: `[ex]: /i.png {.wide}` gives `class="wide"`.
                        // This branch took `href` and `title` and stopped, which
                        // is not a rule, it is where the implementation stopped -
                        // and the clause says so by name (carve#697).
                        //
                        // Same §15 A3 merge as the link branch above: definition
                        // first, use site second, so a repeated key takes the
                        // LAST value and classes ACCUMULATE in source order.
                        apply_image_def(img, def);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod context_tests {
    use super::*;

    fn definitions(href: &str) -> BTreeMap<String, LinkDef> {
        BTreeMap::from([(
            "r".into(),
            LinkDef {
                raw_label: None,
                href: href.into(),
                title: None,
                line: None,
                attrs: None,
            },
        )])
    }

    #[test]
    fn nested_context_restores_outer_definitions_after_unwind() {
        assert!(active_plain_link_destination("r").is_none());
        let (_, defs, needs_late_resolution) = with_active_link_defs(definitions("/outer"), || {
            let result = std::panic::catch_unwind(|| {
                with_active_link_defs(definitions("/inner"), || {
                    assert_eq!(active_plain_link_destination("r").unwrap().0, "/inner");
                    panic!("extension parse failed");
                });
            });
            assert!(result.is_err());
            assert_eq!(active_plain_link_destination(" r ").unwrap().0, "/outer");
        });
        assert_eq!(defs["r"].href, "/outer");
        assert!(!needs_late_resolution);
        assert!(active_plain_link_destination("r").is_none());
    }
}
