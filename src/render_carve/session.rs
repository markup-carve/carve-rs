//! State owned by one canonical render. Search candidates share this value;
//! nested public renders allocate independent sessions.

use super::{escape_window, Occurrence, SENTINEL_COUNT, SENTINEL_DEFAULTS};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::ops::Deref;

/// Owned storage with closure access for short-lived interior borrows.
pub(super) struct Slot<T>(T);
impl<T> Slot<T> {
    pub(super) fn with<R>(&self, read: impl FnOnce(&T) -> R) -> R {
        read(&self.0)
    }
}
impl<T> Deref for Slot<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

pub(super) struct PassState {
    pub(super) unit_counter: Slot<Cell<usize>>,
    pub(super) escape_call_indexes: Slot<RefCell<HashMap<usize, usize>>>,
}

pub(super) struct RenderSession {
    pub(super) pass: PassState,
    pub(super) redundant_ids: Slot<RefCell<BTreeSet<String>>>,
    pub(super) hyphen_breaks_are_unsafe: Slot<Cell<bool>>,
    pub(super) sentinels: Slot<Cell<[char; SENTINEL_COUNT]>>,
    pub(super) inserted: Slot<Cell<[usize; SENTINEL_COUNT]>>,
    pub(super) seen: Slot<Cell<[usize; SENTINEL_COUNT]>>,
    pub(super) staged: Slot<RefCell<String>>,
    pub(super) escalated_units: Slot<RefCell<Option<HashSet<usize>>>>,
    pub(super) asked_units: Slot<RefCell<Option<HashSet<usize>>>>,
    pub(super) relaxed_occurrences: Slot<RefCell<Option<HashSet<Occurrence>>>>,
    pub(super) occurrence_log: Slot<RefCell<Option<Vec<Occurrence>>>>,
    pub(super) last_occurrence_relaxed: Slot<Cell<bool>>,
    pub(super) escaped_openers: Slot<RefCell<HashMap<(usize, usize), bool>>>,
    pub(super) recording: Slot<RefCell<Option<escape_window::Recording>>>,
    pub(super) active: Slot<RefCell<Option<escape_window::Window>>>,
}
impl RenderSession {
    pub(super) fn new() -> Self {
        Self {
            pass: PassState {
                unit_counter: Slot(Cell::new(0)),
                escape_call_indexes: Slot(RefCell::new(HashMap::new())),
            },
            redundant_ids: Slot(RefCell::new(BTreeSet::new())),
            hyphen_breaks_are_unsafe: Slot(Cell::new(false)),
            sentinels: Slot(Cell::new(SENTINEL_DEFAULTS)),
            inserted: Slot(Cell::new([0; SENTINEL_COUNT])),
            seen: Slot(Cell::new([0; SENTINEL_COUNT])),
            staged: Slot(RefCell::new(String::new())),
            escalated_units: Slot(RefCell::new(None)),
            asked_units: Slot(RefCell::new(None)),
            relaxed_occurrences: Slot(RefCell::new(None)),
            occurrence_log: Slot(RefCell::new(None)),
            last_occurrence_relaxed: Slot(Cell::new(false)),
            escaped_openers: Slot(RefCell::new(HashMap::new())),
            recording: Slot(RefCell::new(None)),
            active: Slot(RefCell::new(None)),
        }
    }

    /// Reset emission ordinals before a minimal, conservative or fallback pass.
    pub(super) fn begin_pass(&self) {
        self.pass.unit_counter.set(0);
        self.pass.escape_call_indexes.borrow_mut().clear();
        if let Some(log) = self.occurrence_log.borrow_mut().as_mut() {
            log.clear();
        }
    }
}

pub(super) struct CellScope<'a, T: Copy> {
    cell: &'a Cell<T>,
    previous: T,
}
impl<'a, T: Copy> CellScope<'a, T> {
    pub(super) fn replace(cell: &'a Cell<T>, value: T) -> Self {
        Self {
            cell,
            previous: cell.replace(value),
        }
    }
}
impl<T: Copy> Drop for CellScope<'_, T> {
    fn drop(&mut self) {
        self.cell.set(self.previous);
    }
}
pub(super) struct RefCellScope<'a, T> {
    cell: &'a RefCell<T>,
    previous: Option<T>,
}
impl<'a, T> RefCellScope<'a, T> {
    pub(super) fn replace(cell: &'a RefCell<T>, value: T) -> Self {
        Self {
            cell,
            previous: Some(cell.replace(value)),
        }
    }
}
impl<T> Drop for RefCellScope<'_, T> {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            self.cell.replace(previous);
        }
    }
}
