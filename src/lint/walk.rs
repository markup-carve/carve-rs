//! The one walk over the lint tree. The range index, the node rules, the ported 0.1 rules and the
//! control comments read their nodes from it instead of walking the tree each.
//!
//! The walk keeps the stack order the range, node-rule and control walks used (a node, then its
//! children last first), so their records keep their order. The 0.1 rules read a forward preorder
//! that stops at untyped objects and nested arrays; it is rebuilt from the parent links.

use super::legacy;
use super::nodes::{self, Item};
use super::{Holder, LazyPath, Range};
use crate::Arena;
use crate::view::V;
use std::cell::OnceCell;

#[derive(Clone, Copy)]
pub(super) enum Segment<'a> {
    Key(&'a str),
    Index(usize),
}

pub(super) struct PathNode<'a> {
    pub(super) segment: Segment<'a>,
    pub(super) len: usize,
    pub(super) previous: Option<&'a PathNode<'a>>,
}

impl<'a> PathNode<'a> {
    fn child(arena: &'a Arena, parent: Option<&'a Self>, segment: Segment<'a>) -> &'a Self {
        let width = match segment {
            Segment::Key(key) => key.len(),
            Segment::Index(index) => index
                .checked_ilog10()
                .map_or(1, |digits| digits as usize + 1),
        };
        arena.alloc(Self {
            len: parent.map_or(0, |node| node.len) + 1 + width,
            segment,
            previous: parent,
        })
    }
}

/// The JSON pointer of a path, `""` for the root.
pub(super) fn path_text(path: Option<&PathNode<'_>>) -> String {
    use std::fmt::Write as _;
    let mut segments = Vec::new();
    let mut current = path;
    while let Some(node) = current {
        segments.push(node.segment);
        current = node.previous;
    }
    let mut text = String::with_capacity(path.map_or(0, |node| node.len));
    for segment in segments.into_iter().rev() {
        text.push('/');
        match segment {
            Segment::Key(key) => text.push_str(key),
            Segment::Index(index) => {
                let _ = write!(text, "{index}");
            }
        }
    }
    text
}

pub(super) struct Walk<'a> {
    pub(super) ranges: Vec<Range<'a>>,
    pub(super) items: Vec<Item<'a>>,
    /// The 0.1 nodes in walk order: a node's subtree follows it, its parent precedes it.
    pub(super) legacy: Vec<legacy::Node<'a>>,
    /// Indices into `legacy` in document order, the order the 0.1 rules visit.
    pub(super) legacy_order: Vec<u32>,
    /// The line comments, the candidates for lint controls.
    pub(super) comments: Vec<V<'a>>,
    /// The nodes that hold `children`, which the ranges point into.
    pub(super) holders: Vec<Holder>,
    /// The node of each range, for the `node` field of the data rules.
    pub(super) values: Vec<V<'a>>,
}

/// What the node rules carry down; `None` below `data`, `props`, `pos` and `segments`.
#[derive(Clone, Copy)]
struct Rules<'a> {
    parents: Option<&'a nodes::Parent<'a>>,
}

/// Whether the 0.1 walk reaches a value: as a node under its parent's key, or as an array whose
/// items it reaches.
#[derive(Clone, Copy)]
enum Old {
    No,
    Node(Option<usize>, legacy::Slot),
    Items(usize, legacy::Slot),
}

struct Entry<'a> {
    value: V<'a>,
    path: Option<&'a PathNode<'a>>,
    rules: Option<Rules<'a>>,
    old: Old,
    place: Place,
}

/// Where a value sits for the data rules: its holder, its index in a `children` list and that list.
#[derive(Clone, Copy, Default)]
struct Place {
    holder: Option<u32>,
    index: Option<u32>,
    /// The `children` list this value is an item of, as an index into `Visit::lists`.
    list: Option<usize>,
    /// The value is a `children` list.
    children: bool,
}

struct Visit<'a, 's> {
    arena: &'a Arena,
    source: &'s str,
    full: bool,
    walk: Walk<'a>,
    /// The 0.1 nodes in walk order, with their parent's index in this list.
    old: Vec<legacy::Node<'a>>,
    pending: Vec<Entry<'a>>,
    /// Per `children` list: the item index and range of the item recorded last. Items are met last
    /// to first.
    lists: Vec<Option<(u32, u32)>>,
}

impl<'a> Walk<'a> {
    pub(super) const fn empty() -> Self {
        Self {
            ranges: Vec::new(),
            items: Vec::new(),
            legacy: Vec::new(),
            legacy_order: Vec::new(),
            comments: Vec::new(),
            holders: Vec::new(),
            values: Vec::new(),
        }
    }

