//! The flat event list of one content: what the scan settled, in source
//! order, for the builder to turn into a tree.

use super::token::{K, Kind, Tok};
use crate::ast::Node;
use crate::attr::AttrBlock;

#[derive(Debug, Clone, PartialEq)]
pub enum Ev {
    /// A container opens; `v0..v1` is its opening markup.
    Enter {
        kind: Kind,
        v0: usize,
        v1: usize,
    },
    /// The container closes; `v0..v1` is its closing markup with attached blocks.
    /// `note` marks the end of an inline footnote.
    Exit {
        v0: usize,
        v1: usize,
        attrs: Option<AttrBlock>,
        note: bool,
    },
    /// Text; `val` is `None` when it is the content slice. `plain` text is
    /// open to smart typography and abbreviations.
    Text {
        v0: usize,
        v1: usize,
        val: Option<String>,
        plain: bool,
    },
    Nbsp {
        v0: usize,
        v1: usize,
    },
    Break {
        v0: usize,
        v1: usize,
        hard: bool,
    },
    CellJoin {
        v0: usize,
        v1: usize,
    },
    Leaf {
        v0: usize,
        v1: usize,
        node: Box<Node>,
        transparent: bool,
    },
}

/// Turns the settled tokens into events; plain text runs that touch merge.
#[must_use]
pub fn from_tokens(toks: Vec<Tok>) -> Vec<Ev> {
    let notes: Vec<bool> = toks
        .iter()
        .map(|t| {
            matches!(
                t.k,
                K::Open {
                    kind: Kind::InlineFootnote,
                    ..
                }
            )
        })
        .collect();
    let mut out: Vec<Ev> = Vec::with_capacity(toks.len());
    for t in toks {
        let Tok { v0, v1, k } = t;
        let ev = match k {
            K::Skip => continue,
            K::Text { val, plain } => Ev::Text { v0, v1, val, plain },
            K::Nbsp => Ev::Nbsp { v0, v1 },
            K::Break { hard } => Ev::Break { v0, v1, hard },
            K::CellJoin => Ev::CellJoin { v0, v1 },
            K::Leaf {
                node, transparent, ..
            } => Ev::Leaf {
                v0,
                v1,
                node,
                transparent,
            },
            K::Open { kind, .. } => Ev::Enter { kind, v0, v1 },
            K::Close { attrs, open } => {
                let note = notes.get(open).copied().unwrap_or(false);
                Ev::Exit {
                    v0,
                    v1,
                    attrs: attrs.map(|a| *a),
                    note,
                }
            }
            K::Attr(_) | K::Delim(_) | K::Pending | K::CaptionMark => Ev::Text {
                v0,
                v1,
                val: None,
                plain: true,
            },
        };
        push(&mut out, ev);
    }
    out
}

fn push(out: &mut Vec<Ev>, ev: Ev) {
    if let Ev::Text {
        v0,
        v1,
        val: None,
        plain: true,
    } = ev
        && let Some(Ev::Text {
            v1: last,
            val: None,
            plain: true,
            ..
        }) = out.last_mut()
        && *last == v0
    {
        *last = v1;
        return;
    }
    out.push(ev);
}
