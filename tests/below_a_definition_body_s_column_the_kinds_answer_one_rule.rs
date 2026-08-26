//! Below a definition description's content column, the five invisible-line
//! kinds have to answer ONE rule, and two of them answered it backwards
//! (markup-carve/carve-rs#1438).
//!
//! 1. A LINK REFERENCE DEFINITION folded where the FOOTNOTE spelling ends the
//!    body. PART 9 section 10 I5 lists the two together and no clause separates
//!    them by column, so one rule cannot give them opposite answers. carve-js,
//!    carve-php and the oracle end the body for both. The reason this engine
//!    split them is mechanical: the pre-pass rewrites a collected definition to
//!    an invisible `%%` placeholder, which `interrupts_paragraph` sees, and it
//!    collects only AT a tracked content column - so below the column the
//!    footnote kind still arrived as a placeholder (its own pass reaches an
//!    indented body) while the link kind arrived as its raw line, which no arm
//!    of `interrupts_paragraph` matches.
//!
//! 2. An ABBREVIATION DEFINITION ended the body where it should FOLD.
//!    AN ABBREVIATION DEFINITION IS RECOGNIZED ONLY AT DOCUMENT LEVEL (PART 12
//!    section 7), so a line the container is still deciding on is not a
//!    definition at all - it is ordinary paragraph text, and
//!    markup-carve/carve#1786 states that half directly: "the plain line that is
//!    not an opener and folds from any column". The arm was gated on
//!    `cur.at_document_level`, which describes the CURSOR and not the line, so
//!    the identical description answered one way at top level and the other
//!    inside a list item - where this engine folded all along.
//!
//! The two invert each other, so fixing one alone leaves the band
//! self-inconsistent in the opposite direction. After both, the band reads:
//! link and footnote definitions end the body, an abbreviation definition
//! folds, a comment ends the body, an attribute line ends the body and stays
//! literal. Every row below is carve-js and carve-php byte for byte.
//!
//! Nothing in the corpus pins the band, which is why neither was caught.

fn html(source: &str) -> String {
    let convenience = carve::to_html(source);
    let cli = carve::try_to_html_with_options(source, &carve::Options::default())
        .expect("the default profile denies nothing");
    // BOTH ENTRY POINTS, ALWAYS. `to_html` opens with a layout fast path and the
    // CLI runs transforms, and a divergence living on one path only is what a
    // single-path assertion cannot see.
    assert_eq!(
        convenience, cli,
        "the two render paths disagree on:\n{source}"
    );
    convenience
}

const ENDS: &str = "<dl>\n  <dt>t</dt>\n  <dd>d</dd>\n</dl>\n";
const FOLDS_HEAD: &str = "<dl>\n  <dt>t</dt>\n  <dd>d\n";

#[test]
fn a_link_definition_below_the_column_ends_the_body_like_the_footnote_one() {
    for indent in ["  ", " "] {
        assert_eq!(
            html(&format!(":: t\n:  d\n{indent}[r]: /u\ntail\n")),
            format!("{ENDS}<p>[r]: /u\ntail</p>"),
            "indent {:?}",
            indent
        );
        // The kind it has to agree with, in the same position and build.
        assert_eq!(
            html(&format!(":: t\n:  d\n{indent}[^f]: n\ntail\n")),
            format!("{ENDS}<p>[^f]: n\ntail</p>"),
            "indent {:?}",
            indent
        );
    }
}

#[test]
fn a_link_definition_below_the_column_still_registers_nothing() {
    // The pre-pass declined to collect it, and ending the body must not change
    // that: the text reaches the page, so a later reference stays literal.
    let out = html(":: t\n:  d\n  [r]: /u\ntail\n\n[link][r]\n");
    assert!(out.contains("<p>[r]: /u\ntail</p>"), "{out}");
    assert!(out.contains("<p>[link][r]</p>"), "{out}");
    assert!(!out.contains("href=\"/u\""), "{out}");
}

#[test]
fn an_abbreviation_definition_below_the_column_folds_as_prose() {
    for indent in ["", " ", "  "] {
        assert_eq!(
            html(&format!(":: t\n:  d\n{indent}*[A]: a\ntail\n")),
            format!("{FOLDS_HEAD}*[A]: a\ntail</dd>\n</dl>"),
            "indent {:?}",
            indent
        );
    }
}

#[test]
fn an_abbreviation_definition_below_the_column_registers_nothing() {
    // It is description text, and text defines no abbreviation.
    let out = html(":: t\n:  d\n  *[A]: a\ntail\n\nA here\n");
    assert!(!out.contains("<abbr"), "{out}");
    assert!(out.contains("<p>A here</p>"), "{out}");
}

#[test]
fn the_nested_spelling_answers_the_same_way_it_always_did() {
    // A description inside a list item folded the abbreviation all along, which
    // is which of the two answers is right: one rule cannot depend on how deep
    // the host sits.
    assert_eq!(
        html("- :: t\n  :  d\n   *[A]: a\n  tail\n"),
        "<ul>\n  <li>\n    <dl>\n      <dt>t</dt>\n      <dd>d\n*[A]: a\ntail</dd>\n    </dl>\n  </li>\n</ul>"
    );
}

#[test]
fn controls_the_plain_line_folds_and_the_other_two_kinds_do_not_move() {
    // The plain line is what "folds as prose" means, and the comment and
    // attribute kinds are the two this build already answered - a fix that
    // reached past the two kinds at issue fails here.
    assert_eq!(
        html(":: t\n:  d\n  x\ntail\n"),
        format!("{FOLDS_HEAD}x\ntail</dd>\n</dl>")
    );
    assert_eq!(
        html(":: t\n:  d\n  %% c\ntail\n"),
        format!("{ENDS}<p>tail</p>")
    );
    assert_eq!(
        html(":: t\n:  d\n  {.k}\ntail\n"),
        format!("{ENDS}<p>{{.k}}\ntail</p>")
    );
}

#[test]
fn controls_at_and_above_the_content_column_are_untouched() {
    // AT the column a link definition is collected and an attribute line is
    // dropped inside the description; the band being fixed is strictly BELOW.
    assert_eq!(
        html(":: t\n:  d\n   [r]: /u\ntail\n\n[link][r]\n"),
        format!("{ENDS}<p>tail</p>\n<p><a href=\"/u\">link</a></p>")
    );
    assert_eq!(
        html(":: t\n:  d\n   {.k}\ntail\n"),
        format!("{ENDS}<p>tail</p>")
    );
}

#[test]
fn control_at_document_level_an_abbreviation_definition_is_still_one() {
    // The waived arm is a CONTAINER-continuation question. At document level,
    // where section 7 recognizes the definition, it still interrupts and still
    // registers.
    assert_eq!(
        html("para\n*[A]: a\n\nA here\n"),
        "<p>para</p>\n<p><abbr title=\"a\">A</abbr> here</p>"
    );
}
