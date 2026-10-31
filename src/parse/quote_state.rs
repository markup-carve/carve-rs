use super::*;
use crate::include_walk::{visit_inline_children, SubtreeVisitor};

mod unicode_categories;

#[derive(Default)]
struct State {
    depth: usize,
    span: Option<SpanHandle>,
}

struct SpanHandle {
    frame: usize,
    // Returned child vectors keep their allocation when moved into a parent node.
    allocation: Option<usize>,
    // Record only the route to the opener, so demotion never rescans the block.
    path: Vec<(usize, usize)>,
    index: usize,
    demote: bool,
}

thread_local! {
    static STATE: RefCell<State> = const { RefCell::new(State { depth: 0, span: None }) };
}

// Detached inline content finalizes its quotes before restoring the host state.
pub(super) struct Scope(State);

impl Scope {
    pub(super) fn enter() -> Self {
        Self(STATE.with(|state| std::mem::take(&mut *state.borrow_mut())))
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        STATE.with(|state| *state.borrow_mut() = std::mem::take(&mut self.0));
    }
}

pub(super) struct Frame(usize);

impl Frame {
    pub(super) fn enter() -> Self {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            if state.depth == 0 {
                state.span = None;
            }
            state.depth += 1;
            Self(state.depth)
        })
    }

    pub(super) fn push(&self, out: &mut Vec<InlineNode>, mut node: InlineNode) {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            if let Some(span) = state.span.as_mut() {
                if let Some(allocation) = span.allocation {
                    struct Locate<'a> {
                        span: &'a mut SpanHandle,
                        allocation: usize,
                        parent: usize,
                        frame: usize,
                        child: usize,
                    }
                    impl SubtreeVisitor for Locate<'_> {
                        fn inlines(&mut self, nodes: &mut Vec<InlineNode>) {
                            if nodes.as_ptr() as usize == self.allocation {
                                self.span.path.push((self.parent, self.child));
                                self.span.allocation = None;
                                self.span.frame = self.frame;
                            }
                            self.child += 1;
                        }
                    }
                    visit_inline_children(
                        &mut node,
                        &mut Locate {
                            span,
                            allocation,
                            parent: out.len(),
                            frame: self.0,
                            child: 0,
                        },
                    );
                }
            }
        });
        out.push(node);
    }

    pub(super) fn finish(&self, out: &mut [InlineNode]) {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            if let Some(span) = state.span.as_mut() {
                if self.0 == 1 {
                    if span.demote {
                        demote(out, &span.path, span.index);
                    }
                } else if span.frame == self.0 && span.allocation.is_none() {
                    span.allocation = Some(out.as_ptr() as usize);
                }
            }
        });
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.depth -= 1;
            if state.depth == 0 {
                state.span = None;
            }
        });
    }
}

fn demote(nodes: &mut [InlineNode], path: &[(usize, usize)], index: usize) {
    if let Some((&(parent, child), remaining)) = path.split_last() {
        struct Rewrite<'a> {
            child: usize,
            remaining: &'a [(usize, usize)],
            index: usize,
            ordinal: usize,
        }
        impl SubtreeVisitor for Rewrite<'_> {
            fn inlines(&mut self, nodes: &mut Vec<InlineNode>) {
                if self.ordinal == self.child {
                    demote(nodes, self.remaining, self.index);
                }
                self.ordinal += 1;
            }
        }
        visit_inline_children(
            &mut nodes[parent],
            &mut Rewrite {
                child,
                remaining,
                index,
                ordinal: 0,
            },
        );
    } else if let InlineNode::SmartPunctuation(node) = &mut nodes[index] {
        node.kind = "right_single_quote".into();
        node.glyph = Some("’".into());
    }
}

pub(super) fn single(
    text: &str,
    i: usize,
    prev: char,
    next: Option<char>,
    open: bool,
    index: usize,
) -> bool {
    let alnum = next.is_some_and(is_alnum);
    let mut apostrophe = next.is_some_and(|c| c.is_ascii_digit()) || (!open && alnum);
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if !apostrophe && !open && !is_flank_space(prev) {
            state.span = None;
        }
        if open && !apostrophe {
            if alnum {
                let rest = &text[i + 1..];
                let end = rest
                    .char_indices()
                    .take_while(|&(_, c)| is_letter(c))
                    .map(|(offset, c)| offset + c.len_utf8())
                    .last()
                    .unwrap_or(0);
                let word = rest[..end].to_ascii_lowercase();
                let mut tail = rest[end..].chars();
                let quoted = tail.next() == Some('\'') && !tail.next().is_some_and(is_alnum);
                let elision = matches!(
                    word.as_str(),
                    "tis"
                        | "tisn"
                        | "twas"
                        | "twasn"
                        | "twere"
                        | "twill"
                        | "twould"
                        | "em"
                        | "cause"
                        | "til"
                        | "n"
                        | "bout"
                ) && !quoted;
                apostrophe = elision || state.span.is_some();
            }
            if !apostrophe && state.span.is_none() {
                state.span = Some(SpanHandle {
                    frame: state.depth,
                    allocation: None,
                    path: Vec::new(),
                    index,
                    demote: alnum
                        // Braced comments that open the content count as
                        // nothing (carve#4552); they leave `prev` unset.
                        && !(state.depth == 1 && (i == 0 || (prev == '\0' && text.starts_with("{%"))))
                        && prev != '“',
                });
            }
        }
    });
    apostrophe || !open
}

fn in_ranges(c: char, ranges: &[(u32, u32)]) -> bool {
    let value = c as u32;
    ranges
        .binary_search_by(|&(start, end)| {
            if end < value {
                std::cmp::Ordering::Less
            } else if start > value {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

fn is_letter(c: char) -> bool {
    if c.is_ascii() {
        c.is_ascii_alphabetic()
    } else {
        in_ranges(c, &unicode_categories::LETTER)
    }
}

fn is_alnum(c: char) -> bool {
    if c.is_ascii() {
        c.is_ascii_alphanumeric()
    } else {
        is_letter(c) || in_ranges(c, &unicode_categories::NUMBER)
    }
}

#[cfg(test)]
mod unicode_category_tests {
    use super::{is_alnum, is_letter};

    #[test]
    fn categories_match_regex_for_every_unicode_scalar() {
        let letter = regex::Regex::new(r"^\p{L}$").unwrap();
        let alnum = regex::Regex::new(r"^[\p{L}\p{N}]$").unwrap();
        for c in (0..=0x10ffff).filter_map(char::from_u32) {
            let mut buffer = [0; 4];
            let value = c.encode_utf8(&mut buffer);
            assert_eq!(is_letter(c), letter.is_match(value), "U+{:04X}", c as u32);
            assert_eq!(is_alnum(c), alnum.is_match(value), "U+{:04X}", c as u32);
        }
    }
}
