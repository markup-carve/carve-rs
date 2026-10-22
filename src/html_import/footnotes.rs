use super::{
    dom_text_is_layout_only, is_counted_admonition_title, Importer, ADMONITION_ID_PREFIX,
    FOOTNOTE_DEFINITION_BLOCKS, FOOTNOTE_WRAPPER_BLOCKS,
};
use super::{HtmlImportDiagnosticCode, HtmlImportError, HtmlImportSeverity};
use crate::ast::*;
use html5ever::{ns, QualName};
use markup5ever_rcdom::{Handle, Node, NodeData};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
/// A footnote-reference candidate: the anchor, the definition block its
/// fragment resolves to, and the fragment itself.
pub(super) struct FootnoteCandidate {
    reference: Handle,
    block: Handle,
    fragment: String,
    marked: std::cell::Cell<Option<bool>>,
    backlink: std::cell::Cell<Option<bool>>,
}

impl FootnoteCandidate {
    fn is_marked(&self) -> bool {
        self.marked.get().unwrap_or_else(|| {
            let marked = is_footnote_reference_marked(&self.reference);
            self.marked.set(Some(marked));
            marked
        })
    }

    fn is_backlink(&self) -> bool {
        self.backlink.get().unwrap_or_else(|| {
            let backlink = is_footnote_backlink_marked(&self.reference);
            self.backlink.set(Some(backlink));
            backlink
        })
    }
}

/// One recognized note: its block and every reference bound to it.
pub(super) struct FootnoteGroup {
    block: Handle,
    refs: Vec<Handle>,
    fragments: Vec<String>,
}

#[derive(Default)]
pub(super) struct FootnoteLookups {
    target_counts: Option<HashMap<usize, usize>>,
    anchors: HashMap<usize, Option<HashMap<String, Vec<Handle>>>>,
    anchor_ranges: Option<HashMap<String, Vec<(usize, Handle)>>>,
    inverse_ranges: HashMap<String, Vec<(usize, usize)>>,
    inverse_first: HashMap<(usize, String), Option<usize>>,
    subtree_ends: Option<HashMap<usize, usize>>,
    nearest_blocks: Option<HashMap<usize, Option<Handle>>>,
}

impl FootnoteLookups {
    fn target_count(
        &mut self,
        node: &Handle,
        elements: &[Handle],
        used: &HashSet<String>,
    ) -> usize {
        let counts = self.target_counts.get_or_insert_with(|| {
            let mut counts = HashMap::with_capacity(elements.len());
            for element in elements.iter().rev() {
                let count = usize::from(is_footnote_fragment_target(element, used))
                    + counts.get(&node_key(element)).copied().unwrap_or(0);
                counts.insert(node_key(element), count);
                if let Some(parent) = parent_handle(element) {
                    *counts.entry(node_key(&parent)).or_default() += count;
                }
            }
            counts
        });
        counts.get(&node_key(node)).copied().unwrap_or(0)
    }

    fn contains(
        &mut self,
        ancestor: &Handle,
        node: &Handle,
        elements: &[Handle],
        order: &HashMap<usize, usize>,
    ) -> bool {
        let mut current = parent_handle(node);
        for _ in 0..8 {
            let Some(handle) = current else { return false };
            if Rc::ptr_eq(&handle, ancestor) {
                return true;
            }
            current = parent_handle(&handle);
        }
        if current.is_none() {
            return false;
        }
        match (order.get(&node_key(ancestor)), order.get(&node_key(node))) {
            (Some(start), Some(at)) => {
                start < at && *at <= self.subtree_end(ancestor, elements, order)
            }
            _ => false,
        }
    }

    fn nearest_block(&mut self, node: &Handle, elements: &[Handle]) -> Option<Handle> {
        let nearest = self.nearest_blocks.get_or_insert_with(|| {
            let mut nearest: HashMap<usize, Option<Handle>> =
                HashMap::with_capacity(elements.len());
            for element in elements {
                let block = if Importer::tag(element)
                    .map(|tag| FOOTNOTE_DEFINITION_BLOCKS.contains(&tag.as_str()))
                    .unwrap_or(false)
                {
                    Some(element.clone())
                } else {
                    parent_handle(element)
                        .and_then(|parent| nearest.get(&node_key(&parent)).cloned().flatten())
                };
                nearest.insert(node_key(element), block);
            }
            nearest
        });
        nearest.get(&node_key(node)).cloned().flatten()
    }

    fn anchors_for(&mut self, block: &Handle) -> Option<&HashMap<String, Vec<Handle>>> {
        self.anchors
            .entry(node_key(block))
            .or_insert_with(|| {
                let mut indexed: HashMap<String, Vec<Handle>> = HashMap::new();
                let mut stack: Vec<Handle> =
                    block.children.borrow().iter().rev().cloned().collect();
                let mut visited = 0;
                while let Some(node) = stack.pop() {
                    if !matches!(node.data, NodeData::Element { .. }) {
                        continue;
                    }
                    visited += 1;
                    if visited > 64 {
                        return None;
                    }
                    if Importer::tag(&node).as_deref() == Some("a") {
                        let href = Importer::attr(&node, "href").unwrap_or_default();
                        indexed.entry(href).or_default().push(node.clone());
                    }
                    stack.extend(node.children.borrow().iter().rev().cloned());
                }
                Some(indexed)
            })
            .as_ref()
    }

    fn subtree_end(
        &mut self,
        block: &Handle,
        elements: &[Handle],
        order: &HashMap<usize, usize>,
    ) -> usize {
        let ends = self.subtree_ends.get_or_insert_with(|| {
            let mut ends = order.clone();
            for element in elements.iter().rev() {
                if let Some(parent) = parent_handle(element) {
                    let end = ends[&node_key(element)];
                    if let Some(parent_end) = ends.get_mut(&node_key(&parent)) {
                        *parent_end = (*parent_end).max(end);
                    }
                }
            }
            ends
        });
        ends[&node_key(block)]
    }

    fn anchor_range(
        &mut self,
        href: &str,
        elements: &[Handle],
        order: &HashMap<usize, usize>,
    ) -> Option<&Vec<(usize, Handle)>> {
        self.anchor_ranges
            .get_or_insert_with(|| {
                let mut ranges: HashMap<String, Vec<(usize, Handle)>> = HashMap::new();
                for element in elements {
                    if Importer::tag(element).as_deref() == Some("a") {
                        let key = Importer::attr(element, "href").unwrap_or_default();
                        ranges
                            .entry(key)
                            .or_default()
                            .push((order[&node_key(element)], element.clone()));
                    }
                }
                ranges
            })
            .get(href)
    }

    fn has_anchor(
        &mut self,
        block: &Handle,
        href: &str,
        elements: &[Handle],
        order: &HashMap<usize, usize>,
    ) -> bool {
        if let Some(small) = self.anchors_for(block) {
            return small.contains_key(href);
        }
        let end = self.subtree_end(block, elements, order);
        let start = order[&node_key(block)];
        let Some(range) = self.anchor_range(href, elements, order) else {
            return false;
        };
        range
            .get(range.partition_point(|(at, _)| *at <= start))
            .map(|(at, _)| *at <= end)
            .unwrap_or(false)
    }

