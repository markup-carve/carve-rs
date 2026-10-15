//! Allocate private-use markers absent from the document. Parser and writers
//! use them temporarily, then remove them from output. Each caller keeps its
//! own run so slot changes at one site do not renumber another. Fixed markers
//! silently collide with authored text (carve#678). Ported from
//! markup-carve/carve-js#1289.
//!
//! U+E000 is ordinary text; generated spaces use explicit AST nodes. E001 is
//! retained as the historical pool start.

use std::collections::BTreeSet;

/// The pool every run is taken from: the BMP private-use area, minus its first
/// code point (see the module note on U+E000).
pub(crate) const POOL_FIRST: u32 = 0xe001;
pub(crate) const POOL_LAST: u32 = 0xf8ff;

/// Add the private-use code points in `text` to `occupied`.
pub(crate) fn collect_private_use(text: &str, occupied: &mut BTreeSet<u32>) {
    occupied.extend(
        text.chars()
            .map(u32::from)
            .filter(|code| (POOL_FIRST..=POOL_LAST).contains(code)),
    );
}

/// Which private-use code points `text` occupies.
pub(crate) fn occupied_private_use(text: &str) -> BTreeSet<u32> {
    let mut occupied = BTreeSet::new();
    collect_private_use(text, &mut occupied);
    occupied
}

/// The `N` code points starting at `first`.
fn run_from<const N: usize>(first: u32) -> [char; N] {
    std::array::from_fn(|offset| {
        char::from_u32(first + offset as u32).expect("private-use code point")
    })
}

/// Pick an unused run, trying `base` first, then skipping past occupied points.
/// A whole-run step would miss free gaps. Falls back to `base` if the pool is
/// full so a writer can still render as it did before allocation.
pub(crate) fn pick_sentinel_run<const N: usize>(occupied: &BTreeSet<u32>, base: u32) -> [char; N] {
    let taken = |from: u32| (0..N as u32).any(|offset| occupied.contains(&(from + offset)));

    if !taken(base) {
        return run_from(base);
    }

    let mut start = POOL_FIRST;
    while start + (N as u32).saturating_sub(1) <= POOL_LAST {
        let mut free = 0u32;
        while (free as usize) < N && !occupied.contains(&(start + free)) {
            free += 1;
        }
        if free as usize == N {
            return run_from(start);
        }
        start += free + 1;
    }

    run_from(base)
}

/// Reserve the nonce core even when adjacent text omits part of a marker.
pub(crate) fn collect_marker_nonce_cores(
    text: &str,
    nonces: &mut std::collections::BTreeSet<usize>,
) {
    for (at, _) in text.match_indices('m') {
        let rest = &text[at + 1..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && rest.as_bytes().get(digits) == Some(&b':') {
            if let Ok(nonce) = rest[..digits].parse::<usize>() {
                nonces.insert(nonce);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taken(ranges: &[(u32, u32)]) -> BTreeSet<u32> {
        let mut set = BTreeSet::new();
        for (from, to) in ranges {
            set.extend(*from..=*to);
        }
        set
    }

    #[test]
    fn the_preferred_run_is_kept_when_the_document_is_clear() {
        assert_eq!(
            pick_sentinel_run::<4>(&BTreeSet::new(), 0xe004),
            ['\u{e004}', '\u{e005}', '\u{e006}', '\u{e007}']
        );
    }

    #[test]
    fn one_occupied_code_point_moves_the_whole_run() {
        let picked = pick_sentinel_run::<4>(&taken(&[(0xe006, 0xe006)]), 0xe004);
        assert!(!picked.contains(&'\u{e006}'));
        for (offset, ch) in picked.iter().enumerate() {
            assert_eq!(u32::from(*ch), u32::from(picked[0]) + offset as u32);
        }
    }

    /// THE SCAN WALKS ONE CODE POINT AT A TIME. A document that occupies every
    /// Nth code point from the base leaves free windows that no aligned scan
    /// ever lands on, and an aligned scan reports the pool full while nearly all
    /// of it is free.
    #[test]
    fn a_free_window_off_the_run_boundary_is_found() {
        let mut occupied = BTreeSet::new();
        for code in (POOL_FIRST..POOL_FIRST + 400).step_by(4) {
            occupied.insert(code);
        }
        let picked = pick_sentinel_run::<3>(&occupied, 0xe004);
        for ch in picked {
            assert!(!occupied.contains(&u32::from(ch)), "{ch:?} was occupied");
        }
    }

    /// Exhaustion falls back to the preferred run rather than refusing - the
    /// writer-side answer markup-carve/carve-js#1289 settled on, and the one
    /// markup-carve/carve-rs#1218 already ports for the parser.
    #[test]
    fn an_exhausted_pool_falls_back_to_the_preferred_run() {
        assert_eq!(
            pick_sentinel_run::<2>(&taken(&[(POOL_FIRST, POOL_LAST)]), 0xe005),
            ['\u{e005}', '\u{e006}']
        );
    }

    /// Occupancy is answered from the document's own characters, and only from
    /// the pool: U+E000 is never in it, so it can never be allocated away from
    /// the nbsp placeholder.
    #[test]
    fn occupancy_covers_the_pool_and_nothing_else() {
        let occupied = occupied_private_use("a\u{e000}b\u{e004}c\u{f8ff}d\u{f900}");
        assert!(occupied.contains(&0xe004));
        assert!(occupied.contains(&0xf8ff));
        assert!(!occupied.contains(&0xe000));
        assert!(!occupied.contains(&0xf900));
    }
}
