//! The note pass of the `word` and `google-docs` editor adapters
//! (C2-M-102 to C2-M-110), on the parsed tree before the mapping.

use super::attrs::class_list;
use super::dom::{Dom, Id, Kind, NodeData};
use super::notes::{is_backlink, is_footnote_section};
use std::collections::{HashMap, HashSet};

const BLOCKS: &[&str] = &["li", "div", "section", "aside", "p", "td", "blockquote"];
const WRAPPERS: &[&str] = &["div", "li", "section", "aside"];
const KEY_PREFIXES: &[&str] = &["user-content-fn-", "fn-", "fn"];

struct Candidate {
    anchor: Id,
    block: Id,
    fragment: String,
}

struct Note {
    block: Id,
    refs: Vec<Id>,
    fragments: HashSet<String>,
}

fn href_fragment(dom: &Dom, id: Id) -> Option<&str> {
    dom.attr(id, "href")
        .filter(|h| h.len() > 1)
        .and_then(|h| h.strip_prefix('#'))
}

fn identity(dom: &Dom, id: Id) -> &str {
    match dom.attr(id, "id") {
        Some(v) if !v.is_empty() => v,
        _ => dom.attr(id, "name").unwrap_or(""),
    }
}

fn has_class(dom: &Dom, id: Id, names: &[&str]) -> bool {
    class_list(dom, id)
        .iter()
        .any(|c| names.contains(&c.as_str()))
}

fn is_marked_reference(dom: &Dom, id: Id) -> bool {
    has_class(dom, id, &["footnote-ref", "footnoteRef"])
}

fn is_top_level(dom: &Dom, id: Id) -> bool {
    dom.is(id, "body") || dom.is(id, "html")
}

fn tag_in(dom: &Dom, id: Id, names: &[&str]) -> bool {
    dom.tag(id).is_some_and(|t| names.contains(&t))
}

fn note_key(id: &str) -> &str {
    KEY_PREFIXES
        .iter()
        .find_map(|p| id.strip_prefix(p))
        .unwrap_or(id)
}

/// C2-M-108: whitespace, comments, `<hr>` and `<br>` are the layout of the export.
fn is_layout(dom: &Dom, id: Id) -> bool {
    dom.is_blank(id) || dom.is_comment(id) || dom.is(id, "hr") || dom.is(id, "br")
}

/// The elements in document order, with the end of each subtree as an index.
struct Walk {
    order: Vec<Id>,
    index: HashMap<Id, usize>,
    end: HashMap<Id, usize>,
    sections: Vec<Id>,
}

impl Walk {
    fn new(dom: &Dom) -> Self {
        let mut walk = Self {
            order: Vec::new(),
            index: HashMap::new(),
            end: HashMap::new(),
            sections: Vec::new(),
        };
        let mut stack: Vec<(Id, bool)> = Vec::new();
        let push = |stack: &mut Vec<(Id, bool)>, nodes: &[Id]| {
            stack.extend(
                nodes
                    .iter()
                    .rev()
                    .filter(|&&n| dom.is_element(n))
                    .map(|&n| (n, false)),
            );
        };
        push(&mut stack, dom.children(dom.root));
        while let Some((node, exit)) = stack.pop() {
            if exit {
                walk.end.insert(node, walk.order.len() - 1);
                continue;
            }
            if dom.is(node, "section") && is_footnote_section(dom, node) {
                walk.sections.push(node);
            }
            walk.index.insert(node, walk.order.len());
            walk.order.push(node);
            stack.push((node, true));
            push(&mut stack, dom.children(node));
        }
        walk
    }

    fn at(&self, id: Id) -> usize {
        self.index.get(&id).copied().unwrap_or(usize::MAX)
    }

    fn end_of(&self, id: Id) -> usize {
        self.end.get(&id).copied().unwrap_or(0)
    }

    fn contains(&self, outer: Id, inner: Id) -> bool {
        let at = self.at(inner);
        at >= self.at(outer) && at <= self.end_of(outer)
    }

    /// The positions inside `block` of the `<a>` elements of `list`.
    fn within<'l>(&self, list: &'l [usize], block: Id) -> &'l [usize] {
        let from = list.partition_point(|&p| p < self.at(block));
        let to = list.partition_point(|&p| p <= self.end_of(block));
        list.get(from..to).unwrap_or(&[])
    }
}

/// Edits in place: a removed child keeps its slot until `finish`.
#[derive(Default)]
struct Edits {
    gone: HashSet<Id>,
}

