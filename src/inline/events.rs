//! The flat event list of one content: what the scan settled, in source
//! order, for the builder to turn into a tree.

use super::token::{K, Kind, Tok};
use crate::ast::Node;
use crate::attr::AttrBlock;

#[derive(Debug, PartialEq)]
pub enum Ev<'a> {
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
        attrs: Option<Box<AttrBlock>>,
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
        node: Node<'a>,
        transparent: bool,
    },
}

/// Turns the settled tokens into events (into `out`, `toks` emptied);
/// plain text runs that touch merge.
pub fn from_tokens<'a>(toks: &mut Vec<Tok<'a>>, out: &mut Vec<Ev<'a>>) {
    out.clear();
    out.reserve(toks.len());
    out.extend(Stream::new(toks));
}

/// The events of the settled tokens, read as they are made (`toks` emptied).
pub struct Stream<'t, 'a> {
    toks: std::vec::Drain<'t, Tok<'a>>,
    /// Per token, whether it opens an inline footnote; empty when none does.
    notes: Vec<bool>,
    /// The plain text run read so far, not given out yet.
    run: Option<(usize, usize)>,
    /// The event behind that run.
    held: Option<Ev<'a>>,
}

const fn plain_text<'a>(v0: usize, v1: usize) -> Ev<'a> {
    Ev::Text {
        v0,
        v1,
        val: None,
        plain: true,
    }
}

impl<'t, 'a> Stream<'t, 'a> {
    pub fn new(toks: &'t mut Vec<Tok<'a>>) -> Self {
        let note = |t: &Tok<'_>| {
            matches!(
                t.k,
                K::Open {
                    kind: Kind::InlineFootnote,
                    ..
                }
            )
        };
        // Only inline footnotes need the opener lookup of each closer.
        let notes = if toks.iter().any(note) {
            toks.iter().map(note).collect()
        } else {
            Vec::new()
        };
        Self {
            toks: toks.drain(..),
            notes,
            run: None,
            held: None,
        }
    }

    /// The event of a token that is not plain text; `None` for a skipped one.
    fn event(&self, t: Tok<'a>) -> Option<Ev<'a>> {
        let Tok { v0, v1, k } = t;
        Some(match k {
            K::Skip => return None,
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
            K::Close { attrs, open } => Ev::Exit {
                v0,
                v1,
                attrs,
                note: self.notes.get(open).copied().unwrap_or(false),
            },
            K::Attr(_) | K::Delim(_) | K::Pending | K::CaptionMark => plain_text(v0, v1),
        })
    }
}

impl<'a> Iterator for Stream<'_, 'a> {
    type Item = Ev<'a>;

    fn next(&mut self) -> Option<Ev<'a>> {
        if let Some(ev) = self.held.take() {
            return Some(ev);
        }
        loop {
            let Some(t) = self.toks.next() else {
                return self.run.take().map(|(a, b)| plain_text(a, b));
            };
            let plain = matches!(
                t.k,
                K::Text {
                    val: None,
                    plain: true
                } | K::Attr(_)
                    | K::Delim(_)
                    | K::Pending
                    | K::CaptionMark
            );
            if plain {
                // Plain text runs that touch merge.
                match self.run {
                    Some((a, b)) if b == t.v0 => self.run = Some((a, t.v1)),
                    Some((a, b)) => {
                        self.run = Some((t.v0, t.v1));
                        return Some(plain_text(a, b));
                    }
                    None => self.run = Some((t.v0, t.v1)),
                }
                continue;
            }
            let Some(ev) = self.event(t) else { continue };
            if let Some((a, b)) = self.run.take() {
                self.held = Some(ev);
                return Some(plain_text(a, b));
            }
            return Some(ev);
        }
    }
}
