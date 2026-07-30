//! In CommonMark a change of ordered-list delimiter SEPARATES two adjacent lists,
//! exactly as a change of bullet does. Measured against commonmark.js: `1. a`
//! followed by `1) c` gives two `<ol>` elements; the same input with one delimiter
//! gives one.
//!
//! So normalizing `1)` to `1.` merges lists the source kept apart -- the same
//! defect the bullet marker had (carve-rs#307), left in place a few lines below
//! the comment explaining why bullets must not be normalized (carve#352, corpus
//! 31).

#[test]
fn a_paren_delimiter_survives() {
    assert_eq!(carve::to_markdown("1) one\n2) two\n"), "1) one\n2) two\n");
}

#[test]
fn a_dot_delimiter_survives() {
    assert_eq!(carve::to_markdown("1. one\n2. two\n"), "1. one\n2. two\n");
}

#[test]
fn two_adjacent_lists_stay_apart() {
    let out = carve::to_markdown("1. a\n2. b\n\n1) c\n2) d\n");
    assert!(out.contains("1. a"), "got: {out:?}");
    assert!(out.contains("1) c"), "got: {out:?}");
}

#[test]
fn an_explicit_start_works_with_either_delimiter() {
    assert_eq!(
        carve::to_markdown("3) three\n4) four\n"),
        "3) three\n4) four\n"
    );
    assert_eq!(
        carve::to_markdown("3. three\n4. four\n"),
        "3. three\n4. four\n"
    );
}