impl Edits {
    fn is_gone(&self, id: Id) -> bool {
        self.gone.contains(&id)
    }

    fn children(&self, dom: &Dom, parent: Id) -> Vec<Id> {
        dom.children(parent)
            .iter()
            .copied()
            .filter(|c| !self.is_gone(*c))
            .collect()
    }

    fn parent_of(&self, dom: &Dom, id: Id) -> Option<Id> {
        if self.is_gone(id) {
            None
        } else {
            dom.parent(id)
        }
    }

    fn before(&self, dom: &Dom, id: Id) -> Option<Id> {
        let siblings = dom.children(dom.parent(id)?);
        let at = siblings.iter().position(|&c| c == id)?;
        siblings
            .get(..at)?
            .iter()
            .rev()
            .copied()
            .find(|c| !self.is_gone(*c))
    }

    fn detach(&mut self, dom: &Dom, id: Id) {
        if dom.parent(id).is_some() {
            self.gone.insert(id);
        }
    }

    fn replace(dom: &mut Dom, id: Id, replacement: Id) {
        let Some(parent) = dom.parent(id) else {
            return;
        };
        let mut siblings = dom.children(parent).to_vec();
        let Some(at) = siblings.iter().position(|&c| c == id) else {
            return;
        };
        if let Some(slot) = siblings.get_mut(at) {
            *slot = replacement;
        }
        dom.set_children(parent, siblings);
        dom.set_parent(replacement, Some(parent));
        dom.set_parent(id, None);
    }

    fn finish(&self, dom: &mut Dom) {
        let parents: HashSet<Id> = self.gone.iter().filter_map(|&g| dom.parent(g)).collect();
        for parent in parents {
            let kept = self.children(dom, parent);
            dom.set_children(parent, kept);
        }
        for &g in &self.gone {
            dom.set_parent(g, None);
        }
    }
}

fn element(dom: &mut Dom, name: &str, attrs: Vec<(String, String)>, children: Vec<Id>) -> Id {
    let id = dom.push(NodeData {
        kind: Kind::Element {
            name: html5ever::LocalName::from(name),
            html: true,
            attrs,
        },
        children: children.clone(),
        parent: None,
        content: None,
        span: None,
    });
    for child in children {
        dom.set_parent(child, Some(id));
    }
    id
}

fn fragment_targets(dom: &Dom, walk: &Walk) -> HashMap<String, Id> {
    let mut targets = HashMap::new();
    for &node in &walk.order {
        if let Some(id) = dom.attr(node, "id").filter(|v| !v.is_empty()) {
            targets.entry(id.to_string()).or_insert(node);
        }
    }
    for &node in &walk.order {
        if dom.is(node, "a")
            && let Some(name) = dom.attr(node, "name").filter(|v| !v.is_empty())
        {
            targets.entry(name.to_string()).or_insert(node);
        }
    }
    targets
}

/// C2-M-103: an anchor names an element by its fragment and is no note
/// reference or backlink that C2-M-062 and C2-M-063 read.
fn is_anchor(dom: &Dom, id: Id, targets: &HashMap<String, Id>) -> bool {
    dom.is(id, "a")
        && dom.attr(id, "role") != Some("doc-noteref")
        && !dom.has(id, "data-footnote-ref")
        && !is_backlink(dom, id)
        && href_fragment(dom, id).is_some_and(|f| targets.contains_key(f))
}

/// How many elements that an anchor's fragment names each element holds.
fn named_counts(dom: &Dom, walk: &Walk, named: &HashSet<Id>) -> HashMap<Id, usize> {
    let mut held: HashMap<Id, usize> = HashMap::new();
    for &node in walk.order.iter().rev() {
        let count = held.get(&node).copied().unwrap_or(0) + usize::from(named.contains(&node));
        held.insert(node, count);
        if let Some(parent) = dom.parent(node).filter(|&p| dom.is_element(p)) {
            *held.entry(parent).or_insert(0) += count;
        }
    }
    held
}

/// C2-M-103: the block of a named element, or its one-note wrapper.
fn block_of(dom: &Dom, target: Id, held: &HashMap<Id, usize>) -> Option<Id> {
    let mut node = target;
    while !tag_in(dom, node, BLOCKS) {
        node = dom
            .parent(node)
            .filter(|&p| dom.is_element(p) && !is_top_level(dom, p))?;
    }
    let wraps = dom.parent(node).filter(|&p| {
        dom.is_element(p)
            && tag_in(dom, p, WRAPPERS)
            && dom.attr(p, "id").is_some_and(|v| !v.is_empty())
            && held.get(&p) == Some(&1)
    });
    Some(wraps.unwrap_or(node))
}