    fn first_inverse_in_range(
        &mut self,
        block: &Handle,
        href: &str,
        by_reference: &HashMap<usize, usize>,
        elements: &[Handle],
        order: &HashMap<usize, usize>,
    ) -> Option<usize> {
        if !self.inverse_ranges.contains_key(href) {
            let eligible = self
                .anchor_range(href, elements, order)?
                .iter()
                .filter_map(|(at, anchor)| {
                    by_reference
                        .get(&node_key(anchor))
                        .map(|index| (*at, *index))
                })
                .collect();
            self.inverse_ranges.insert(href.to_string(), eligible);
        }
        let end = self.subtree_end(block, elements, order);
        let start = order[&node_key(block)];
        let range = &self.inverse_ranges[href];
        range
            .get(range.partition_point(|(at, _)| *at <= start))
            .filter(|(at, _)| *at <= end)
            .map(|(_, index)| *index)
    }
}

impl<'a> Importer<'a> {
    pub(super) fn adapter_footnotes(
        &mut self,
        root: &Handle,
        heuristic: bool,
    ) -> Result<BTreeMap<String, Vec<BlockNode>>, HtmlImportError> {
        let elements = footnote_document_elements(root);
        let mut order: HashMap<usize, usize> = HashMap::with_capacity(elements.len());
        for (index, element) in elements.iter().enumerate() {
            order.insert(node_key(element), index);
        }

        let targets = footnote_fragment_targets(&elements);
        let mut lookups = FootnoteLookups::default();
        let candidates =
            footnote_pair_candidates(&elements, &targets, heuristic, &order, &mut lookups);
        let candidates =
            resolve_footnote_pair_direction(candidates, &order, &elements, &mut lookups);
        if candidates.is_empty() {
            return Ok(BTreeMap::new());
        }

        let definitions = attach_remaining_footnote_references(
            &elements,
            group_footnote_definitions(candidates, &order, &elements, &mut lookups),
            heuristic,
        );
        drop(lookups);
        let definition_blocks: HashSet<_> = definitions
            .iter()
            .map(|note| node_key(&note.block))
            .collect();
        let mut nested_reference_sites = HashSet::new();
        let mut definitions_with_nested_notes = HashSet::new();
        for reference in definitions.iter().flat_map(|note| note.refs.iter()) {
            let mut parent = parent_handle(reference);
            while let Some(handle) = parent {
                if definition_blocks.contains(&node_key(&handle)) {
                    nested_reference_sites.insert(node_key(reference));
                    definitions_with_nested_notes.insert(node_key(&handle));
                    break;
                }
                parent = parent_handle(&handle);
            }
        }

        let mut defs = BTreeMap::new();
        let mut containers: Vec<Handle> = Vec::new();
        let mut seen: HashSet<usize> = HashSet::new();
        let mut detached: HashMap<usize, (Handle, HashSet<usize>)> = HashMap::new();
        let mut used_labels = HashSet::new();
        let conflicting_carriers: HashSet<usize> = definitions
            .iter()
            .filter(|definition| {
                Self::carrier(&definition.block, crate::roundtrip::FOOTNOTE_LABEL)
                    .is_some_and(|label| valid_footnote_carrier_label(&label))
                    && Self::carrier(&definition.block, crate::roundtrip::INLINE_FOOTNOTE).is_some()
            })
            .map(|definition| node_key(&definition.block))
            .collect();
        let reserved_labels: HashSet<String> = definitions
            .iter()
            .filter(|definition| !conflicting_carriers.contains(&node_key(&definition.block)))
            .flat_map(|definition| {
                std::iter::once(&definition.block)
                    .chain(definition.refs.iter())
                    .filter_map(|handle| Self::carrier(handle, crate::roundtrip::FOOTNOTE_LABEL))
                    .filter(|label| valid_footnote_carrier_label(label))
            })
            .map(|label| crate::parse::label_key(&label))
            .collect();

        let mut labels_in_order = Vec::with_capacity(definitions.len());
        for (index, definition) in definitions.iter().enumerate() {
            let conflicting = conflicting_carriers.contains(&node_key(&definition.block));
            if conflicting {
                self.diag(
                    HtmlImportDiagnosticCode::AttributeDropped,
                    "Dropped conflicting footnote label and inline footnote carriers".into(),
                    HtmlImportSeverity::Warning,
                    "footnote",
                    &definition.block,
                );
            }
            let labels: HashSet<String> = std::iter::once(&definition.block)
                .chain(definition.refs.iter())
                .filter(|_| !conflicting)
                .filter_map(|handle| Self::carrier(handle, crate::roundtrip::FOOTNOTE_LABEL))
                .collect();
            let requested = labels.iter().next().filter(|label| {
                labels.len() == 1
                    && valid_footnote_carrier_label(label)
                    && !used_labels.contains(&crate::parse::label_key(label))
            });
            let label = if let Some(label) = requested {
                label.clone()
            } else {
                if !labels.is_empty() {
                    self.diag(
                        HtmlImportDiagnosticCode::AttributeDropped,
                        "Dropped invalid, conflicting or duplicate footnote label carriers".into(),
                        HtmlImportSeverity::Warning,
                        "footnote",
                        &definition.block,
                    );
                }
                let mut number = index + 1;
                while reserved_labels.contains(&number.to_string())
                    || used_labels.contains(&number.to_string())
                {
                    number += 1;
                }
                number.to_string()
            };
            used_labels.insert(crate::parse::label_key(&label));
            labels_in_order.push(label);
        }
        debug_assert_eq!(labels_in_order.len(), definitions.len());
        // Register every site before reading bodies that can reference later notes.
        for (definition, label) in definitions.iter().zip(&labels_in_order) {
            for reference in &definition.refs {
                let site = footnote_reference_site(reference);
                let attrs = if Self::tag(reference).as_deref() == Some("sup") {
                    let mut attrs = self.attrs(reference, "footnote").unwrap_or_default();
                    attrs.id = None;
                    attrs.order.retain(|slot| !matches!(slot, AttrSlot::Id));
                    (!attrs.classes.is_empty() || !attrs.key_values.is_empty()).then_some(attrs)
                } else {
                    None
                };
                self.footnote_refs.insert(
                    node_key(&site),
                    (
                        site,
                        Footnote {
                            attrs,
                            id: Some(label.clone()),
                            inline: None,
                            number: None,
                            ref_id: None,
                            pos: None,
                        },
                    ),
                );
            }
        }
        for (index, (definition, label)) in definitions.iter().zip(&labels_in_order).enumerate() {
            let conflicting = conflicting_carriers.contains(&node_key(&definition.block));
            if index == 0 {
                remove_footnote_separator(&definition.block);
            }

            let identities: Vec<String> = definition
                .refs
                .iter()
                .map(footnote_anchor_identity)
                .filter(|identity| !identity.is_empty())
                .collect();
            strip_footnote_backlinks(
                &definition.block,
                &identities,
                &definition.fragments,
                self.admonition_title_ids.get_mut().as_mut(),
            );
            self.sibling_index.borrow_mut().clear();
            self.index_backref_entries.borrow_mut().clear();

            let body: Vec<Handle> = definition.block.children.borrow().clone();
            let blocks = self.blocks(&body, &format!("footnote[{label}]"), 1)?;
            let inline_marker = Self::carrier(&definition.block, crate::roundtrip::INLINE_FOOTNOTE);
            let inline = if !conflicting && inline_marker.is_some() {
                match blocks.as_slice() {
                    [BlockNode::Paragraph(paragraph)]
                        if paragraph.attrs.is_none()
                            && definition.refs.len() == 1
                            && !nested_reference_sites.contains(&node_key(&definition.refs[0]))
                            && !definitions_with_nested_notes
                                .contains(&node_key(&definition.block)) =>
                    {
                        Some(paragraph.children.clone())
                    }
                    _ => {
                        self.diag(
                            HtmlImportDiagnosticCode::AttributeDropped,
                            "Inline footnote carrier has a body that cannot be represented inline"
                                .into(),
                            HtmlImportSeverity::Warning,
                            "footnote",
                            &definition.block,
                        );
                        None
                    }
                }
            } else {
                None
            };
            if inline.is_some()
                && std::iter::once(&definition.block)
                    .chain(definition.refs.iter())
                    .filter_map(|handle| Self::carrier(handle, crate::roundtrip::FOOTNOTE_LABEL))
                    .any(|label| valid_footnote_carrier_label(&label))
            {
                self.diag(
                    HtmlImportDiagnosticCode::AttributeDropped,
                    "Dropped footnote label carriers on an inline note".into(),
                    HtmlImportSeverity::Warning,
                    "footnote",
                    &definition.block,
                );
            }
            if inline.is_none() {
                defs.insert(label.clone(), blocks);
            }
            self.detached_footnote_blocks
                .insert(node_key(&definition.block));
            if let Some(index) = self.admonition_title_ids.get_mut() {
                index.remove_subtree(&definition.block);
            }

            for reference in &definition.refs {
                let site = footnote_reference_site(reference);
                let attrs = self
                    .footnote_refs
                    .get(&node_key(&site))
                    .and_then(|(_, note)| note.attrs.clone());
                self.footnote_refs.insert(
                    node_key(&site),
                    (
                        site.clone(),
                        Footnote {
                            attrs,
                            id: if inline.is_some() {
                                None
                            } else {
                                Some(label.clone())
                            },
                            inline: inline.clone(),
                            number: None,
                            ref_id: None,
                            pos: None,
                        },
                    ),
                );
            }

            if let Some(container) = parent_handle(&definition.block) {
                if seen.insert(node_key(&container)) {
                    containers.push(container.clone());
                }
                detached
                    .entry(node_key(&container))
                    .or_insert_with(|| (container, HashSet::new()))
                    .1
                    .insert(node_key(&definition.block));
            }
        }
        for (_, (parent, removed)) in detached {
            parent
                .children
                .borrow_mut()
                .retain(|child| !removed.contains(&node_key(child)));
        }
        for definition in &definitions {
            definition.block.parent.set(None);
        }
        self.detached_footnote_blocks.clear();

        // Kept unique, because every note in one list names the SAME
        // container: pruning it once per note walked that list's children
        // once per note, which is quadratic on a document that is mostly
        // notes.
        // ONE MARKER FOR THE DOCUMENT, at the FIRST section that leaves a slot
        // with content after it. Every note in one list names the same
        // container, and a document with two endnotes sections has one place the
        // renderer will rebuild them, so a second directive would spell a
        // position no render can honour.
        let mut pruning = FootnotePruner::default();
        for container in &containers {
            pruning.prune(container);
        }
        pruning.finish();

        *self.admonition_title_ids.get_mut() = None;
        Ok(defs)
    }
}

