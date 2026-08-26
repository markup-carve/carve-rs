//! Below a definition description's content column, the five invisible-line
//! kinds answer ONE rule - and the rule is that the four registering kinds FOLD
//! (markup-carve/carve#1809, §10 I5 DEFINITION OWNERSHIP IS COLUMN-SCOPED).
//!
//! THIS FILE FIRST ASSERTED THE OPPOSITE FOR TWO OF THEM, and the history is the
//! point. markup-carve/carve-rs#1438 read the band as "a link reference
//! definition ends the body, like the footnote spelling PART 9 section 10 I5
//! lists it with", which made the two kinds agree - in the wrong direction.
//! carve#1809 then ruled the band from the LIST ITEM's answer: at a nonzero
//! column below a container's content column an invisible line "is lazy
//! paragraph text of THAT container (the one whose content column it fell
//! below) and does not register". Corpus 430 and 430-6 pin it, and
//! markup-carve/carve-rs#1443 is the change that moved these rows.
//!
//! What #1438 got right survives unchanged, and is why the file stays: the
//! ABBREVIATION definition folds because PART 12 section 7 recognizes it only as
//! a direct child of the DOCUMENT, so inside a `dd` it was never an invisible
//! line at all - and the arm was gated on `cur.at_document_level`, which
//! describes the CURSOR and not the line, so the identical description answered
//! one way at top level and the other inside a list item. Two kinds, two
//! reasons, one answer.
//!
//! Every row runs on BOTH render entry points.

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
fn a_link_definition_below_the_column_folds_like_the_footnote_one() {
    // The pair this file was written about, now both folding. They still have to
    // AGREE - that half of #1438 was right and no clause separates them by
    // column - and carve#1809 supplied the direction.
    for indent in ["  ", " "] {
        assert_eq!(
            html(&format!(":: t\n:  d\n{indent}[r]: /u\ntail\n")),
            format!("{FOLDS_HEAD}[r]: /u\ntail</dd>\n</dl>"),
            "indent {:?}",
            indent
        );
        assert_eq!(
            html(&format!(":: t\n:  d\n{indent}[^f]: n\ntail\n")),
            format!("{FOLDS_HEAD}[^f]: n\ntail</dd>\n</dl>"),
            "indent {:?}",
            indent
        );
    }
}

#[test]
fn a_link_definition_below_the_column_still_registers_nothing() {
    // Unchanged in substance, and it is the row that makes the fold a whole one:
    // the characters reach the page AND the symbol table stays empty, so a later
    // reference is literal. Text plus a registration is the half fold corpus 430
    // exists to catch.
    let out = html(":: t\n:  d\n  [r]: /u\ntail\n\n[link][r]\n");
    assert!(out.contains("<dd>d\n[r]: /u\ntail</dd>"), "{out}");
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
fn controls_the_plain_line_and_the_comment_bound_the_band() {
    // The plain line is what "folds as text" means and folded from every column
    // all along. The ATTRIBUTE line moved with the definitions under carve#1809 -
    // this row used to assert that it ended the body - while the COMMENT is
    // column-exempt (PART 9 section 24) and renders nothing at any column, which
    // corpus 430-5 pins. The comment is what tells this fix from one that folded
    // the whole invisible set.
    assert_eq!(
        html(":: t\n:  d\n  x\ntail\n"),
        format!("{FOLDS_HEAD}x\ntail</dd>\n</dl>")
    );
    assert_eq!(
        html(":: t\n:  d\n  {.k}\ntail\n"),
        format!("{FOLDS_HEAD}{{.k}}\ntail</dd>\n</dl>")
    );
    assert_eq!(
        html(":: t\n:  d\n  %% c\ntail\n"),
        format!("{ENDS}<p>tail</p>")
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
