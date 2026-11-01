//! PART 0 S1 tests an open `%%%` span's closer before ownership, so a closer
//! below a note body's column ends the span the note holds even when another
//! closer follows (carve#4721, pack case notes-6db864f6ba22).

#[test]
fn the_first_closer_below_the_column_ends_the_held_span() {
    let html = carve::to_html("see[^f]\n\n[^f]: head\n  %%%\n  a\n%%%\n%%%\n");
    assert!(!html.contains("<p>a"), "{html}");
    assert!(html.contains("<p>head<a href=\"#fnref1\""), "{html}");
}