/// C2-M-104: of two candidates inside each other's block, whether `first` is the reference.
fn wins(dom: &Dom, walk: &Walk, first: Id, second: Id) -> bool {
    let first_marked = is_marked_reference(dom, first);
    if first_marked != is_marked_reference(dom, second) {
        return first_marked;
    }
    let second_back = has_class(dom, second, &["footnote-back"]);
    if has_class(dom, first, &["footnote-back"]) != second_back {
        return second_back;
    }
    walk.at(first) < walk.at(second)
}

/// C2-M-104: the candidates that are references, each with its block.
fn references(
    dom: &Dom,
    walk: &Walk,
    anchors: &[Id],
    targets: &HashMap<String, Id>,
    by_href: &HashMap<String, Vec<usize>>,
) -> Vec<Candidate> {
    let target = |a: Id| href_fragment(dom, a).and_then(|f| targets.get(f).copied());
    let named: HashSet<Id> = anchors.iter().filter_map(|&a| target(a)).collect();
    let held = named_counts(dom, walk, &named);
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut slot: HashMap<Id, usize> = HashMap::new();
    for &anchor in anchors {
        let (Some(fragment), Some(named)) = (href_fragment(dom, anchor), target(anchor)) else {
            continue;
        };
        let Some(block) = block_of(dom, named, &held) else {
            continue;
        };
        if walk.contains(block, anchor) {
            continue;
        }
        let own = identity(dom, anchor);
        let mutual = !own.is_empty()
            && by_href
                .get(&format!("#{own}"))
                .is_some_and(|list| !walk.within(list, block).is_empty());
        if mutual || is_marked_reference(dom, anchor) {
            slot.insert(anchor, candidates.len());
            candidates.push(Candidate {
                anchor,
                block,
                fragment: fragment.to_string(),
            });
        }
    }
    // The first candidate among the anchors of a block that name an identity.
    let mut first: HashMap<(Id, String), Option<usize>> = HashMap::new();
    let mut keep = vec![true; candidates.len()];
    for (at, candidate) in candidates.iter().enumerate() {
        let own = identity(dom, candidate.anchor);
        if own.is_empty() {
            continue;
        }
        let inverse = *first
            .entry((candidate.block, own.to_string()))
            .or_insert_with(|| {
                let list = by_href
                    .get(&format!("#{own}"))
                    .map_or(&[][..], Vec::as_slice);
                walk.within(list, candidate.block)
                    .iter()
                    .find_map(|&p| walk.order.get(p).and_then(|n| slot.get(n)).copied())
            });
        if let Some(inverse) = inverse.and_then(|i| candidates.get(i))
            && walk.contains(inverse.block, candidate.anchor)
            && wins(dom, walk, inverse.anchor, candidate.anchor)
            && let Some(k) = keep.get_mut(at)
        {
            *k = false;
        }
    }
    candidates
        .into_iter()
        .zip(keep)
        .filter_map(|(c, k)| k.then_some(c))
        .collect()
}

/// C2-M-104: one note per block, in document order, without a block that contains another note.
fn notes_of(walk: &Walk, kept: Vec<Candidate>) -> Vec<Note> {
    let mut groups: Vec<Note> = Vec::new();
    let mut slot: HashMap<Id, usize> = HashMap::new();
    for candidate in kept {
        let at = *slot.entry(candidate.block).or_insert_with(|| {
            groups.push(Note {
                block: candidate.block,
                refs: Vec::new(),
                fragments: HashSet::new(),
            });
            groups.len() - 1
        });
        if let Some(group) = groups.get_mut(at) {
            group.refs.push(candidate.anchor);
            group.fragments.insert(candidate.fragment);
        }
    }
    groups.sort_by_key(|n| walk.at(n.block));
    // In document order a block that contains another note contains the next one.
    let blocks: Vec<Id> = groups.iter().map(|n| n.block).collect();
    groups
        .into_iter()
        .enumerate()
        .filter(|(at, note)| {
            blocks
                .get(at + 1)
                .is_none_or(|&next| !walk.contains(note.block, next))
        })
        .map(|(_, note)| note)
        .collect()
}