    /// Every record of `tree`.
    pub(super) fn new(tree: V<'a>, arena: &'a Arena, source: &str) -> Self {
        Self::run(tree, arena, source, true)
    }

    /// Only the node-rule items of `tree`, for the alternate heading-id tree.
    pub(super) fn items(tree: V<'a>, arena: &'a Arena) -> Vec<Item<'a>> {
        Self::run(tree, arena, "", false).items
    }

    fn run(tree: V<'a>, arena: &'a Arena, source: &str, full: bool) -> Self {
        let mut visit = Visit {
            arena,
            source,
            full,
            walk: Self::empty(),
            old: Vec::new(),
            pending: vec![Entry {
                value: tree,
                path: None,
                rules: Some(Rules { parents: None }),
                old: if full && source.contains('#') {
                    Old::Node(None, legacy::Slot::Other)
                } else {
                    Old::No
                },
                place: Place::default(),
            }],
            lists: Vec::new(),
        };
        while let Some(entry) = visit.pending.pop() {
            if let Some(items) = entry.value.as_array() {
                visit.array(&entry, items);
            } else if entry.value.is_object() {
                visit.object(&entry);
            }
        }
        let mut walk = visit.walk;
        walk.legacy_order = preorder(&visit.old);
        walk.legacy = visit.old;
        walk
    }
}

impl<'a> Visit<'a, '_> {
    fn array(&mut self, entry: &Entry<'a>, items: crate::view::Arr<'a>) {
        let old = match entry.old {
            Old::Items(parent, slot) => Old::Node(Some(parent), slot),
            _ => Old::No,
        };
        let list = (self.full && entry.place.children).then(|| {
            self.lists.push(None);
            self.lists.len() - 1
        });
        for (index, item) in items.iter().enumerate() {
            if item.is_container() {
                let place = Place {
                    holder: entry.place.holder,
                    index: u32::try_from(index).ok(),
                    list,
                    children: entry.place.children,
                };
                self.push(
                    item,
                    entry.path,
                    Segment::Index(index),
                    entry.rules,
                    old,
                    place,
                );
            }
        }
    }

    fn push(
        &mut self,
        value: V<'a>,
        parent: Option<&'a PathNode<'a>>,
        segment: Segment<'a>,
        rules: Option<Rules<'a>>,
        old: Old,
        place: Place,
    ) {
        if !value.may_hold_nodes() || (!self.full && rules.is_none()) {
            return;
        }
        self.pending.push(Entry {
            value,
            path: Some(PathNode::child(self.arena, parent, segment)),
            rules,
            old,
            place,
        });
    }

    fn object(&mut self, entry: &Entry<'a>) {
        let value = entry.value;
        let kind = value.ty().as_str();
        let own = if self.full {
            self.record(entry, kind)
        } else {
            None
        };
        let parents = entry.rules.map(|rules| self.item(entry, kind, rules));
        let old = match (entry.old, kind) {
            (Old::Node(parent, slot), Some(_)) => {
                self.old.push(legacy::Node {
                    value,
                    parent,
                    slot,
                });
                Some(self.old.len() - 1)
            }
            _ => None,
        };
        value.each(&mut |key, child| {
            if !child.is_container() {
                return;
            }
            let rules = entry
                .rules
                .zip(parents)
                .and_then(|(_, parents)| Self::child_rules(key, parents));
            let old = old.map_or(Old::No, |at| old_child(at, key, child));
            let place = self.child_place(entry, key, own);
            self.push(child, entry.path, Segment::Key(key), rules, old, place);
        });
    }

    /// The place of a child under `key`. A path's holder is the node before its last `/children/`;
    /// the document's children have none.
    fn child_place(&mut self, entry: &Entry<'a>, key: &str, own: Option<u32>) -> Place {
        if !self.full {
            return Place::default();
        }
        if key != "children" {
            return Place {
                holder: entry.place.holder,
                ..Place::default()
            };
        }
        let holder = if entry.path.is_some() {
            self.walk.holders.push(Holder {
                range: own,
                up: entry.place.holder,
            });
            u32::try_from(self.walk.holders.len() - 1).ok()
        } else {
            None
        };
        Place {
            holder,
            index: None,
            list: None,
            children: true,
        }
    }

    /// The range of a positioned node and the line comment candidates; the new range's index.
    fn record(&mut self, entry: &Entry<'a>, kind: Option<&'a str>) -> Option<u32> {
        let value = entry.value;
        let kind = kind?;
        let mut own = None;
        if let Some(pos) = value.pos() {
            let number = |key: &str| match pos {
                V::Pos(pos) => pos_number(pos, key),
                _ => usize::try_from(pos.at(key).as_u64().unwrap_or(0)).unwrap_or(0),
            };
            own = u32::try_from(self.walk.ranges.len()).ok();
            self.walk.ranges.push(Range {
                start: number("startOffset"),
                end: number("endOffset"),
                path: LazyPath {
                    node: entry.path,
                    text: OnceCell::new(),
                },
                kind,
                start_line: number("startLine"),
                end_line: number("endLine"),
                has_attrs: value.get("attrs").is_some(),
                footnote_key: (kind == "footnote_ref")
                    .then(|| value.at("key").as_str())
                    .flatten(),
                form_block: value.at("form") == "block",
                image_line: kind == "paragraph"
                    && value
                        .at("children")
                        .as_array()
                        .and_then(crate::view::Arr::single)
                        .is_some_and(|child| child.ty().as_str() == Some("image")),
                holder: entry.place.holder,
                index: entry.place.index.unwrap_or(0),
                prev: None,
                joined: false,
            });
            self.walk.values.push(value);
            if let (Some(slot), Some(at), Some(made)) = (
                entry.place.list.and_then(|list| self.lists.get_mut(list)),
                entry.place.index,
                own,
            ) {
                super::link_prev(slot, at, made, &mut self.walk.ranges);
            }
        }
        if kind == "comment" && value.at("form") == "line" {
            self.walk.comments.push(value);
        }
        own
    }

    /// Records a typed node for the node rules; the parent chain its children carry.
    fn item(
        &mut self,
        entry: &Entry<'a>,
        kind: Option<&'a str>,
        rules: Rules<'a>,
    ) -> Option<&'a nodes::Parent<'a>> {
        let Some(kind) = kind else {
            return rules.parents;
        };
        let bounds = entry.value.offsets();
        let raw = bounds
            .and_then(|(start, end)| self.source.get(start..end))
            .unwrap_or("");
        if !self.full || nodes::needs_hand_node(kind, raw) {
            self.walk.items.push(Item {
                node: entry.value,
                bounds,
                path: OnceCell::new(),
                path_node: entry.path,
                parents: rules.parents,
            });
        }
        Some(&*self.arena.alloc(nodes::Parent {
            kind,
            depth: rules.parents.map_or(0, |parent| parent.depth + 1),
            previous: rules.parents,
        }))
    }

    fn child_rules(key: &str, parents: Option<&'a nodes::Parent<'a>>) -> Option<Rules<'a>> {
        if matches!(key, "data" | "props" | "pos" | "segments") {
            return None;
        }
        Some(Rules { parents })
    }
}