/// Every element in the subtree, in document order.
pub(super) fn footnote_document_elements(root: &Handle) -> Vec<Handle> {
    let mut elements = Vec::new();
    let mut stack: Vec<Handle> = root.children.borrow().iter().rev().cloned().collect();
    while let Some(node) = stack.pop() {
        if !matches!(node.data, NodeData::Element { .. }) {
            continue;
        }
        if !matches!(Importer::tag(&node).as_deref(), Some("code" | "pre")) {
            let children = node.children.borrow();
            for child in children.iter().rev() {
                stack.push(child.clone());
            }
        }
        elements.push(node);
    }
    elements
}

/// Map every same-document fragment name to the element it addresses.
///
/// `id` first and `name` second, in two passes rather than one, so an `id`
/// always wins over the legacy `<a name>` form when both spell one fragment.
pub(super) fn footnote_fragment_targets(elements: &[Handle]) -> HashMap<String, Handle> {
    let mut targets: HashMap<String, Handle> = HashMap::new();
    for element in elements {
        if let Some(id) = Importer::attr(element, "id") {
            if !id.is_empty() {
                targets.entry(id).or_insert_with(|| element.clone());
            }
        }
    }
    for element in elements {
        if Importer::tag(element).as_deref() != Some("a") {
            continue;
        }
        if let Some(name) = Importer::attr(element, "name") {
            if !name.is_empty() {
                targets.entry(name).or_insert_with(|| element.clone());
            }
        }
    }
    targets
}

