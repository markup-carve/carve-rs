//! Picking a run of in-band markers a document cannot collide with.
//!
//! Three sites in this crate need one. The parser leaves a placeholder on the
//! line a collected definition came from; the canonical writer carries verbatim
//! whitespace, blank lines and an escaped space through whole-document
//! normalization; the Markdown target carries PART 11 §8a and §8b's deferred
//! escape decisions to the line it emits them on. All of them do it by putting a
//! private-use character in the text and taking it back out later.
//!
//! A FIXED character cannot be told apart from an authored one, and the failure
//! is silent in both directions: the writer eats the author's character, and the
//! author's character makes the writer act on text it never wrote. That is the
//! rule markup-carve/carve#678 settled, and it has now been re-broken once per
//! new marker - markup-carve/carve-rs#607, #630, #1214 and #1216 - which is why
//! the allocation lives here instead of being spelled again at each site.
//!
//! Escaping the authored occurrences cannot fix it: any escape needs a reserved
//! character, and that character has the same collision. Picking characters the
//! document does not use does fix it, and cannot fail in practice - the BMP
//! private-use area alone has 6400 code points.
//!
//! WHAT IS SHARED IS THE ALLOCATOR, NOT THE RUN. Each site keeps its own run and
//! its own slot meanings, because they have different lifetimes: the parser's
//! spans one parse, the writer's one document's line assembly, the Markdown
//! target's one document's line emission. A single shared run would tie them
//! together for no gain and make a slot added at one site a renumbering at the
//! others. Ported from markup-carve/carve-js#1289's `src/sentinel-run.ts`.
//!
//! U+E000 is NOT allocatable here, and no caller should ask for it. It is the
//! parser's in-band marker for a non-breaking space ([`crate::NBSP_PLACEHOLDER`]),
//! shared with the HTML, plain, ANSI and Markdown renderers, so an authored
//! U+E000 is already indistinguishable from a parsed nbsp before any writer
//! runs. That is the other half of carve#678 and needs a decision about what the
//! parsed text of an nbsp is, not a change here.

use std::collections::BTreeSet;

/// The pool every run is taken from: the BMP private-use area, minus its first
/// code point (see the module note on U+E000).
pub(crate) const POOL_FIRST: u32 = 0xe001;
pub(crate) const POOL_LAST: u32 = 0xf8ff;

/// Which private-use code points `text` occupies, added to `occupied`.
///
/// A SET of code points rather than a search over the text joined: the answer is
/// bounded by the private-use area (at most 6400 entries) however large the
/// document is, and one pass builds it. Joining is a second full copy of the
/// document, and scanning per candidate instead would be one full pass per
/// rejected run.
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

/// `N` private-use code points, none of them in `occupied`.
///
/// The preferred run - `N` code points from `base` - is tried first, so the
/// common case, a document with no private-use character at all, pays one scan
/// that finds nothing and `N` set lookups.
///
/// When any preferred code point IS taken the search walks the pool ONE CODE
/// POINT AT A TIME. Stepping a whole run at a time would step over the free
/// window between two occupied code points whenever it is not a whole number of
/// runs from the base, and report the area full while nearly all of it was free.
/// Resuming PAST the occupied code point rather than one code point on is the
/// same walk with the answers that cannot hold skipped: every run that begins in
/// between contains the code point that just failed.
///
/// The last resort is the preferred run rather than a refusal: it needs the
/// whole pool occupied, and a writer that gives up is worse than one that falls
/// back to the behavior it had before the run was picked at all.
/// markup-carve/carve-js#1289 lands in the same place.
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
