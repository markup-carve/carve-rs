//! A BLOCK OPENER AT A DESCRIPTION-HOSTED NOTE'S FLOOR REACHES THE NOTE BODY
//! (markup-carve/carve#1974).
//!
//! A footnote definition written into a `dd` has a body floor two columns past
//! its OWN marker (PART 9 §16), exactly as a note nested in another note does
//! (carve-rs#1575). A block opener at or past that floor opens a block inside
//! the note, so it is gathered into the note - and an unreferenced note then
//! drops with it, leaving the description empty. A trailing line at column 0 is
//! below both the note's floor and the description's base column, so it is a
//! top-level sibling paragraph (the column-reach model of carve#1946/#1971),
//! not a lazy continuation of the list item.
//!
//! One band must stay put: the opener ONE column shy of the floor is an ordinary
//! description child, because the note reaches nothing there.
//!
//! PLAIN CONTINUATION AT THE FLOOR IS THE SAME LINE AS AN OPENER. This file
//! first pinned the opposite, on the reading that continuation was a separate
//! settled question. Corpus `359-a-footnote-definition-s-block-runs-to-the-end-
//! of-its-body` settles it the other way for a container-hosted note - marker at
//! column 2, plain continuation at 4, and the continuation is the note's - and
//! this engine passes that row under a LIST-ITEM host. Keeping the carve-out
//! left one floor for openers and another for prose under a `dd`, which is the
//! two-answers-at-once state this fix set out to end, and the ruling names
//! opener-vs-text parity among the goldens to pin.
//!
//! ORACLE: the maintainer ruling on carve#1974, plus carve-js `df49fafa` and
//! carve-php `d3269e08`, both built from source and byte-identical to every
//! expectation here (carve-rs#1578).

use carve::to_html;

fn html(source: &str) -> String {
    to_html(source).trim().to_string()
}

/// The description content column is 3, so `[^f]` stands at column 3 and its
/// floor is 5. The list opener sits at 5 and reaches the note; `[^f]` is never
/// referenced, so the note takes the list and drops. `tail` at column 0 falls
/// out of the description to the document.
#[test]
fn an_opener_at_the_floor_is_absorbed_and_a_column_zero_tail_is_top_level() {
    let src = ":: t\n:  [^f]: note\n     - nested\ntail\n";
    assert_eq!(
        html(src),
        "<dl>\n  <dt>t</dt>\n  <dd></dd>\n</dl>\n<p>tail</p>"
    );
}

/// Same opener, but `tail` sits at the description's own content column, so the
/// list is still absorbed by the note while `tail` stays in the description.
#[test]
fn an_absorbed_opener_leaves_a_tail_at_the_description_column_in_the_dd() {
    let src = ":: t\n:  [^f]: note\n     - nested\n   tail\n";
    assert_eq!(html(src), "<dl>\n  <dt>t</dt>\n  <dd>tail</dd>\n</dl>");
}

/// The opener one column shy of the floor (column 4 against a floor of 5) reaches
/// nothing: the list is an ordinary description child and `tail` lazily continues
/// the list item, exactly as before. This band already agreed and must not move.
#[test]
fn an_opener_one_column_shy_of_the_floor_stays_a_description_child() {
    let src = ":: t\n:  [^f]: note\n    - nested\ntail\n";
    assert_eq!(
        html(src),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>nested\ntail</li>\n    </ul>\n  </dd>\n</dl>"
    );
}

/// Plain continuation at the floor is the note's, exactly as an opener there is:
/// the floor is a column, and a line either reaches it or does not. `[^f]` is
/// unreferenced, so it drops with the line it took, and `tail` at column 0 falls
/// out of the description to the document.
#[test]
fn plain_continuation_at_the_floor_is_the_note_s_too() {
    let src = ":: t\n:  [^f]: note\n     more\ntail\n";
    assert_eq!(
        html(src),
        "<dl>\n  <dt>t</dt>\n  <dd></dd>\n</dl>\n<p>tail</p>"
    );
}

/// The same continuation one column shy of the floor, the other side of the
/// boundary: at column 4 against a floor of 5 the description keeps it.
#[test]
fn plain_continuation_one_column_shy_of_the_floor_stays_in_the_description() {
    let src = ":: t\n:  [^f]: note\n    more\ntail\n";
    assert_eq!(
        html(src),
        "<dl>\n  <dt>t</dt>\n  <dd>more\ntail</dd>\n</dl>"
    );
}

/// AND ACROSS A BLANK LINE, which is the ordinary spelling of a note with more
/// than one paragraph. §16 allows blank lines between a body's chunks, so the
/// gather is entered across one - testing only the line directly under the
/// definition refused every multi-paragraph hosted note.
#[test]
fn a_hosted_note_body_resumes_after_a_blank_line() {
    let src = ":: a\n:  [^f]: t\n\n     more\ntail\n\nx[^f]\n";
    let out = html(src);
    assert!(out.contains("<dd></dd>"), "the description kept the note's body: {out}");
    assert!(
        out.contains("<p>more<a href=\"#fnref1\""),
        "the note did not take the resumed body: {out}"
    );
}