/// Every anchor that could be a footnote reference, with the block it would
/// bind to. A candidate needs the mutual back-link or an explicit reference
/// marker; an anchor inside its own would-be note is never one.
pub(super) fn footnote_pair_candidates(
    elements: &[Handle],
    targets: &HashMap<String, Handle>,
    heuristic: bool,
    order: &HashMap<usize, usize>,
    lookups: &mut FootnoteLookups,
) -> Vec<FootnoteCandidate> {
    let mut anchors: Vec<(Handle, String)> = Vec::new();
    let mut used: HashSet<String> = HashSet::new();
    for element in elements {
        if Importer::tag(element).as_deref() != Some("a") {
            continue;
        }
        let href = Importer::attr(element, "href").unwrap_or_default();
        let Some(fragment) = href.strip_prefix('#') else {
            continue;
        };
        if fragment.is_empty() || !targets.contains_key(fragment) {
            continue;
        }
        used.insert(fragment.to_string());
        anchors.push((element.clone(), fragment.to_string()));
    }

    let mut candidates = Vec::new();
    let mut bare_sites = HashSet::new();
    let mut bare_targets = Vec::new();
    let mut bare_target_numbers = HashMap::new();
    let mut ambiguous_bare_targets = HashSet::new();
    for backlink in elements {
        if Importer::tag(backlink).as_deref() != Some("a")
            || Importer::attr(backlink, "role").as_deref() != Some("doc-backlink")
        {
            continue;
        }
        let href = Importer::attr(backlink, "href").unwrap_or_default();
        // An authored id can replace `fnref{n}` (PART 9 §16), so the target
        // decides, not the spelling.
        if href.len() < 2 || !href.starts_with('#') {
            continue;
        }
        let Some(reference) = targets
            .get(&href[1..])
            .filter(|node| Importer::tag(node).as_deref() == Some("sup"))
        else {
            continue;
        };
        let mut target = parent_handle(backlink);
        while let Some(node) = target.as_ref() {
            if Importer::attr(node, "id").is_some() {
                break;
            }
            target = parent_handle(node);
        }
        let Some(target) = target else { continue };
        let mut ancestor = parent_handle(&target);
        while let Some(node) = ancestor.as_ref() {
            if Importer::attr(node, "role").as_deref() == Some("doc-endnotes") {
                break;
            }
            ancestor = parent_handle(node);
        }
        if ancestor.is_none() {
            continue;
        }
        let fragment = Importer::attr(&target, "id").unwrap();
        used.insert(fragment.clone());
        // Only a generated `fnref{n}` names its note number.
        let number = href
            .strip_prefix("#fnref")
            .and_then(|identity| identity.split('-').next())
            .filter(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()));
        let target_key = node_key(&target);
        if let Some(number) = number {
            if bare_target_numbers
                .insert(target_key, number.to_string())
                .is_some_and(|previous| previous != number)
            {
                ambiguous_bare_targets.insert(target_key);
            }
        }
        bare_targets.push((reference.clone(), target, fragment));
    }
    // Count every target before deciding whether a wrapper holds one note.
    let definition_labels = bare_targets
        .iter()
        .map(|(_, _, label)| label.clone())
        .chain(
            anchors
                .iter()
                .filter(|(anchor, _)| {
                    Importer::attr(anchor, "role").as_deref() != Some("doc-backlink")
                        && (heuristic
                            || Importer::attr(anchor, "role").as_deref() == Some("doc-noteref"))
                })
                .map(|(_, fragment)| fragment.clone()),
        )
        .collect();
    let mut bare_lookups = FootnoteLookups::default();
    for (reference, target, fragment) in bare_targets {
        if ambiguous_bare_targets.contains(&node_key(&target))
            || bare_lookups.target_count(&target, elements, &definition_labels) > 1
        {
            continue;
        }
        let Some(block) = resolve_footnote_definition_block(&target, elements, &used, lookups)
        else {
            continue;
        };
        if lookups.contains(&block, &reference, elements, order)
            || !bare_sites.insert(node_key(&reference))
        {
            continue;
        }
        candidates.push(FootnoteCandidate {
            reference: reference.clone(),
            block,
            fragment,
            marked: std::cell::Cell::new(Some(true)),
            backlink: std::cell::Cell::new(Some(false)),
        });
    }
    for (anchor, fragment) in anchors {
        // OUTSIDE THE HEURISTIC ONLY THE AUTHORED ROLE OPENS A PAIR. The vendor
        // classes belong to the heuristic: a class is a styling hook, where the
        // role is a statement about what the element IS.
        if !heuristic && Importer::attr(&anchor, "role").as_deref() != Some("doc-noteref") {
            continue;
        }
        let Some(block) =
            resolve_footnote_definition_block(&targets[&fragment], elements, &used, lookups)
        else {
            continue;
        };
        if lookups.contains(&block, &anchor, elements, order) {
            continue;
        }

        let identity = footnote_anchor_identity(&anchor);
        let mutual = !identity.is_empty()
            && lookups.has_anchor(&block, &format!("#{identity}"), elements, order);
        if !mutual && !is_footnote_reference_marked(&anchor) {
            continue;
        }

        candidates.push(FootnoteCandidate {
            reference: anchor,
            block,
            fragment,
            marked: std::cell::Cell::new(None),
            backlink: std::cell::Cell::new(None),
        });
    }
    candidates
}

/// The block a reference's target belongs to.
///
/// The target itself when it is already a block (Pandoc's `<li id="fn1">`),
/// otherwise the nearest block ancestor of the anchor the fragment names.
/// Then ONE guarded climb, because Word and LibreOffice wrap each note in a
/// dedicated `<div id=...>` and the body can be several paragraphs inside it:
/// the climb only happens into a wrapper that carries an id and holds exactly
/// one referenced target, which is what keeps a shared container (Google Docs'
/// one trailing `<div>` around every note) from swallowing its siblings.
///
/// A fragment whose nearest block would be the document itself is refused -
/// taking it would move every block in the document into one note - which here
/// is the climb running off the top past `<body>` and `<html>`, neither of
/// which is a definition block.
pub(super) fn resolve_footnote_definition_block(
    target: &Handle,
    elements: &[Handle],
    used: &HashSet<String>,
    lookups: &mut FootnoteLookups,
) -> Option<Handle> {
    let mut block = target.clone();
    let mut depth = 0;
    while !Importer::tag(&block)
        .map(|tag| FOOTNOTE_DEFINITION_BLOCKS.contains(&tag.as_str()))
        .unwrap_or(false)
    {
        let parent = parent_handle(&block)?;
        if !matches!(parent.data, NodeData::Element { .. }) {
            return None;
        }
        block = parent;
        depth += 1;
        if depth == 8 {
            block = lookups.nearest_block(&block, elements)?;
            break;
        }
    }

    if let Some(parent) = parent_handle(&block) {
        let wraps = Importer::tag(&parent)
            .map(|tag| FOOTNOTE_WRAPPER_BLOCKS.contains(&tag.as_str()))
            .unwrap_or(false);
        if wraps
            && Importer::attr(&parent, "role").as_deref() != Some("doc-endnotes")
            && !Importer::attr(&parent, "id").unwrap_or_default().is_empty()
            && lookups.target_count(&parent, elements, used) == 1
        {
            block = parent;
        }
    }

    Some(block)
}

pub(super) fn is_footnote_fragment_target(node: &Handle, used: &HashSet<String>) -> bool {
    if let Some(id) = Importer::attr(node, "id") {
        if !id.is_empty() && used.contains(&id) {
            return true;
        }
    }
    if Importer::tag(node).as_deref() != Some("a") {
        return false;
    }
    Importer::attr(node, "name")
        .map(|name| !name.is_empty() && used.contains(&name))
        .unwrap_or(false)
}

/// Keep one side of every mutually linked anchor pair.
///
/// The pair is symmetric, so both directions produce a candidate and one of
/// them is the back-link reading as a reference. An explicit marker decides
/// where there is one; otherwise document order does, because a footnote
/// reference precedes the note it opens in every export shape measured.
pub(super) fn resolve_footnote_pair_direction(
    candidates: Vec<FootnoteCandidate>,
    order: &HashMap<usize, usize>,
    elements: &[Handle],
    lookups: &mut FootnoteLookups,
) -> Vec<FootnoteCandidate> {
    let mut by_reference: HashMap<usize, usize> = HashMap::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        by_reference.insert(node_key(&candidate.reference), index);
    }

    let mut kept = Vec::new();
    for (index, candidate) in candidates.iter().enumerate() {
        let inverse = inverse_footnote_candidate(
            &candidates,
            &by_reference,
            candidate,
            elements,
            order,
            lookups,
        );
        if inverse
            .map(|other| footnote_reference_side_wins(&candidates[other], candidate, order))
            .unwrap_or(false)
        {
            continue;
        }
        kept.push(index);
    }

    let mut out = Vec::with_capacity(kept.len());
    let mut remaining: Vec<Option<FootnoteCandidate>> = candidates.into_iter().map(Some).collect();
    for index in kept {
        if let Some(candidate) = remaining[index].take() {
            out.push(candidate);
        }
    }
    out
}