/// C2-M-104: every other anchor outside the notes whose fragment is that of a
/// reference binds to the same note.
fn attach_remaining(dom: &Dom, walk: &Walk, notes: &mut [Note], anchors: &[Id]) {
    let starts: Vec<usize> = notes.iter().map(|n| walk.at(n.block)).collect();
    let inside_note = |node: Id| {
        let slot = starts.partition_point(|&s| s <= walk.at(node));
        slot.checked_sub(1)
            .and_then(|s| notes.get(s))
            .is_some_and(|n| walk.contains(n.block, node))
    };
    let inside: Vec<bool> = anchors.iter().map(|&a| inside_note(a)).collect();
    let mut by_fragment: HashMap<String, usize> = HashMap::new();
    for (at, note) in notes.iter().enumerate() {
        for fragment in &note.fragments {
            by_fragment.entry(fragment.clone()).or_insert(at);
        }
    }
    let mut bound: HashSet<Id> = notes.iter().flat_map(|n| n.refs.iter().copied()).collect();
    for (&anchor, inside) in anchors.iter().zip(inside) {
        let Some(&at) = href_fragment(dom, anchor).and_then(|f| by_fragment.get(f)) else {
            continue;
        };
        if bound.contains(&anchor) || inside {
            continue;
        }
        if let Some(note) = notes.get_mut(at) {
            note.refs.push(anchor);
            bound.insert(anchor);
        }
    }
}

/// C2-M-102: the note pass of `word` and `google-docs`.
pub(crate) fn note_pass(dom: &mut Dom) {
    let walk = Walk::new(dom);
    let targets = fragment_targets(dom, &walk);
    let anchors: Vec<Id> = walk
        .order
        .iter()
        .copied()
        .filter(|&n| is_anchor(dom, n, &targets))
        .collect();
    if anchors.is_empty() {
        return;
    }
    let mut by_href: HashMap<String, Vec<usize>> = HashMap::new();
    for (at, &node) in walk.order.iter().enumerate() {
        if dom.is(node, "a")
            && let Some(href) = dom.attr(node, "href")
        {
            by_href.entry(href.to_string()).or_default().push(at);
        }
    }
    let kept = references(dom, &walk, &anchors, &targets, &by_href);
    let mut notes = notes_of(&walk, kept);
    if notes.is_empty() {
        return;
    }
    attach_remaining(dom, &walk, &mut notes, &anchors);
    rewrite(dom, &walk, &notes);
}

/// The items of the first list of each footnote section (C2-M-061).
fn section_items(dom: &Dom, walk: &Walk) -> HashSet<Id> {
    let mut items = HashSet::new();
    for &section in &walk.sections {
        if let Some(list) = dom.elements(section).into_iter().find(|&c| dom.is(c, "ol")) {
            items.extend(dom.elements(list).into_iter().filter(|&i| dom.is(i, "li")));
        }
    }
    items
}

fn rewrite(dom: &mut Dom, walk: &Walk, notes: &[Note]) {
    let mut edits = Edits::default();
    let items = section_items(dom, walk);
    let taken: HashSet<String> = items
        .iter()
        .filter_map(|&i| dom.attr(i, "id").filter(|v| !v.is_empty()))
        .map(|id| note_key(id).to_string())
        .collect();
    // C2-M-107: a note that is an item of a footnote section keeps its place and its id.
    let stays = |dom: &Dom, note: &Note| {
        items.contains(&note.block) && dom.attr(note.block, "id").is_some_and(|v| !v.is_empty())
    };
    let mut next = 1usize;
    let hrefs: Vec<String> = notes
        .iter()
        .map(|note| {
            if stays(dom, note) {
                return format!("#{}", dom.attr(note.block, "id").unwrap_or(""));
            }
            while taken.contains(&next.to_string()) {
                next += 1;
            }
            next += 1;
            format!("#fn{}", next - 1)
        })
        .collect();

    // C2-M-105: every reference is replaced before any note leaves, since a
    // note can hold the reference to another.
    for (at, (note, href)) in notes.iter().zip(&hrefs).enumerate() {
        for &anchor in &note.refs {
            let site = dom
                .parent(anchor)
                .filter(|&p| {
                    dom.is(p, "sup")
                        && dom
                            .children(p)
                            .iter()
                            .all(|&c| c == anchor || dom.is_blank(c))
                })
                .unwrap_or(anchor);
            let text = dom.push(NodeData {
                kind: Kind::Text((at + 1).to_string()),
                children: Vec::new(),
                parent: None,
                content: None,
                span: None,
            });
            let attrs = vec![
                ("role".to_string(), "doc-noteref".to_string()),
                ("href".to_string(), href.clone()),
            ];
            let replacement = element(dom, "a", attrs, vec![text]);
            Edits::replace(dom, site, replacement);
        }
    }
    for note in notes {
        drop_backlinks(dom, note, walk, &mut edits);
    }
    let moving: Vec<(Id, &str)> = notes
        .iter()
        .zip(&hrefs)
        .filter(|(note, _)| !stays(dom, note))
        .map(|(note, href)| (note.block, href.as_str()))
        .collect();
    let Some(&(first, _)) = moving.first() else {
        edits.finish(dom);
        return;
    };
    drop_separator(dom, first, &mut edits);

    let mut moved = Vec::new();
    let mut emptied: Vec<Id> = Vec::new();
    for &(block, href) in &moving {
        let parent = edits.parent_of(dom, block);
        edits.detach(dom, block);
        if let Some(p) = parent
            && !emptied.contains(&p)
        {
            emptied.push(p);
        }
        let children = edits.children(dom, block);
        let id = href.strip_prefix('#').unwrap_or(href).to_string();
        moved.push(element(dom, "li", vec![("id".into(), id)], children));
    }
    for parent in emptied {
        prune_empty(dom, parent, &mut edits);
    }
    edits.finish(dom);

    let list = element(dom, "ol", Vec::new(), moved);
    let section = element(
        dom,
        "section",
        vec![("role".into(), "doc-endnotes".into())],
        vec![list],
    );
    let body = find_body(dom);
    let mut children = dom.children(body).to_vec();
    children.push(section);
    dom.set_children(body, children);
    dom.set_parent(section, Some(body));
}

