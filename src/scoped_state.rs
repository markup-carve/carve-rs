//! Restoration guards for state scoped to a synchronous operation on one thread.

use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::rc::Rc;
use std::thread::LocalKey;

pub(crate) struct CellScope<T: Copy + 'static> {
    key: &'static LocalKey<Cell<T>>,
    previous: T,
    thread: PhantomData<Rc<()>>,
}

impl<T: Copy + 'static> CellScope<T> {
    pub(crate) fn replace(key: &'static LocalKey<Cell<T>>, value: T) -> Self {
        Self {
            key,
            previous: key.with(|cell| cell.replace(value)),
            thread: PhantomData,
        }
    }
}

impl<T: Copy + 'static> Drop for CellScope<T> {
    fn drop(&mut self) {
        self.key.with(|cell| cell.set(self.previous));
    }
}

pub(crate) struct RefCellScope<T: 'static> {
    key: &'static LocalKey<RefCell<T>>,
    previous: Option<T>,
    thread: PhantomData<Rc<()>>,
}

impl<T: 'static> RefCellScope<T> {
    pub(crate) fn replace(key: &'static LocalKey<RefCell<T>>, value: T) -> Self {
        Self {
            key,
            previous: Some(key.with(|cell| cell.replace(value))),
            thread: PhantomData,
        }
    }
}

impl<T: 'static> Drop for RefCellScope<T> {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            self.key.with(|cell| cell.replace(previous));
        }
    }
}