/// The candidate that reads the same mutual pair from the other end.
///
/// Found through the back anchor the candidate's own block holds rather than
/// by comparing every candidate with every other: a document with a thousand
/// notes made that scan a thousand times a thousand containment walks, and the
/// anchor names the inverse directly.
pub(super) fn inverse_footnote_candidate(
    candidates: &[FootnoteCandidate],
    by_reference: &HashMap<usize, usize>,
    candidate: &FootnoteCandidate,
    elements: &[Handle],
    order: &HashMap<usize, usize>,
    lookups: &mut FootnoteLookups,
) -> Option<usize> {
    let identity = footnote_anchor_identity(&candidate.reference);
    if identity.is_empty() {
        return None;
    }
    let wanted = format!("#{identity}");
    let key = (node_key(&candidate.block), wanted.clone());
    let first = if let Some(first) = lookups.inverse_first.get(&key) {
        *first
    } else if let Some(small) = lookups.anchors_for(&candidate.block) {
        let anchors = small.get(&wanted)?;
        let first = anchors
            .iter()
            .find_map(|anchor| by_reference.get(&node_key(anchor)).copied());
        let cache = anchors.len() >= 8;
        if cache {
            lookups.inverse_first.insert(key, first);
        }
        first
    } else {
        lookups.first_inverse_in_range(&candidate.block, &wanted, by_reference, elements, order)
    };
    // Equal hrefs resolve to the same block through the first fragment target.
    first.filter(|&index| {
        lookups.contains(
            &candidates[index].block,
            &candidate.reference,
            elements,
            order,
        )
    })
}

pub(super) fn footnote_reference_side_wins(
    first: &FootnoteCandidate,
    second: &FootnoteCandidate,
    order: &HashMap<usize, usize>,
) -> bool {
    let first_marked = first.is_marked();
    let second_marked = second.is_marked();
    if first_marked != second_marked {
        return first_marked;
    }

    let first_back = first.is_backlink();
    let second_back = second.is_backlink();
    if first_back != second_back {
        return second_back;
    }

    order.get(&node_key(&first.reference)).copied().unwrap_or(0)
        < order
            .get(&node_key(&second.reference))
            .copied()
            .unwrap_or(0)
}

/// One entry per definition block, carrying every reference bound to it.
///
/// A block that contains another definition block is a container, not a note:
/// keeping both would move a subtree into two places at once. The next group
/// in document order reveals whether a block contains a note.
pub(super) fn group_footnote_definitions(
    candidates: Vec<FootnoteCandidate>,
    order: &HashMap<usize, usize>,
    elements: &[Handle],
    lookups: &mut FootnoteLookups,
) -> Vec<FootnoteGroup> {
    let mut index_of: HashMap<usize, usize> = HashMap::new();
    let mut groups: Vec<FootnoteGroup> = Vec::new();
    let mut fragments: Vec<Option<HashSet<String>>> = Vec::new();
    for candidate in candidates {
        let key = node_key(&candidate.block);
        let index = *index_of.entry(key).or_insert_with(|| {
            fragments.push(None);
            groups.push(FootnoteGroup {
                block: candidate.block.clone(),
                refs: Vec::new(),
                fragments: Vec::new(),
            });
            groups.len() - 1
        });
        groups[index].refs.push(candidate.reference);
        let known = if groups[index].fragments.len() < 8 {
            groups[index].fragments.contains(&candidate.fragment)
        } else {
            !fragments[index]
                .get_or_insert_with(|| groups[index].fragments.iter().cloned().collect())
                .insert(candidate.fragment.clone())
        };
        if !known {
            groups[index].fragments.push(candidate.fragment);
        }
    }

    groups.sort_by_key(|group| order.get(&node_key(&group.block)).copied().unwrap_or(0));
    let containers: HashSet<usize> = groups
        .windows(2)
        .filter(|pair| lookups.contains(&pair[0].block, &pair[1].block, elements, order))
        .map(|pair| node_key(&pair[0].block))
        .collect();
    groups
        .into_iter()
        .filter(|group| !containers.contains(&node_key(&group.block)))
        .collect()
}

/// Bind every remaining anchor that addresses a confirmed note.
///
/// Once a block IS a footnote definition, an anchor pointing at it is a
/// reference to it whatever it looks like. This matters for the second and
/// later reference to one note: only one of them can be the back-link's
/// target, so the mutual pair that confirmed the note cannot confirm them, and
/// without this they stayed literal links beside a `[^1]`. An anchor inside a
/// note stays a link - a note's body may address another note.
pub(super) fn attach_remaining_footnote_references(
    elements: &[Handle],
    mut definitions: Vec<FootnoteGroup>,
    heuristic: bool,
) -> Vec<FootnoteGroup> {
    let mut by_fragment: HashMap<String, usize> = HashMap::new();
    for (index, definition) in definitions.iter().enumerate() {
        for fragment in &definition.fragments {
            by_fragment.insert(fragment.clone(), index);
        }
    }

    // Which elements sit inside a note, computed once: asking each anchor
    // whether it is inside any note walked the tree once per anchor and per
    // note, which is quadratic on a document that is mostly notes.
    let mut inside: HashSet<usize> = HashSet::new();
    for definition in &definitions {
        for element in footnote_document_elements(&definition.block) {
            inside.insert(node_key(&element));
        }
        inside.insert(node_key(&definition.block));
    }

    let mut references: Vec<Option<HashSet<usize>>> =
        (0..definitions.len()).map(|_| None).collect();
    for element in elements {
        if Importer::tag(element).as_deref() != Some("a") {
            continue;
        }
        // Outside the heuristic an unmarked anchor addressing a note is a LINK,
        // not a reference: the role is the whole signal, so a content link to
        // `#fn1` in a role-marked document keeps the author's shape. The marked
        // candidates already sit in their groups; this loop exists for the
        // unmarked SECOND reference the heuristic binds.
        if !heuristic && Importer::attr(element, "role").as_deref() != Some("doc-noteref") {
            continue;
        }
        let href = Importer::attr(element, "href").unwrap_or_default();
        let Some(fragment) = href.strip_prefix('#') else {
            continue;
        };
        let Some(&index) = by_fragment.get(fragment) else {
            continue;
        };
        if inside.contains(&node_key(element)) {
            continue;
        }
        let known = if definitions[index].refs.len() < 8 {
            definitions[index]
                .refs
                .iter()
                .any(|reference| Rc::ptr_eq(reference, element))
        } else {
            !references[index]
                .get_or_insert_with(|| definitions[index].refs.iter().map(node_key).collect())
                .insert(node_key(element))
        };
        if !known {
            definitions[index].refs.push(element.clone());
        }
    }

    definitions
}

