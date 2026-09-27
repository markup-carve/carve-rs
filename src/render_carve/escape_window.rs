//! Pruned renders for the escape-narrowing search (PART 11 §2b).
//!
//! A probe renders and re-parses only the blocks around the units it relaxes,
//! inside their real ancestor containers, instead of the whole document. The
//! search verifies its final state against the whole document and repeats
//! itself with whole-document probes when that check fails, so a window only
//! ever predicts.
//!
//! Units are walk ordinals, so a skipped block still advances the counter by
//! the units it would have claimed, as recorded in the control render.

use crate::scoped_state::RefCellScope;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// The key of the document-level entry list, which is not a slice of blocks.
pub(super) const ROOT: usize = 0;

/// A block list and an index in it.
type Slot = (usize, usize);

struct Element {
    slot: Slot,
    parent: Option<usize>,
}

/// Where every unit of one render sits, recorded by [`record`].
#[derive(Default)]
pub(super) struct Layout {
    elements: Vec<Element>,
    /// How many units each slot's element claims, itself included.
    spans: Rc<HashMap<Slot, usize>>,
    /// The innermost element each unit was claimed in, by ordinal.
    owner: Vec<Option<usize>>,
    /// Every unit the render claimed.
    total: usize,
}

struct Recording {
    layout: Layout,
    spans: HashMap<Slot, usize>,
    stack: Vec<(usize, usize)>,
}

/// Every block list on a window's paths, cut to the range the paths use.
#[derive(Clone)]
pub(super) struct Window {
    ranges: HashMap<usize, (usize, usize)>,
    spans: Rc<HashMap<Slot, usize>>,
}

impl Window {
    /// How many units a pruned render still claims.
    fn kept_units(&self, total: usize) -> usize {
        let skipped: usize = self
            .spans
            .iter()
            .filter(|((list, index), _)| {
                self.ranges
                    .get(list)
                    .is_some_and(|&(lo, hi)| !(lo..=hi).contains(index))
            })
            .map(|(_, units)| units)
            .sum();
        total.saturating_sub(skipped)
    }
}

thread_local! {
    static RECORDING: RefCell<Option<Recording>> = const { RefCell::new(None) };
    static ACTIVE: RefCell<Option<Window>> = const { RefCell::new(None) };
}

pub(super) struct Session {
    _recording: RefCellScope<Option<Recording>>,
    _active: RefCellScope<Option<Window>>,
}

impl Session {
    pub(super) fn new() -> Self {
        Self {
            _recording: RefCellScope::replace(&RECORDING, None),
            _active: RefCellScope::replace(&ACTIVE, None),
        }
    }
}

/// What a block loop does with the element at `slot`.
pub(super) enum Visit {
    /// Outside the active window: skip it, having advanced the counter.
    Skip,
    /// Render it; `true` when [`leave`] must close a recorded element.
    Render(bool),
}

/// Called by a block loop before it renders the element at (`list`, `index`).
pub(super) fn visit(list: usize, index: usize) -> Visit {
    let skipped = ACTIVE.with(|cell| {
        let active = cell.borrow();
        let window = active.as_ref()?;
        let &(lo, hi) = window.ranges.get(&list)?;
        if (lo..=hi).contains(&index) {
            return None;
        }
        Some(window.spans.get(&(list, index)).copied().unwrap_or(0))
    });
    if let Some(units) = skipped {
        super::UNIT_COUNTER.with(|c| c.set(c.get() + units));
        return Visit::Skip;
    }
    let recorded = RECORDING.with(|cell| {
        let mut cell = cell.borrow_mut();
        let Some(recording) = cell.as_mut() else {
            return false;
        };
        let parent = recording.stack.last().map(|&(id, _)| id);
        let id = recording.layout.elements.len();
        recording.layout.elements.push(Element {
            slot: (list, index),
            parent,
        });
        recording
            .stack
            .push((id, super::UNIT_COUNTER.with(|c| c.get())));
        true
    });
    Visit::Render(recorded)
}

/// Close the element [`visit`] opened, when it recorded one.
pub(super) fn leave(recorded: bool) {
    if !recorded {
        return;
    }
    RECORDING.with(|cell| {
        if let Some(recording) = cell.borrow_mut().as_mut() {
            if let Some((id, start)) = recording.stack.pop() {
                let slot = recording.layout.elements[id].slot;
                let end = super::UNIT_COUNTER.with(|c| c.get());
                recording.spans.insert(slot, end - start);
            }
        }
    });
}

