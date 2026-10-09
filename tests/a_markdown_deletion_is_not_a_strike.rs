//! PART 11 section 8c sends a `strike` whose delimiter spelling would not read
//! back to inline `<del>`, and the Markdown target wrote a CriticMarkup
//! deletion as `<del>` too. One tag for two constructs, so the importer read
//! every deletion back as a strike and a substitution as a strike beside an
//! unrelated insertion. The bytes look undamaged, which is why nothing
//! reported the loss.
//!
//! Clause `CARVE-P11-045` spells a deletion `<del class="critic-delete">` and
//! leaves a bare `<del>` meaning `strike` (markup-carve/carve#2845).

use carve::{markdown_to_carve, to_html, to_markdown};

/// Carve -> Markdown -> Carve, the trip the defect is measured on.
fn round(carve: &str) -> String {
    markdown_to_carve(&to_markdown(carve))
}

#[test]
fn a_deletion_round_trips_as_a_deletion() {
    assert_eq!(
        to_markdown("delete {-del-} here\n"),
        "delete <del class=\"critic-delete\">del</del> here\n"
    );
    assert_eq!(round("delete {-del-} here\n"), "delete {-del-} here\n");
}

#[test]
fn a_substitution_round_trips_as_a_substitution() {
    assert_eq!(
        to_markdown("substitute {~old~>new~} here\n"),
        "substitute <del class=\"critic-delete\">old</del><ins>new</ins> here\n"
    );
    assert_eq!(
        round("substitute {~old~>new~} here\n"),
        "substitute {~old~>new~} here\n"
    );
}

/// The control the ruling turns on: section 8c's own worked example is a
/// `strike` written as a bare `<del>`, and a bare `<del>` must keep reading
/// back as a `strike` rather than as the deletion.
#[test]
fn a_bare_del_is_still_a_strike() {
    assert_eq!(to_markdown("f {~ ~} g\n"), "f <del> </del> g\n");
    assert_eq!(markdown_to_carve("a <del>x</del> b\n"), "a ~x~ b\n");
    assert!(to_html(&markdown_to_carve("a <del>x</del> b\n")).contains("<s>x</s>"));
}

/// A body that would close or re-open the braced construct keeps the raw span
/// the tag arrived as.
#[test]
fn a_body_that_breaks_out_stays_a_raw_span() {
    assert!(
        markdown_to_carve("a <del class=\"critic-delete\">x {y} z</del> b\n").contains("{=html}")
    );
    assert!(
        markdown_to_carve("a <del class=\"critic-delete\">x ~> y</del><ins>z</ins> b\n")
            .contains("{=html}")
    );
}

/// A `<del>` carrying anything other than that one class is not the shape
/// section 8c writes, so it stays a raw span too.
#[test]
fn a_del_of_another_shape_stays_a_raw_span() {
    assert!(markdown_to_carve("a <del class=\"other\">x</del> b\n").contains("{=html}"));
    assert!(
        markdown_to_carve("a <del class=\"critic-delete\" id=\"k\">x</del> b\n")
            .contains("{=html}")
    );
}

/// The constructs this change must not move.
#[test]
fn the_neighboring_constructs_still_round_trip() {
    for carve in [
        "insert {+ins+} here\n",
        "highlight =hi= here\n",
        "subscript {,s,} here\n",
        "superscript {^s^} here\n",
        "strike ~str~ here\n",
    ] {
        assert_eq!(round(carve), carve, "round trip moved: {carve:?}");
    }
    assert_eq!(
        to_markdown("comment {#note#} here\n"),
        "comment <span class=\"critic-comment\">note</span> here\n"
    );
    assert_eq!(
        markdown_to_carve("strike ~~str~~ here\n"),
        "strike ~str~ here\n"
    );
}