pub(super) fn valid_footnote_carrier_label(label: &str) -> bool {
    !crate::parse::label_key(label).is_empty() && !label.contains(['[', ']', '|', '\r', '\n'])
}

pub(super) fn footnote_anchor_identity(anchor: &Handle) -> String {
    match Importer::attr(anchor, "id") {
        Some(id) if !id.is_empty() => id,
        _ => Importer::attr(anchor, "name").unwrap_or_default(),
    }
}

pub(super) fn footnote_anchors_under(node: &Handle) -> Vec<Handle> {
    footnote_document_elements(node)
        .into_iter()
        .filter(|element| Importer::tag(element).as_deref() == Some("a"))
        .collect()
}

/// `footnoteRef` is Pandoc 1.x's spelling of `footnote-ref`, which it used
/// together with a back-link carrying no attributes at all.
pub(super) fn is_footnote_reference_marked(anchor: &Handle) -> bool {
    if Importer::attr(anchor, "role").as_deref() == Some("doc-noteref") {
        return true;
    }
    footnote_has_class(anchor, "footnote-ref") || footnote_has_class(anchor, "footnoteRef")
}

pub(super) fn is_footnote_backlink_marked(anchor: &Handle) -> bool {
    if Importer::attr(anchor, "role").as_deref() == Some("doc-backlink") {
        return true;
    }
    footnote_has_class(anchor, "footnote-back")
}

pub(super) fn footnote_has_class(node: &Handle, wanted: &str) -> bool {
    Importer::attr(node, "class")
        .unwrap_or_default()
        .split_whitespace()
        .any(|class| class == wanted)
}

pub(super) fn node_key(handle: &Handle) -> usize {
    Rc::as_ptr(handle) as usize
}

/// Whether an element carries `wanted` among its space-separated classes.
pub(super) fn has_class(node: &Handle, wanted: &str) -> bool {
    Importer::attr(node, "class")
        .unwrap_or_default()
        .split_whitespace()
        .any(|class| class == wanted)
}

/// Whether this `<p class="admonition-title">` is one the renderer's own
/// counter counted.
///
/// WHICH PARAGRAPHS THE COUNTER COUNTS is the renderer's condition, not the
/// class alone. It increments for a CANONICAL admonition with a title and no
/// authored name, and that is exactly when it emits the id and points the
/// `<aside>`'s `aria-labelledby` at it - so a paragraph qualifies when its
/// parent aside names it back. Counting every `admonition-title` instead would
/// desync on the two shapes that carry the class and no counter: a
/// non-canonical `::: custom "T"` (a `<div>`, title with no id) and a canonical
/// one the author named (`aria-label` wins, title with no id).
/// Whether this is the paragraph a container's TITLE renders as.
///
/// The class alone, because the class alone is what the renderer always writes.
/// The generated `id` beside it is conditional - `render_admonition` emits one
/// only for a Tier-1 kind with no authored name - so a `::: sidebar "A"` (a
/// `<div>`, no id) and a `::: note "A"` carrying an authored `aria-label` (no id
/// either) both render a bare `<p class="admonition-title">`. Reading the id as
/// the marker left the title of both in the body, where it was written back as
/// an ordinary paragraph carrying the renderer's class.
///
/// [`is_counted_admonition_title`] is the NARROWER question and stays narrow: it
/// asks whether the renderer's counter produced this id, which is a question
/// about dropping a derived value, not about what the paragraph IS.
/// A value lifted off the front of a container's body onto its OPENER, plus
/// what is left of the body and that body's own paths.
///
/// Two lifts have this shape - the quoted title and the grouping `[label]` -
/// and naming it keeps their signatures readable and stops `type_complexity`
/// firing on each of them separately.
pub(super) type Lifted<T> = Result<(Option<T>, Vec<Handle>, Vec<String>), HtmlImportError>;

/// The id the renderer derives for this title paragraph - `adm-1`, `adm-2`, …
/// in document order, which is the order the renderer's own counter runs in,
/// PUT THROUGH THE ID NAMESPACE the renderer allocates in.
/// Replays the renderer's ID allocation over the remaining DOM.
/// Derived counters have disjoint suffix namespaces; only authored IDs reserve
/// names that another title allocation can encounter.
pub(super) struct AdmonitionTitleIndex {
    nodes: HashMap<usize, (Option<usize>, Option<String>)>,
    counts: Vec<usize>,
    reserved: HashMap<String, usize>,
    suffixes: HashMap<String, usize>,
}

impl AdmonitionTitleIndex {
    pub(super) fn new(node: &Handle, detached: &HashSet<usize>) -> Self {
        let mut root = node.clone();
        while let Some(parent) = parent_handle(&root) {
            root = parent;
        }
        let mut nodes = HashMap::new();
        let mut reserved = HashMap::new();
        let mut titles = 0;
        let mut stack = vec![root];
        while let Some(current) = stack.pop() {
            if detached.contains(&node_key(&current)) {
                continue;
            }
            if is_counted_admonition_title(&current) {
                titles += 1;
                nodes.insert(node_key(&current), (Some(titles), None));
            } else if let Some(id) =
                Importer::attr(&current, "id").filter(|id| id.starts_with(ADMONITION_ID_PREFIX))
            {
                *reserved.entry(id.clone()).or_insert(0) += 1;
                nodes.insert(node_key(&current), (None, Some(id)));
            }
            stack.extend(current.children.borrow().iter().rev().cloned());
        }
        let mut counts = vec![0; titles + 1];
        for (position, count) in counts.iter_mut().enumerate().skip(1) {
            *count = position & position.wrapping_neg();
        }
        Self {
            nodes,
            counts,
            reserved,
            suffixes: HashMap::new(),
        }
    }

    pub(super) fn title_id(&mut self, node: &Handle) -> Option<String> {
        let (position, _) = self.nodes.get(&node_key(node))?;
        let mut position = (*position)?;
        let mut ordinal = 0;
        while position > 0 {
            ordinal += self.counts[position];
            position &= position - 1;
        }
        let base = format!("{ADMONITION_ID_PREFIX}{ordinal}");
        if !self.reserved.contains_key(&base) {
            return Some(base);
        }
        // Different title counters have disjoint suffix namespaces.
        let suffix = self.suffixes.entry(base.clone()).or_insert(2);
        while self.reserved.contains_key(&format!("{base}-{suffix}")) {
            *suffix += 1;
        }
        Some(format!("{base}-{suffix}"))
    }

    pub(super) fn remove_node(&mut self, node: &Handle) {
        let Some((position, id)) = self.nodes.remove(&node_key(node)) else {
            return;
        };
        if let Some(mut position) = position {
            while position < self.counts.len() {
                self.counts[position] -= 1;
                position += position & position.wrapping_neg();
            }
        }
        if let Some(id) = id {
            let count = self.reserved.get_mut(&id).expect("indexed reservation");
            *count -= 1;
            if *count == 0 {
                self.reserved.remove(&id);
                if let Some((base, suffix)) = id.rsplit_once('-') {
                    if let Ok(value) = suffix.parse::<usize>() {
                        if value >= 2 && value.to_string() == suffix {
                            if let Some(next) = self.suffixes.get_mut(base) {
                                *next = (*next).min(value);
                            }
                        }
                    }
                }
            }
        }
    }

