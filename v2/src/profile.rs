//! Stage hooks for the timing program in `bench/`: the inline pass runs once per leaf
//! inside the tree builder, so its time can only be taken from inside.

use std::sync::OnceLock;

/// Called with `true` before and `false` after each inline pass.
pub static INLINE_HOOK: OnceLock<fn(bool)> = OnceLock::new();

pub(crate) fn inline(enter: bool) {
    if let Some(f) = INLINE_HOOK.get() {
        f(enter);
    }
}
