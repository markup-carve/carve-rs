//! `fmt` writes a nested list with the indentation it read.
//!
//! Each level used to be indented twice - once by an absolute
//! `"  " * (list_depth - 1)` and again by the parent item's continuation
//! prefix - with a two-space strip of the child's output as partial
//! compensation. The parent's prefix IS the child's indentation, so the
//! absolute term was redundant: output grew as O(depth^3) where the source is
//! O(depth^2), and `05-lists-5` came back with four spaces where it was written
//! with two (carve-rs#594; carve-js fixed the same shape in its #653).

use carve::to_carve;

fn ladder(depth: usize) -> String {
    (0..depth)
        .map(|i| format!("{}- l{i}\n", "  ".repeat(i)))
        .collect()
}

#[test]
fn a_nested_list_round_trips_byte_for_byte() {
    let src = "- fruit\n  - apples\n  - oranges\n- vegetables\n";

    assert_eq!(to_carve(src), src);
}

#[test]
fn one_level_of_nesting_keeps_two_spaces() {
    let src = "- parent\n  - child\n";

    assert_eq!(to_carve(src), src);
}

#[test]
fn the_output_does_not_grow_with_depth() {
    // The ratio used to double as the depth doubled. Byte-identical now, which
    // is the strongest form of "does not grow".
    for depth in [5, 10, 20, 40] {
        let src = ladder(depth);
        assert_eq!(to_carve(&src), src, "depth {depth}");
    }
}

#[test]
fn an_ordered_ladder_keeps_its_own_marker_width() {
    // `1. ` is three columns, so the child sits at three - not at two, and not
    // at five.
    let src = "1. outer\n   1. inner\n";

    assert_eq!(to_carve(src), src);
}

#[test]
fn formatting_stays_idempotent() {
    let once = to_carve("- fruit\n  - apples\n    - deep\n- veg\n");

    assert_eq!(to_carve(&once), once);
}