    pub(super) fn remove_subtree(&mut self, node: &Handle) {
        let mut stack = vec![node.clone()];
        while let Some(current) = stack.pop() {
            self.remove_node(&current);
            stack.extend(current.children.borrow().iter().cloned());
        }
    }
}

/// The node's parent, restored into the cell it was read out of.
///
/// `Cell` has no borrow, so the only way to look at a `Weak` inside one is to
/// take it and put it back.
pub(super) fn parent_handle(node: &Handle) -> Option<Handle> {
    let weak = node.parent.take();
    let parent = weak.as_ref().and_then(|parent| parent.upgrade());
    node.parent.set(weak);
    parent
}

pub(super) fn footnote_detach_many(nodes: impl IntoIterator<Item = Handle>) {
    let mut nodes = nodes.into_iter();
    let Some(first) = nodes.next() else { return };
    let Some(second) = nodes.next() else {
        if let Some(parent) = parent_handle(&first) {
            let mut children = parent.children.borrow_mut();
            if let Some(at) = children.iter().position(|child| Rc::ptr_eq(child, &first)) {
                children.remove(at);
            }
            first.parent.set(None);
        }
        return;
    };
    let mut groups: HashMap<usize, (Handle, HashSet<usize>)> = HashMap::new();
    for node in [first, second].into_iter().chain(nodes) {
        if let Some(parent) = parent_handle(&node) {
            groups
                .entry(node_key(&parent))
                .or_insert_with(|| (parent, HashSet::new()))
                .1
                .insert(node_key(&node));
            node.parent.set(None);
        }
    }
    for (_, (parent, removed)) in groups {
        parent
            .children
            .borrow_mut()
            .retain(|child| !removed.contains(&node_key(child)));
    }
}

/// Remove the rule that separates the notes from the body.
///
/// Every producer measured emits one, and it is chrome rather than content:
/// Pandoc puts `<hr />` inside the section, Word `<br clear=all><hr ...>`
/// inside the footnote-list div, Google Docs a bare `<hr class="cN">` as a
/// sibling of the notes. Only the first two would be swept up by pruning an
/// emptied container, so the separator is looked for explicitly - at the first
/// note, and at each of its ancestors, taking only what immediately precedes
/// it.
pub(super) fn remove_footnote_separator(first: &Handle) {
    let mut node = first.clone();
    loop {
        let Some(parent) = parent_handle(&node) else {
            return;
        };
        let children = parent.children.borrow();
        let Some(index) = children.iter().position(|child| Rc::ptr_eq(child, &node)) else {
            return;
        };
        let mut removed = Vec::new();
        let mut stopped = false;
        for previous in children[..index].iter().rev() {
            if is_footnote_chrome_node(previous) {
                continue;
            }
            if matches!(Importer::tag(previous).as_deref(), Some("hr") | Some("br")) {
                removed.push(previous.clone());
            } else {
                stopped = true;
                break;
            }
        }
        drop(children);
        footnote_detach_many(removed);
        if stopped
            || matches!(
                Importer::tag(&parent).as_deref(),
                None | Some("body") | Some("html")
            )
        {
            return;
        }
        node = parent;
    }
}

/// Whether a node is part of the separator's packaging rather than content.
///
/// Word brackets the `<br clear=all><hr>` inside the footnote-list div in
/// downlevel-revealed conditionals, `<![if !supportFootnotes]>` and the
/// matching `<![endif]>`. Those are NOT comments in the source, but html5ever
/// follows the HTML grammar and reads `<!` without `--` as a BOGUS COMMENT, so
/// here they arrive as comment nodes - measured, not assumed - and the comment
/// branch is what recognizes them. carve-php reads the same bytes back as TEXT
/// because libxml has no such production, which is why its port carries a
/// pattern for the text spelling that this one has no way to reach.
pub(super) fn is_footnote_chrome_node(node: &Handle) -> bool {
    match &node.data {
        NodeData::Comment { .. } => true,
        // MEASURED (markup-carve/carve-rs#1345): read through `str::trim`, a text
        // node holding one U+00A0 immediately before the separator was chrome, so
        // the walk stepped over it and the content space was gone from both
        // exits. The same slot holding an ordinary word kept it.
        NodeData::Text { .. } => dom_text_is_layout_only(node),
        _ => false,
    }
}

/// Remove the navigation an engine regenerates: the back-link, and the marker
/// anchor Word, Google Docs and LibreOffice put it on.
///
/// Carried into the note body it would render as a stray link to a fragment
/// that no longer exists, and the visible marker it wraps (`[1]`, `1`, the
/// return arrow) would be written into the note's own text. The third clause -
/// an anchor that IS the fragment target the reference points at, with a
/// fragment href - is what removes the marker anchor that is the note's anchor
/// and its back-link and its visible marker in one element.
pub(super) fn strip_footnote_backlinks(
    block: &Handle,
    identities: &[String],
    fragments: &[String],
    mut title_index: Option<&mut AdmonitionTitleIndex>,
) {
    let identity_set: Option<HashSet<&str>> =
        (identities.len() >= 8).then(|| identities.iter().map(String::as_str).collect());
    let fragment_set: Option<HashSet<&str>> =
        (fragments.len() >= 8).then(|| fragments.iter().map(String::as_str).collect());
    let mut anchors = Vec::new();
    let mut parents = Vec::new();
    for anchor in footnote_anchors_under(block) {
        let href = Importer::attr(&anchor, "href").unwrap_or_default();
        let target = href.strip_prefix('#');
        let points_back = target
            .map(|fragment| {
                identity_set
                    .as_ref()
                    .map(|set| set.contains(fragment))
                    .unwrap_or_else(|| identities.iter().any(|identity| identity == fragment))
            })
            .unwrap_or(false);
        let is_marker = target.is_some() && {
            let identity = footnote_anchor_identity(&anchor);
            fragment_set
                .as_ref()
                .map(|set| set.contains(identity.as_str()))
                .unwrap_or_else(|| fragments.contains(&identity))
        };
        if !is_footnote_backlink_marked(&anchor) && !points_back && !is_marker {
            continue;
        }
        if let Some(parent) = parent_handle(&anchor) {
            if matches!(
                Importer::tag(&parent).as_deref(),
                Some("sup") | Some("span")
            ) {
                parents.push(parent);
            }
        }
        if let Some(index) = title_index.as_deref_mut() {
            index.remove_subtree(&anchor);
        }
        anchors.push(anchor);
    }
    footnote_detach_many(anchors);
    if parents.is_empty() {
        return;
    }
    let mut seen = HashSet::new();
    let mut ordered = Vec::new();
    for parent in parents.into_iter().rev() {
        if seen.insert(node_key(&parent)) {
            ordered.push(parent);
        }
    }
    let mut empty = HashSet::new();
    let mut detached = Vec::new();
    for parent in ordered.into_iter().rev() {
        if !matches!(
            Importer::tag(&parent).as_deref(),
            Some("sup") | Some("span")
        ) {
            continue;
        }
        let emptied = parent.children.borrow().iter().all(|child| {
            empty.contains(&node_key(child))
                || match &child.data {
                    NodeData::Element { .. } => false,
                    NodeData::Text { .. } => dom_text_is_layout_only(child),
                    _ => true,
                }
        });
        if emptied {
            empty.insert(node_key(&parent));
            if let Some(index) = title_index.as_deref_mut() {
                index.remove_node(&parent);
            }
            detached.push(parent);
        }
    }
    footnote_detach_many(detached);
}

