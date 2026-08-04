//! An implicit heading reference is written back as the reference the author
//! wrote (PART 11 R1, carve#478).
//!
//! There is no `[label]: url` line for it, so `[H][]` is the ONLY record of the
//! authored form: resolving it to `[H](#H)` bakes a generated id into the source
//! on every fmt pass. An EXPLICIT definition still normalizes to the inline
//! form, because there the definition line is dropped either way and the
//! authored pair is not reproducible from the tree.
//!
//! This used to be carried by "the node still has a ref": the explicit branch
//! cleared it and the heading branch did not. PART 12 §3a made BOTH keep it
//! (carve#597), which left the writer unable to tell them apart - and it wrote
//! the resolved form for both, silently changing what fmt does to a heading
//! reference.

#[test]
fn a_heading_reference_keeps_its_authored_form() {
    assert_eq!(
        carve::to_carve("# H\n\nSee [H][].\n"),
        "# H\n\nSee [H][].\n"
    );
}

#[test]
fn an_explicit_definition_still_normalizes() {
    assert_eq!(
        carve::to_carve("see [t][r].\n\n[r]: /u\n"),
        "see [t](/u).\n",
    );
}

#[test]
fn an_unresolved_reference_round_trips_verbatim() {
    assert_eq!(carve::to_carve("see [t][miss].\n"), "see [t][miss].\n");
}

#[test]
fn the_html_is_unchanged_either_way() {
    assert!(carve::to_html("# H\n\nSee [H][].\n").contains("<a href=\"#H\">H</a>"));
    assert!(carve::to_html("see [t][r].\n\n[r]: /u\n").contains("<a href=\"/u\">t</a>"));
}

#[test]
fn fmt_is_idempotent_on_a_heading_reference() {
    let once = carve::to_carve("# H\n\nSee [H][].\n");

    assert_eq!(carve::to_carve(&once), once);
}
