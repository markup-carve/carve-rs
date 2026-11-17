//! Include expansion (`includes.md`, C2-N): the `includes` option, the
//! resolver a host gives, and the expansion of the include nodes of a parsed
//! tree, which runs after the parse and before resolution (C2-N-006).

mod code;
mod expand;
pub mod fs;
mod lines;
mod section;
mod shift;

pub(crate) use expand::{expand, holds_blocks, no_resolver};

use std::sync::Arc;

/// C2-N-007: the error causes a resolver returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    NotFound,
    OutsideRoot,
    Unreadable,
    NonText,
}

impl Cause {
    /// The name of the cause in `data/includes.json`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::OutsideRoot => "outside_root",
            Self::Unreadable => "unreadable",
            Self::NonText => "non_text",
        }
    }
}

/// C2-N-007: what a resolver receives for one directive.
#[derive(Debug, Clone, Copy)]
pub struct Request<'r> {
    /// The decoded path of the directive (C2-B-202).
    pub path: &'r str,
    pub section: Option<&'r str>,
    /// The decoded valid options (C2-A-181), an object.
    pub options: &'r serde_json::Value,
    /// The id of the including file; `None` for a top-level document without one.
    pub from: Option<&'r str>,
}

/// C2-N-007, C2-N-008: the whole target as bytes and its id (C2-N-009), or a cause.
pub type Resolver = Arc<dyn Fn(&Request<'_>) -> Result<(Vec<u8>, String), Cause> + Send + Sync>;

/// The value of the `includes` option (C2-N-003).
#[derive(Clone)]
pub struct Includes {
    pub resolver: Resolver,
    /// C2-N-066: the id of the top-level document, which a cycle can return to.
    pub file: Option<String>,
    /// C2-N-064: lowered, never raised, past `include_depth`.
    pub depth: usize,
    /// C2-N-065: lowered, never raised, past `include_bytes`.
    pub bytes: usize,
}

impl Includes {
    /// A resolver with the limits of `data/limits.json`.
    #[must_use]
    pub fn new(resolver: Resolver) -> Self {
        Self {
            resolver,
            file: None,
            depth: crate::tables::INCLUDE_DEPTH,
            bytes: crate::tables::INCLUDE_BYTES,
        }
    }

    fn depth_limit(&self) -> usize {
        self.depth.min(crate::tables::INCLUDE_DEPTH)
    }

    fn bytes_limit(&self) -> usize {
        self.bytes.min(crate::tables::INCLUDE_BYTES)
    }
}

impl std::fmt::Debug for Includes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Includes")
            .field("file", &self.file)
            .field("depth", &self.depth)
            .field("bytes", &self.bytes)
            .finish_non_exhaustive()
    }
}

/// One dependency of a document (C2-N-083): the path as written, the
/// including file, and the id when the file was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub path: String,
    pub from: Option<String>,
    pub file: Option<String>,
}

use crate::Arena;
use crate::ast::{Document, Node};

fn take<'a>(n: &mut Node<'a>, arena: &'a Arena) -> Node<'a> {
    std::mem::replace(
        n,
        Node::SoftBreak(arena.alloc(crate::ast::SoftBreak::default())),
    )
}

/// C2-N-075: an expanded include renders its children in its place, so the
/// rules that read the blocks of a container read them there.
pub(crate) fn flatten<'a>(doc: &mut Document<'a>, arena: &'a Arena) {
    if let Some(new) = flatten_list(doc.children, arena) {
        doc.children = new;
    }
}

fn flatten_list<'a>(nodes: &mut [Node<'a>], arena: &'a Arena) -> Option<&'a mut [Node<'a>]> {
    let mut any = false;
    for n in nodes.iter_mut() {
        if let Node::Include(i) = n {
            if let Some(children) = i.children.as_deref_mut() {
                any = true;
                if let Some(new) = flatten_list(children, arena) {
                    i.children = Some(new);
                }
            }
        } else {
            flatten_node(n, arena);
        }
    }
    if !any {
        return None;
    }
    let mut out = Vec::with_capacity(nodes.len());
    for n in nodes.iter_mut() {
        match n {
            Node::Include(i) if i.children.is_some() => {
                let kids = i.children.take().unwrap_or_default();
                out.extend(kids.iter_mut().map(|k| take(k, arena)));
            }
            _ => out.push(take(n, arena)),
        }
    }
    Some(arena.nodes(&mut out))
}

fn flatten_node<'a>(n: &mut Node<'a>, arena: &'a Arena) {
    if matches!(n, Node::List(_) | Node::DefinitionList(_)) {
        n.for_each_child_mut(&mut |c| flatten_node(c, arena));
        return;
    }
    if !expand::holds_blocks(n) {
        return;
    }
    let new = n.children_mut().and_then(|c| flatten_list(c, arena));
    if let Some(new) = new {
        n.set_children(new);
    }
}

/// C2-N-079: one `include-expanded-roundtrip` for each expanded include that
/// no expanded include holds.
pub(crate) fn roundtrip_losses(nodes: &[Node<'_>]) -> Vec<&'static str> {
    let mut out = Vec::new();
    count(nodes, &mut out);
    out
}

fn count(nodes: &[Node<'_>], out: &mut Vec<&'static str>) {
    for n in nodes {
        if let Node::Include(i) = n {
            if i.children.is_some() {
                out.push("include-expanded-roundtrip");
            }
        } else if expand::holds_blocks(n) || matches!(n, Node::List(_) | Node::DefinitionList(_)) {
            n.for_each_child(&mut |c| count(std::slice::from_ref(c), out));
        }
    }
}

/// C2-N-019: every positioned node an included file produced carries `pos.file`.
pub(crate) fn stamp_files(v: &mut serde_json::Value, file: Option<&str>) {
    match v {
        serde_json::Value::Object(m) => {
            if let (Some(f), Some(serde_json::Value::Object(p))) = (file, m.get_mut("pos")) {
                p.insert("file".into(), f.into());
            }
            let inner = (m.get("type").and_then(serde_json::Value::as_str) == Some("include"))
                .then(|| {
                    m.get("file")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                })
                .flatten();
            for (k, c) in m.iter_mut() {
                if k == "pos" {
                    continue;
                }
                let f = if k == "children" && inner.is_some() {
                    inner.as_deref()
                } else {
                    file
                };
                stamp_files(c, f);
            }
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(|c| stamp_files(c, file)),
        _ => {}
    }
}
