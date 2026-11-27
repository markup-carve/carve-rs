//! The files of an expanded tree (C2-N-053 to C2-N-055): the definitions of
//! each file, the file that includes it, and the file of each reference, so a
//! reference looks in its own file first and then outward.

use crate::ast::{Document, Node};
use crate::diagnostic::IncludeSite;
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
    /// The include node that read this file (C2-N-087).
    site: Option<IncludeSite>,
}

/// A snippet copy whose include the walk has not reached: the copy, the include and the depth.
type Later<'r, 'a> = (&'r Node<'a>, usize, usize);

/// File 0 is the top-level document.
#[derive(Default)]
pub struct Files<'a> {
    list: Vec<File<'a>>,
    of: AddrMap<usize, usize>,
    /// The file of each node with an authored id, by the address of its `pos`.
    ids: AddrMap<usize, usize>,
    /// The `pos` address of every node an included file produced.
    inside: crate::hash::AddrSet<usize>,
    of_pos: AddrMap<usize, usize>,
    sources: HashMap<&'a str, &'a str>,
    /// C2-N-087: each position of an included file and the file that reached it, in document order.
    reaches: Vec<(&'a str, usize, usize, usize)>,
    /// The file of each expanded include, by [`Node::payload_addr`].
    includes: AddrMap<usize, usize>,
}

impl<'a> Files<'a> {
    /// Reads the files of `doc`, whose includes are expanded.
    #[must_use]
    pub fn build(doc: &Document<'a>, sources: HashMap<&'a str, &'a str>) -> Self {
        let mut f = Files {
            sources,
            ..Files::default()
        };
        let mut later = Vec::new();
        f.add(&*doc.children, None, None, None, &mut later);
        // C2-N-092: a snippet copy that stands before the include of its definition.
        while let Some((n, include, depth)) = later.pop() {
            let file = f.includes.get(&include).copied().unwrap_or(0);
            f.walk_in(n, file, depth, &mut Vec::new());
        }
        f
    }

    fn add<'r>(
        &mut self,
        top: &'r [Node<'a>],
        parent: Option<usize>,
        id: Option<&'a str>,
        site: Option<IncludeSite>,
        later: &mut Vec<Later<'r, 'a>>,
    ) {
        let mut file = File {
            id,
            parent,
            site,
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
            self.walk(c, at, 1, later);
        }
    }

    fn walk<'r>(
        &mut self,
        n: &'r Node<'a>,
        file: usize,
        depth: usize,
        later: &mut Vec<Later<'r, 'a>>,
    ) {
        if depth > super::WALK_CAP {
            return;
        }
        // C2-N-092: a snippet copy resolves against the file of its definition.
        if let Some(include) = crate::snippet::expand::copy_scope(n) {
            if include == 0 {
                return self.walk_in(n, 0, depth, later);
            }
            if let Some(&at) = self.includes.get(&include) {
                return self.walk_in(n, at, depth, later);
            }
            later.push((n, include, depth));
            return;
        }
        self.walk_in(n, file, depth, later);
    }

    fn walk_in<'r>(
        &mut self,
        n: &'r Node<'a>,
        file: usize,
        depth: usize,
        later: &mut Vec<Later<'r, 'a>>,
    ) {
        let id = self.list.get(file).and_then(|f| f.id);
        if let Some((id, p)) = id.zip(n.pos()) {
            self.reaches.push((id, p.start(), p.end(), file));
        }
        if let Node::Include(i) = n {
            if let Some(children) = i.children.as_deref() {
                let site = i.pos.map(|pos| IncludeSite {
                    file: id.map(str::to_string),
                    line: pos.start_line,
                    column: pos.start_column,
                    start_offset: pos.start(),
                    end_offset: pos.end(),
                });
                self.includes.insert(n.payload_addr(), self.list.len());
                self.add(children, Some(file), i.file, site, later);
            }
            return;
        }
        if let Some(p) = n.pos() {
            let at = std::ptr::from_ref(p).addr();
            self.of_pos.insert(at, file);
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
            self.walk(c, file, depth + 1, later);
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

    /// C2-N-087: the include chain of the file instance at `pos`, outermost first.
    #[must_use]
    pub fn chain_at(&self, pos: usize) -> Vec<IncludeSite> {
        self.chain_of(self.ids.get(&pos).copied())
    }

    /// The included file that owns this exact AST position.
    #[must_use]
    pub fn file_of_pos(&self, pos: &crate::ast::Pos) -> Option<&'a str> {
        self.list
            .get(*self.of_pos.get(&std::ptr::from_ref(pos).addr())?)?
            .id
    }

    /// C2-N-087: the include chain of the first reach of each wanted position, outermost first.
    pub fn included_by<'k>(
        &self,
        wanted: impl Iterator<Item = (&'k str, usize, usize)>,
    ) -> HashMap<(&'k str, usize, usize), Vec<IncludeSite>>
    where
        'a: 'k,
    {
        let mut first: HashMap<(&'k str, usize, usize), Option<usize>> =
            wanted.map(|k| (k, None)).collect();
        if first.is_empty() {
            return HashMap::new();
        }
        for &(id, start, end, file) in &self.reaches {
            if let Some(slot @ None) = first.get_mut(&(id, start, end)) {
                *slot = Some(file);
            }
        }
        first
            .into_iter()
            .map(|(k, at)| (k, self.chain_of(at)))
            .collect()
    }

    fn chain_of(&self, mut at: Option<usize>) -> Vec<IncludeSite> {
        let mut chain = Vec::new();
        while let Some(i) = at.filter(|&i| i > 0) {
            let Some(f) = self.list.get(i) else { break };
            chain.extend(f.site.clone());
            at = f.parent;
        }
        chain.reverse();
        chain
    }

    /// The accepted source texts of expanded included files.
    pub fn sources(&self) -> impl Iterator<Item = (&'a str, &'a str)> + '_ {
        self.sources.iter().map(|(id, text)| (*id, *text))
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
