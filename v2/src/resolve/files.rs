//! The files of an expanded tree (C2-N-053 to C2-N-055): the definitions of
//! each file, the file that includes it, and the file of each reference, so a
//! reference looks in its own file first and then outward.

use crate::ast::{Document, Node};
use crate::hash::AddrMap;
use std::collections::HashMap;

/// The address of the node a `Node` points to, which stays put when a list
/// is rebuilt around it.
#[must_use]
pub fn inner(n: &Node<'_>) -> usize {
    match n {
        Node::FootnoteRef(x) => std::ptr::from_ref(&**x).addr(),
        Node::Footnote(x) => std::ptr::from_ref(&**x).addr(),
        Node::Link(x) => std::ptr::from_ref(&**x).addr(),
        Node::Image(x) => std::ptr::from_ref(&**x).addr(),
        Node::Abbreviation(x) => std::ptr::from_ref(&**x).addr(),
        _ => 0,
    }
}

#[derive(Default)]
struct File<'a> {
    id: Option<&'a str>,
    parent: Option<usize>,
    links: HashMap<&'a str, &'a str>,
    abbrs: HashMap<&'a str, &'a str>,
    notes: HashMap<&'a str, usize>,
}

/// File 0 is the top-level document.
#[derive(Default)]
pub struct Files<'a> {
    list: Vec<File<'a>>,
    of: AddrMap<usize, usize>,
    /// The file of each node with an authored id, by the address of its `pos`.
    ids: AddrMap<usize, usize>,
    /// The `pos` address of every node an included file produced.
    inside: crate::hash::AddrSet<usize>,
}

impl<'a> Files<'a> {
    /// Reads the files of `doc`, whose includes are expanded.
    #[must_use]
    pub fn build(doc: &Document<'a>) -> Self {
        let mut f = Files::default();
        f.add(&*doc.children, None, None);
        f
    }

    fn add(&mut self, top: &[Node<'a>], parent: Option<usize>, id: Option<&'a str>) {
        let mut file = File {
            id,
            parent,
            ..File::default()
        };
        for c in top {
            match c {
                Node::LinkReferenceDefinition(d) => {
                    file.links.entry(d.key).or_insert(d.href);
                }
                Node::AbbreviationDef(d) => {
                    file.abbrs.entry(d.key).or_insert(d.expansion);
                }
                Node::Footnote(d) => {
                    file.notes.entry(d.key).or_insert(inner(c));
                }
                _ => {}
            }
        }
        let at = self.list.len();
        self.list.push(file);
        for c in top {
            self.walk(c, at, 1);
        }
    }

    fn walk(&mut self, n: &Node<'a>, file: usize, depth: usize) {
        if depth > super::WALK_CAP {
            return;
        }
        if let Node::Include(i) = n {
            if let Some(children) = i.children.as_deref() {
                self.add(children, Some(file), i.file);
            }
            return;
        }
        if let Some(p) = n.pos() {
            let at = std::ptr::from_ref(p).addr();
            if n.attrs().is_some_and(|a| a.id.is_some()) {
                self.ids.insert(at, file);
            }
            if file > 0 {
                self.inside.insert(at);
            }
        }
        if matches!(
            n,
            Node::FootnoteRef(_) | Node::Link(_) | Node::Image(_) | Node::Abbreviation(_)
        ) {
            self.of.insert(inner(n), file);
        }
        for c in super::order::slots(n) {
            self.walk(c, file, depth + 1);
        }
    }

    fn chain<T>(&self, n: &Node<'_>, get: impl Fn(&File<'a>) -> Option<T>) -> Option<T> {
        let mut at = self.of.get(&inner(n)).copied();
        if at.is_none() {
            at = Some(0);
        }
        while let Some(i) = at {
            let file = self.list.get(i)?;
            if let Some(v) = get(file) {
                return Some(v);
            }
            at = file.parent;
        }
        None
    }

    /// C2-N-063: the id of the included file a node with an authored id
    /// stands in, by the address of its `pos`; `None` at the top level.
    #[must_use]
    pub fn file_at(&self, pos: usize) -> Option<&'a str> {
        self.list.get(*self.ids.get(&pos)?)?.id
    }

    /// True when an included file produced `n`, whose offsets count in that file.
    #[must_use]
    pub fn included(&self, n: &Node<'_>) -> bool {
        n.pos()
            .is_some_and(|p| self.inside.contains(&std::ptr::from_ref(p).addr()))
    }

    /// C2-N-054: the destination of a link key, own file first, then outward.
    #[must_use]
    pub fn link(&self, n: &Node<'_>, key: &str) -> Option<&'a str> {
        self.chain(n, |f| f.links.get(key).copied())
    }

    /// C2-N-055: the expansion of an abbreviation.
    #[must_use]
    pub fn abbr(&self, n: &Node<'_>, key: &str) -> Option<&'a str> {
        self.chain(n, |f| f.abbrs.get(key).copied())
    }

    /// C2-N-055: the definition a footnote reference reaches, by [`inner`] address.
    #[must_use]
    pub fn note(&self, n: &Node<'_>, key: &str) -> Option<usize> {
        self.chain(n, |f| f.notes.get(key).copied())
    }
}
