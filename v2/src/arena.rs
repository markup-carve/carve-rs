//! The arena a tree lives in: nodes, child lists and strings in one bump
//! allocator, freed together when the arena is dropped.

use crate::ast::{Attrs, Node, owned};
use bumpalo::Bump;
use serde_json::Value;
use std::cell::RefCell;

/// A bump whose chunk is larger than this is freed, not kept for the next arena.
const KEEP: usize = 1 << 24;

thread_local! {
    /// The reset bump of the last arena of the thread. Chunks of 128 KiB and
    /// more come from fresh pages of the system on every allocation, so a
    /// bump made anew for each document pays a page fault per 4 KiB.
    static SPARE: RefCell<Option<Bump>> = const { RefCell::new(None) };
}

fn spare() -> Option<Bump> {
    SPARE
        .try_with(|s| s.try_borrow_mut().ok().and_then(|mut s| s.take()))
        .ok()
        .flatten()
}

/// Holds the memory of one or more trees. A tree borrows its arena (and the
/// source it was parsed from) for `'a`. Dropping it frees every tree in it;
/// its largest chunk is kept for the next arena of the thread.
pub struct Arena {
    bump: Bump,
    /// JSON values own heap memory and the bump runs no destructors: they
    /// live here, where dropping the arena drops them.
    values: typed_arena::Arena<Value>,
}

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        let mut bump = std::mem::take(&mut self.bump);
        bump.reset();
        if bump.allocated_bytes() <= KEEP {
            let _ = SPARE.try_with(|s| {
                if let Ok(mut s) = s.try_borrow_mut() {
                    *s = Some(bump);
                }
            });
        }
    }
}

impl Arena {
    /// An empty arena.
    #[must_use]
    pub fn new() -> Self {
        Self {
            bump: spare().unwrap_or_default(),
            values: typed_arena::Arena::new(),
        }
    }

    /// An arena whose first chunk holds at least `bytes`.
    #[must_use]
    pub fn with_capacity(bytes: usize) -> Self {
        let bump = spare()
            .filter(|b| b.chunk_capacity() >= bytes)
            .unwrap_or_else(|| Bump::with_capacity(bytes));
        Self {
            bump,
            values: typed_arena::Arena::new(),
        }
    }

    /// An arena whose first chunk fits the tree of a typical document as long as `input`.
    #[must_use]
    pub fn for_input(input: &str) -> Self {
        Self::with_capacity(input.len().saturating_mul(4).clamp(1 << 12, 1 << 26))
    }

    /// Bytes taken from the system so far.
    #[must_use]
    pub fn allocated_bytes(&self) -> usize {
        self.bump.allocated_bytes()
    }

    /// Moves a node into the arena. Every node type owns no heap memory (the
    /// generated types assert it), so nothing is leaked by skipping its drop.
    pub(crate) fn alloc<T>(&self, v: T) -> &mut T {
        self.bump.alloc(v)
    }

    /// A copy of `s` in the arena.
    pub(crate) fn str(&self, s: &str) -> &str {
        if s.is_empty() {
            ""
        } else {
            self.bump.alloc_str(s)
        }
    }

    pub(crate) fn strs(&self, v: &[String]) -> &[&str] {
        if v.is_empty() {
            return &[];
        }
        self.bump
            .alloc_slice_fill_iter(v.iter().map(|s| self.str(s)))
    }

    pub(crate) fn copy_slice<T: Copy>(&self, v: &[T]) -> &[T] {
        if v.is_empty() {
            return &[];
        }
        self.bump.alloc_slice_copy(v)
    }

    pub(crate) fn value(&self, v: &Value) -> &Value {
        self.values.alloc(v.clone())
    }

    pub(crate) fn json(&self, v: Value) -> &Value {
        self.values.alloc(v)
    }

    pub(crate) fn attrs(&self, a: &owned::Attrs) -> &Attrs<'_> {
        self.bump.alloc(a.in_arena(self))
    }

    /// The nodes of `v` moved into the arena; `v` is left empty for reuse.
    pub(crate) fn nodes<'a>(&'a self, v: &mut Vec<Node<'a>>) -> &'a mut [Node<'a>] {
        if v.is_empty() {
            return &mut [];
        }
        self.bump.alloc_slice_fill_iter(v.drain(..))
    }

    /// The nodes of an iterator of known length as a child list.
    pub(crate) fn fill<'a, I>(&'a self, it: I) -> &'a mut [Node<'a>]
    where
        I: ExactSizeIterator<Item = Node<'a>>,
    {
        if it.len() == 0 {
            return &mut [];
        }
        self.bump.alloc_slice_fill_iter(it)
    }

    /// One node as a child list.
    pub(crate) fn one<'a>(&'a self, n: Node<'a>) -> &'a mut [Node<'a>] {
        std::slice::from_mut(self.bump.alloc(n))
    }

    pub(crate) fn nodes_from<'a>(&'a self, v: &[owned::Node]) -> &'a mut [Node<'a>] {
        if v.is_empty() {
            return &mut [];
        }
        self.bump
            .alloc_slice_fill_iter(v.iter().map(|n| n.in_arena(self)))
    }

    pub(crate) fn pairs(&self, v: &[(String, String)]) -> &[(&str, &str)] {
        if v.is_empty() {
            return &[];
        }
        self.bump
            .alloc_slice_fill_iter(v.iter().map(|(k, x)| (self.str(k), self.str(x))))
    }
}
