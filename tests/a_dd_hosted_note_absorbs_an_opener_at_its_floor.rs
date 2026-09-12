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
//! Text and opener stay in PARITY: plain continuation at the floor is absorbed
//! into the note exactly as an opener is (carve-php's own test asserts it,
//! carve-js does it, and carve#1979 pins it in the corpus). One band must stay
//! put: the opener ONE column shy of the floor is an ordinary description child
//! (the note reaches nothing).
//!
//! ORACLE: the maintainer ruling on carve#1974 (djot confirms the note absorbs
//! the list; the column-0 top-level placement follows carve-php's Q2 answer and
//! the column-reach model).

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

/// Plain continuation at the floor is absorbed into the note exactly as an
/// opener is (carve#1974 text/opener parity). `more` reaches the floor and joins
/// the unreferenced note, which then drops; `tail` at column 0 falls to the
/// document.
#[test]
fn plain_continuation_at_the_floor_is_absorbed_like_an_opener() {
    let src = ":: t\n:  [^f]: note\n     more\ntail\n";
    assert_eq!(
        html(src),
        "<dl>\n  <dt>t</dt>\n  <dd></dd>\n</dl>\n<p>tail</p>"
    );
}

/// Text and opener at the floor produce byte-identical output: the continuation
/// case and the list-opener case both empty the description and drop `tail` to
/// the document. This is the parity the ruling requires.
#[test]
fn text_at_the_floor_matches_the_opener_at_the_floor() {
    let text = html(":: t\n:  [^f]: note\n     more\ntail\n");
    let opener = html(":: t\n:  [^f]: note\n     - nested\ntail\n");
    assert_eq!(text, opener);
}
