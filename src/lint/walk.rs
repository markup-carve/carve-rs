//! The one walk over the lint tree. The range index, the node rules and the control comments read
//! their nodes from it instead of walking the tree each.
//!
//! The walk keeps the stack order the range, node-rule and control walks used (a node, then its
//! children last first), so their records keep their order.

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

struct Entry<'a> {
    value: V<'a>,
    path: Option<&'a PathNode<'a>>,
    rules: Option<Rules<'a>>,
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
            pending: vec![Entry {
                value: tree,
                path: None,
                rules: Some(Rules { parents: None }),
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
        visit.walk
    }
}

impl<'a> Visit<'a, '_> {
    fn array(&mut self, entry: &Entry<'a>, items: crate::view::Arr<'a>) {
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
                self.push(item, entry.path, Segment::Index(index), entry.rules, place);
            }
        }
    }

    fn push(
        &mut self,
        value: V<'a>,
        parent: Option<&'a PathNode<'a>>,
        segment: Segment<'a>,
        rules: Option<Rules<'a>>,
        place: Place,
    ) {
        if !value.may_hold_nodes() || (!self.full && rules.is_none()) {
            return;
        }
        self.pending.push(Entry {
            value,
            path: Some(PathNode::child(self.arena, parent, segment)),
            rules,
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
        value.each(&mut |key, child| {
            if !child.is_container() {
                return;
            }
            let rules = entry
                .rules
                .zip(parents)
                .and_then(|(_, parents)| Self::child_rules(key, parents));
            let place = self.child_place(entry, key, own, child.is_array());
            self.push(child, entry.path, Segment::Key(key), rules, place);
        });
    }

    /// The place of a child under `key`. A path's holder is the node before its last `/children/` or
    /// `/title/`; the document's children have none.
    fn child_place(&mut self, entry: &Entry<'a>, key: &str, own: Option<u32>, list: bool) -> Place {
        if !self.full {
            return Place::default();
        }
        let title = key == "title" && list && own.is_some();
        if key != "children" && !title {
            return Place {
                holder: entry.place.holder,
                ..Place::default()
            };
        }
        let holder = if entry.path.is_some() {
            self.walk.holders.push(Holder {
                range: own,
                up: entry.place.holder,
                title,
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