/// Note the element `unit` was claimed in.
pub(super) fn claimed(unit: usize) {
    RECORDING.with(|cell| {
        if let Some(recording) = cell.borrow_mut().as_mut() {
            let owner = recording.stack.last().map(|&(id, _)| id);
            if recording.layout.owner.len() <= unit {
                recording.layout.owner.resize(unit + 1, None);
            }
            recording.layout.owner[unit] = owner;
        }
    });
}

/// Run `render` with the layout recorded.
pub(super) fn record<T>(render: impl FnOnce() -> T) -> (T, Layout) {
    let _scope = RefCellScope::replace(
        &RECORDING,
        Some(Recording {
            layout: Layout::default(),
            spans: HashMap::new(),
            stack: Vec::new(),
        }),
    );
    let out = render();
    let recording = RECORDING
        .with(|cell| cell.borrow_mut().take())
        .expect("recording");
    let mut layout = recording.layout;
    layout.spans = Rc::new(recording.spans);
    layout.total = super::UNIT_COUNTER.with(|c| c.get());
    (out, layout)
}

impl Layout {
    /// The window around `units`: the innermost list of each keeps one
    /// neighbor on each side, an ancestor list keeps just the path. `None`
    /// when a unit was claimed outside every recorded element, or when the
    /// window keeps more than half the document's units and so would save
    /// nothing over a whole-document probe.
    pub(super) fn window_for(&self, units: impl IntoIterator<Item = usize>) -> Option<Window> {
        let mut ranges: HashMap<usize, (usize, usize)> = HashMap::new();
        for unit in units {
            let mut element = (*self.owner.get(unit)?)?;
            let mut pad = 1;
            loop {
                let Element { slot, parent } = &self.elements[element];
                let (list, index) = *slot;
                let (lo, hi) = (index.saturating_sub(pad), index + pad);
                let range = ranges.entry(list).or_insert((lo, hi));
                range.0 = range.0.min(lo);
                range.1 = range.1.max(hi);
                pad = 0;
                match parent {
                    Some(parent) => element = *parent,
                    None => break,
                }
            }
        }
        if ranges.is_empty() {
            return None;
        }
        let window = Window {
            ranges,
            spans: Rc::clone(&self.spans),
        };
        (window.kept_units(self.total) * 2 <= self.total).then_some(window)
    }
}

/// Run `render` with only `window` written.
pub(super) fn render_pruned<T>(window: &Window, render: impl FnOnce() -> T) -> T {
    let _scope = RefCellScope::replace(&ACTIVE, Some(window.clone()));
    render()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{catch_unwind, AssertUnwindSafe};

    #[test]
    fn nested_pruning_restores_the_outer_window_after_unwinding() {
        let _session = Session::new();
        let outer = Window {
            ranges: HashMap::from([(ROOT, (0, 0))]),
            spans: Rc::default(),
        };
        let inner = Window {
            ranges: HashMap::from([(ROOT, (1, 1))]),
            spans: Rc::default(),
        };
        render_pruned(&outer, || {
            assert!(matches!(visit(ROOT, 1), Visit::Skip));
            let result = catch_unwind(AssertUnwindSafe(|| {
                render_pruned(&inner, || {
                    assert!(matches!(visit(ROOT, 1), Visit::Render(false)));
                    panic!("interrupted probe");
                })
            }));
            assert!(result.is_err());
            assert!(matches!(visit(ROOT, 1), Visit::Skip));
        });
        assert!(matches!(visit(ROOT, 1), Visit::Render(false)));
    }

    #[test]
    fn interrupted_inner_recording_preserves_the_outer_layout() {
        let _session = Session::new();
        let (_, layout) = record(|| {
            let Visit::Render(recorded) = visit(ROOT, 0) else {
                panic!("recording skipped")
            };
            claimed(0);
            assert!(catch_unwind(|| record(|| panic!("interrupted recording"))).is_err());
            claimed(1);
            leave(recorded);
        });
        assert_eq!(layout.owner, vec![Some(0), Some(0)]);
        assert_eq!(layout.elements.len(), 1);
        assert!(matches!(visit(ROOT, 0), Visit::Render(false)));
    }
}