/// C2-M-106: a backlink names a reference of its note, or is the element a
/// reference's fragment names.
fn drop_backlinks(dom: &Dom, note: &Note, walk: &Walk, edits: &mut Edits) {
    let identities: HashSet<&str> = note
        .refs
        .iter()
        .map(|&r| identity(dom, r))
        .filter(|v| !v.is_empty())
        .collect();
    let range = walk.at(note.block)..=walk.end_of(note.block);
    let backlinks: Vec<Id> = walk
        .order
        .get(range)
        .unwrap_or(&[])
        .iter()
        .copied()
        .filter(|&n| {
            dom.is(n, "a")
                && dom.parent(n).is_some()
                && (href_fragment(dom, n).is_some_and(|f| identities.contains(f))
                    || (href_fragment(dom, n).is_some()
                        && note.fragments.contains(identity(dom, n)))
                    || has_class(dom, n, &["footnote-back"])
                    || is_backlink(dom, n))
        })
        .collect();
    for backlink in backlinks {
        let parent = edits.parent_of(dom, backlink);
        edits.detach(dom, backlink);
        if let Some(p) = parent
            && (dom.is(p, "sup") || dom.is(p, "span"))
            && edits.children(dom, p).iter().all(|&c| dom.is_blank(c))
        {
            edits.detach(dom, p);
        }
    }
}

/// C2-M-108: the rule and line breaks before the first note, on its level and
/// on each level above it.
fn drop_separator(dom: &Dom, first: Id, edits: &mut Edits) {
    let mut node = first;
    loop {
        let mut sibling = edits.before(dom, node);
        while let Some(s) = sibling.filter(|&s| is_layout(dom, s)) {
            let previous = edits.before(dom, s);
            if dom.is_element(s) {
                edits.detach(dom, s);
            }
            sibling = previous;
        }
        match edits.parent_of(dom, node) {
            Some(p) if sibling.is_none() && dom.is_element(p) && !is_top_level(dom, p) => node = p,
            _ => return,
        }
    }
}

/// C2-M-109: an element the notes left with layout only gives nothing, nor
/// its parent, up to the `<body>`.
fn prune_empty(dom: &Dom, start: Id, edits: &mut Edits) {
    let mut node = start;
    while dom.is_element(node)
        && !is_top_level(dom, node)
        && !edits.is_gone(node)
        && edits.children(dom, node).iter().all(|&c| is_layout(dom, c))
    {
        let Some(parent) = dom.parent(node) else {
            return;
        };
        edits.detach(dom, node);
        node = parent;
    }
}

fn find_body(dom: &Dom) -> Id {
    dom.children(dom.root)
        .iter()
        .copied()
        .filter(|&n| dom.is(n, "html"))
        .find_map(|html| dom.elements(html).into_iter().find(|&c| dom.is(c, "body")))
        .unwrap_or(dom.root)
}
