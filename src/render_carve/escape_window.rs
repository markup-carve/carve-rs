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

use super::session::{RefCellScope, RenderSession};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
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
    /// The ordinal each slot's element starts after.
    starts: Rc<HashMap<Slot, usize>>,
    /// Slots the render visited more than once, at different ordinals.
    repeated: Rc<HashSet<Slot>>,
    /// The innermost element each unit was claimed in, by ordinal.
    owner: Vec<Option<usize>>,
    /// Every unit the render claimed.
    total: usize,
}

pub(super) struct Recording {
    layout: Layout,
    spans: HashMap<Slot, usize>,
    starts: HashMap<Slot, usize>,
    repeated: HashSet<Slot>,
    stack: Vec<(usize, usize)>,
}

/// Every block list on a window's paths, cut to the range the paths use.
#[derive(Clone)]
pub(super) struct Window {
    ranges: HashMap<usize, (usize, usize)>,
    spans: Rc<HashMap<Slot, usize>>,
    starts: Rc<HashMap<Slot, usize>>,
    repeated: Rc<HashSet<Slot>>,
    total: usize,
    /// Cleared when the pruned render's ordinals leave the control's: slot ids
    /// are addresses and a kept block can render differently without its
    /// neighbors, so a window can claim another block's units.
    faithful: Rc<Cell<bool>>,
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

/// What a block loop does with the element at `slot`.
pub(super) enum Visit {
    /// Outside the active window: skip it, having advanced the counter.
    Skip,
    /// Render it; `true` when [`leave`] must close a recorded element.
    Render(bool),
}

/// Called by a block loop before it renders the element at (`list`, `index`).
pub(super) fn visit(session: &RenderSession, list: usize, index: usize) -> Visit {
    let skipped = session.active.with(|cell| {
        let active = cell.borrow();
        let window = active.as_ref()?;
        let slot = (list, index);
        let pruned = window.ranges.get(&list);
        if window.repeated.contains(&slot) {
            if pruned.is_some() {
                window.faithful.set(false);
            }
        } else {
            let at = session.pass.unit_counter.with(|c| c.get());
            match window.starts.get(&slot) {
                Some(&start) if start != at => window.faithful.set(false),
                None if pruned.is_some() => window.faithful.set(false),
                _ => {}
            }
        }
        let &(lo, hi) = pruned?;
        if (lo..=hi).contains(&index) {
            return None;
        }
        Some(window.spans.get(&slot).copied().unwrap_or(0))
    });
    if let Some(units) = skipped {
        session.pass.unit_counter.with(|c| c.set(c.get() + units));
        return Visit::Skip;
    }
    let recorded = session.recording.with(|cell| {
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
        let start = session.pass.unit_counter.with(|c| c.get());
        if *recording.starts.entry((list, index)).or_insert(start) != start {
            recording.repeated.insert((list, index));
        }
        recording.stack.push((id, start));
        true
    });
    Visit::Render(recorded)
}

/// Close the element [`visit`] opened, when it recorded one.
pub(super) fn leave(session: &RenderSession, recorded: bool) {
    if !recorded {
        return;
    }
    session.recording.with(|cell| {
        if let Some(recording) = cell.borrow_mut().as_mut() {
            if let Some((id, start)) = recording.stack.pop() {
                let slot = recording.layout.elements[id].slot;
                let end = session.pass.unit_counter.with(|c| c.get());
                recording.spans.insert(slot, end - start);
            }
        }
    });
}

/// Note the element `unit` was claimed in.
pub(super) fn claimed(session: &RenderSession, unit: usize) {
    session.recording.with(|cell| {
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
pub(super) fn record<T>(session: &RenderSession, render: impl FnOnce() -> T) -> (T, Layout) {
    let _scope = RefCellScope::replace(
        &session.recording,
        Some(Recording {
            layout: Layout::default(),
            spans: HashMap::new(),
            starts: HashMap::new(),
            repeated: HashSet::new(),
            stack: Vec::new(),
        }),
    );
    let out = render();
    let recording = session
        .recording
        .with(|cell| cell.borrow_mut().take())
        .expect("recording");
    let mut layout = recording.layout;
    layout.spans = Rc::new(recording.spans);
    layout.starts = Rc::new(recording.starts);
    layout.repeated = Rc::new(recording.repeated);
    layout.total = session.pass.unit_counter.with(|c| c.get());
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
            starts: Rc::clone(&self.starts),
            repeated: Rc::clone(&self.repeated),
            total: self.total,
            faithful: Rc::new(Cell::new(true)),
        };
        (window.kept_units(self.total) * 2 <= self.total).then_some(window)
    }
}

/// Run `render` with only `window` written. `None` when the render did not
/// claim the control's ordinals, so its bytes predict nothing.
pub(super) fn render_pruned<T>(
    session: &RenderSession,
    window: &Window,
    render: impl FnOnce() -> T,
) -> Option<T> {
    window.faithful.set(true);
    let _scope = RefCellScope::replace(&session.active, Some(window.clone()));
    let out = render();
    let claimed = session.pass.unit_counter.with(|c| c.get());
    (window.faithful.get() && claimed == window.total).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{catch_unwind, AssertUnwindSafe};

    #[test]
    fn nested_pruning_restores_the_outer_window_after_unwinding() {
        let session = &RenderSession::new(false);
        let window = |range| Window {
            ranges: HashMap::from([(ROOT, range)]),
            spans: Rc::default(),
            starts: Rc::default(),
            repeated: Rc::default(),
            total: 0,
            faithful: Rc::new(Cell::new(true)),
        };
        let (outer, inner) = (window((0, 0)), window((1, 1)));
        render_pruned(session, &outer, || {
            assert!(matches!(visit(session, ROOT, 1), Visit::Skip));
            let result = catch_unwind(AssertUnwindSafe(|| {
                render_pruned(session, &inner, || {
                    assert!(matches!(visit(session, ROOT, 1), Visit::Render(false)));
                    panic!("interrupted probe");
                })
            }));
            assert!(result.is_err());
            assert!(matches!(visit(session, ROOT, 1), Visit::Skip));
        });
        assert!(matches!(visit(session, ROOT, 1), Visit::Render(false)));
    }

    /// Element `index` claims `units[index]` ordinals.
    fn walk(session: &RenderSession, units: &[usize]) {
        session.begin_pass();
        for (index, &count) in units.iter().enumerate() {
            let Visit::Render(recorded) = visit(session, ROOT, index) else {
                continue;
            };
            for _ in 0..count {
                let next = session.pass.unit_counter.get() + 1;
                session.pass.unit_counter.set(next);
                claimed(session, next);
            }
            leave(session, recorded);
        }
    }

    #[test]
    fn a_pruned_render_that_claims_other_ordinals_is_unfaithful() {
        let session = &RenderSession::new(false);
        let (_, layout) = record(session, || walk(session, &[1; 6]));
        let window = layout.window_for([2]).expect("window");
        assert!(render_pruned(session, &window, || walk(session, &[1; 6])).is_some());
        // Element 2 claims one more ordinal than the control: element 3 starts
        // past its recorded start.
        assert!(render_pruned(session, &window, || walk(session, &[1, 1, 2, 1, 1, 1])).is_none());
        // The last element drifts, so only the total shows it.
        let last = layout.window_for([6]).expect("window");
        assert!(render_pruned(session, &last, || walk(session, &[1; 6])).is_some());
        assert!(render_pruned(session, &last, || walk(session, &[1, 1, 1, 1, 1, 2])).is_none());
    }

    #[test]
    fn interrupted_inner_recording_preserves_the_outer_layout() {
        let session = &RenderSession::new(false);
        let (_, layout) = record(session, || {
            let Visit::Render(recorded) = visit(session, ROOT, 0) else {
                panic!("recording skipped")
            };
            claimed(session, 0);
            assert!(catch_unwind(AssertUnwindSafe(|| record(session, || panic!(
                "interrupted recording"
            ))))
            .is_err());
            claimed(session, 1);
            leave(session, recorded);
        });
        assert_eq!(layout.owner, vec![Some(0), Some(0)]);
        assert_eq!(layout.elements.len(), 1);
        assert!(matches!(visit(session, ROOT, 0), Visit::Render(false)));
    }
}