fn pos_number(pos: &crate::ast::Pos, key: &str) -> usize {
    usize::try_from(match key {
        "startOffset" => pos.start_offset,
        "endOffset" => pos.end_offset,
        "startLine" => pos.start_line,
        _ => pos.end_line,
    })
    .unwrap_or(0)
}

fn old_child<'a>(at: usize, key: &'a str, child: V<'a>) -> Old {
    if matches!(key, "data" | "props" | "pos" | "attrs") {
        Old::No
    } else if child.is_array() {
        Old::Items(at, legacy::Slot::of(key))
    } else if child.is_object() {
        Old::Node(Some(at), legacy::Slot::of(key))
    } else {
        Old::No
    }
}

/// The indices of the 0.1 nodes in forward preorder. The walk meets the children of a node last
/// first, so prepending each to its parent's child list restores their order.
fn preorder(nodes: &[legacy::Node<'_>]) -> Vec<u32> {
    const NONE: u32 = u32::MAX;
    let index = |at: usize| u32::try_from(at).unwrap_or(NONE);
    let mut first_child = vec![NONE; nodes.len()];
    let mut next_sibling = vec![NONE; nodes.len()];
    for (at, node) in nodes.iter().enumerate() {
        if let Some(first) = node.parent.and_then(|parent| first_child.get_mut(parent)) {
            if let Some(next) = next_sibling.get_mut(at) {
                *next = *first;
            }
            *first = index(at);
        }
    }
    let link = |links: &[u32], at: usize| {
        links
            .get(at)
            .copied()
            .filter(|next| *next != NONE)
            .map(|next| next as usize)
    };
    let mut order = Vec::with_capacity(nodes.len());
    let mut current = (!nodes.is_empty()).then_some(0);
    while let Some(at) = current {
        order.push(index(at));
        current = link(&first_child, at).or_else(|| {
            let mut up = Some(at);
            while let Some(node) = up {
                if let Some(next) = link(&next_sibling, node) {
                    return Some(next);
                }
                up = nodes.get(node).and_then(|n| n.parent);
            }
            None
        });
    }
    order
}
