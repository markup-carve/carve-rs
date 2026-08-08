//! An inline comment keeps the space that makes it a comment
//! (`markup-carve/carve#1028`).
//!
//! `%%` opens a comment only at the start of a line or after whitespace - the
//! grammar's `inline_comment` says "the marker requires whitespace or
//! start-of-run before it". The writer knew that and put one space back, but it
//! asked the PREVIOUS NODE for its last character to decide, and emphasis, a
//! link, an image and a span all answer "no boundary character". That answer is
//! indistinguishable from "nothing precedes me", so the writer glued the marker
//! to the construct before it:
//!
//!     {,y,} %% c        was written as        {,y,}%% c
//!
//! and re-parsing carve-rs's OWN output turned the comment into literal text -
//! `<p><sub>y</sub>%% c</p>` where the source rendered `<p><sub>y</sub></p>`.
//! That is PART 11 section 1 failing in the direction section 1a names: the test
//! is on the emitted bytes, and the writer was reading its source instead.
//!
//! The comparison against the other two engines is not what decides it. Even
//! alone, carve-rs's output does not read back as the document it was written
//! from, and section 1 is stated over one engine.

fn html(src: &str) -> String {
    carve::to_html(src)
}

fn fmt(src: &str) -> String {
    carve::to_carve(src)
}

/// PART 11 section 1, asserted directly.
fn round_trips(src: &str) -> bool {
    html(&fmt(src)) == html(src)
}

/// Every inline construct whose node reports no boundary character. Each one
/// used to swallow the separating space.
#[test]
fn a_comment_after_a_construct_that_reports_no_boundary_keeps_its_space() {
    for src in [
        "{,y,} %% c\n",
        "{^x^} %% c\n",
        "=h= %% c\n",
        "/i/ %% c\n",
        "*b* %% c\n",
        "[t](u) %% c\n",
        "![a](/u) %% c\n",
        "$m$ %% c\n",
    ] {
        let written = fmt(src);
        assert!(
            !written.contains("}%%") && !written.contains("=%%") && !written.contains(")%%"),
            "the marker was glued to the construct before it: {written:?}"
        );
        assert!(round_trips(src), "{src:?} was written as {written:?}");
    }
}

/// The premise, stated as its own assertion: the glued form really does mean
/// something else, so the space is not cosmetic.
#[test]
fn a_glued_marker_is_literal_text() {
    assert!(
        html("{,y,}%% c\n").contains("%% c"),
        "{}",
        html("{,y,}%% c\n")
    );
    assert!(
        !html("{,y,} %% c\n").contains("%% c"),
        "{}",
        html("{,y,} %% c\n")
    );
}

/// A comment that OPENS its line gets no space, which is the behavior the
/// previous-node test got right and this must not lose.
#[test]
fn a_comment_at_the_start_of_a_line_keeps_column_zero() {
    assert_eq!(fmt("%% c\n"), "%% c\n");
    assert_eq!(fmt("::: note\n%% c\n:::\n"), "::: note\n%% c\n:::\n");
}

/// A comment after ordinary text keeps its single space, and gains no second
/// one on a further pass.
#[test]
fn a_comment_after_text_keeps_one_space() {
    assert_eq!(fmt("word %% c\n"), "word %% c\n");
    assert_eq!(fmt(&fmt("word %% c\n")), fmt("word %% c\n"));
    assert_eq!(fmt(&fmt("{,y,} %% c\n")), fmt("{,y,} %% c\n"));
}