/// The node a reference occupies: the anchor, or the `<sup>` that holds
/// nothing but the anchor.
///
/// Google Docs and Pandoc put the `<sup>` outside the anchor, so taking only
/// the anchor would leave `{^...^}` wrapped around the reference. One carrying
/// anything else - an element or non-blank text - keeps its content, and the
/// reference binds inside it.
pub(super) fn footnote_reference_site(reference: &Handle) -> Handle {
    let Some(parent) = parent_handle(reference) else {
        return reference.clone();
    };
    if Importer::tag(&parent).as_deref() != Some("sup") {
        return reference.clone();
    }
    for child in parent.children.borrow().iter() {
        match &child.data {
            NodeData::Element { .. } if !Rc::ptr_eq(child, reference) => {
                return reference.clone();
            }
            // MEASURED (markup-carve/carve-rs#1345): read through `str::trim`, a
            // `<sup>` holding one U+00A0 beside the anchor counted as holding
            // nothing else, so the whole `<sup>` became the reference site and the
            // content space went with it. A word there kept the `<sup>` as
            // `{^Z[^1]^}`.
            NodeData::Text { .. } if !dom_text_is_layout_only(child) => {
                return reference.clone();
            }
            _ => {}
        }
    }
    parent
}

/// The synthetic element `mark_footnote_placement` leaves where a non-final
/// endnotes section sat. It exists only between the footnote pass and the block
/// walk, and no real HTML input can carry the name.
/// Marks the removed notes' position until the block import consumes it.
pub(super) const FOOTNOTE_PLACEMENT_TAG: &str = "carve-footnote-placement";

pub(super) struct FootnotePruneView {
    node: Handle,
    children: Vec<Handle>,
    positions: HashMap<usize, usize>,
    following: Vec<usize>,
    content: usize,
}

#[derive(Default)]
pub(super) struct FootnotePruner {
    views: HashMap<usize, FootnotePruneView>,
    removed: HashSet<usize>,
    changed: HashSet<usize>,
    placement: Option<(Handle, usize, Handle)>,
}

impl FootnotePruner {
    fn packaging(node: &Handle) -> bool {
        is_footnote_chrome_node(node)
            || matches!(Importer::tag(node).as_deref(), Some("hr") | Some("br"))
    }

    fn view(&mut self, node: &Handle) -> &mut FootnotePruneView {
        self.views.entry(node_key(node)).or_insert_with(|| {
            let children = node.children.borrow().clone();
            let positions = children
                .iter()
                .enumerate()
                .map(|(i, child)| (node_key(child), i))
                .collect();
            let mut following = vec![children.len(); children.len() + 1];
            let mut content = 0;
            for i in (0..children.len()).rev() {
                let child = &children[i];
                if !self.removed.contains(&node_key(child)) && !Self::packaging(child) {
                    content += 1;
                }
                following[i] = if is_footnote_chrome_node(child) {
                    following[i + 1]
                } else {
                    i
                };
            }
            FootnotePruneView {
                node: node.clone(),
                children,
                positions,
                following,
                content,
            }
        })
    }

    fn content_from(&mut self, parent: &Handle, from: usize) -> bool {
        self.view(parent);
        let view = self.views.get_mut(&node_key(parent)).unwrap();
        let mut traversed = Vec::new();
        let mut index = view
            .following
            .get(from)
            .copied()
            .unwrap_or(view.children.len());
        while index < view.children.len() && self.removed.contains(&node_key(&view.children[index]))
        {
            traversed.push(index);
            index = view.following[index + 1];
        }
        for at in traversed {
            view.following[at] = index;
        }
        if let Some(slot) = view.following.get_mut(from) {
            *slot = index;
        }
        index < view.children.len()
    }

    fn content_follows(&mut self, parent: &Handle, mut from: usize) -> bool {
        let mut node = parent.clone();
        loop {
            if self.content_from(&node, from) {
                return true;
            }
            let Some(up) = parent_handle(&node) else {
                return false;
            };
            let Some(at) = self.view(&up).positions.get(&node_key(&node)).copied() else {
                return false;
            };
            if self.removed.contains(&node_key(&node)) {
                return false;
            }
            node = up;
            from = at + 1;
        }
    }

    // Keep one marker at the outermost removed slot, only when content follows.
    pub(super) fn prune(&mut self, container: &Handle) {
        let mut node = container.clone();
        let mut slot = None;
        loop {
            if self.removed.contains(&node_key(&node))
                || matches!(
                    Importer::tag(&node).as_deref(),
                    None | Some("body") | Some("html")
                )
                || self.view(&node).content != 0
            {
                break;
            }
            let Some(parent) = parent_handle(&node) else {
                break;
            };
            let Some(index) = self.view(&parent).positions.get(&node_key(&node)).copied() else {
                break;
            };
            self.removed.insert(node_key(&node));
            if !Self::packaging(&node) {
                self.view(&parent).content -= 1;
            }
            self.changed.insert(node_key(&parent));
            node.parent.set(None);
            slot = Some((parent.clone(), index));
            node = parent;
        }
        if let Some((parent, index)) = slot {
            if self.placement.is_none() && self.content_follows(&parent, index) {
                let marker = Node::new(NodeData::Element {
                    name: QualName::new(None, ns!(html), FOOTNOTE_PLACEMENT_TAG.into()),
                    attrs: RefCell::new(Vec::new()),
                    template_contents: RefCell::new(None),
                    mathml_annotation_xml_integration_point: false,
                });
                marker.parent.set(Some(Rc::downgrade(&parent)));
                self.view(&parent).content += 1;
                self.changed.insert(node_key(&parent));
                self.placement = Some((parent, index, marker));
            }
        }
    }

    pub(super) fn finish(self) {
        for key in self.changed {
            let view = &self.views[&key];
            let mut kept = Vec::with_capacity(view.children.len());
            for i in 0..=view.children.len() {
                if let Some((parent, index, marker)) = &self.placement {
                    if node_key(parent) == key && *index == i {
                        kept.push(marker.clone());
                    }
                }
                if let Some(child) = view.children.get(i) {
                    if !self.removed.contains(&node_key(child)) {
                        kept.push(child.clone());
                    }
                }
            }
            *view.node.children.borrow_mut() = kept;
        }
    }
}
