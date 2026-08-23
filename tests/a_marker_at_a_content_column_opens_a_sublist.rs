//! A marker at an item's content column opens a sublist, first in the item or not.
//!
//! PART 9 section 24 C3: "AT content_column: dedented to the body's column 0, a
//! block opener nests and a list marker opens a sublist", holding "whether or
//! not a blank line precedes the child". Section 10 I2 defers to it by name -
//! "TIGHT NESTED LISTS UNAFFECTED ... that is section 24 C3 (content column),
//! not this relation" - and the clause calls the content-column model an
//! intentional divergence from djot.
//!
//! Only an item's FIRST marker got that answer. The collector hands the sub-list
//! to the list parser and the rest of the body back as a further chunk, so the
//! first marker met no open paragraph while every later one met section 10 I2
//! with one open, and folded. Two documents differing only by a sub-list that
//! had already been closed then disagreed about what their shared last line was
//! (markup-carve/carve#1517).
//!
//! THE REPRODUCTION HAS NO TABLE IN IT. The ticket used one, which made the
//! cause look like something about tables; a blank line closes the sub-list just
//! as well and isolates it.

fn flat(source: &str) -> String {
    let html = carve::to_html(source);
    let mut out = String::new();
    let mut space = false;
    for ch in html.chars() {
        if ch.is_whitespace() {
            space = true;
            continue;
        }
        if space && !out.is_empty() {
            out.push(' ');
        }
        space = false;
        out.push(ch);
    }
    out
}

#[test]
fn it_opens_one_below_a_paragraph_when_a_sublist_has_already_closed() {
    assert_eq!(
        flat("- o\n  - z\n\n  para\n  - s1\n"),
        "<ul> <li><p>o</p> <ul> <li>z</li> </ul> <p>para</p> <ul> <li>s1</li> </ul> </li> </ul>"
    );
}

#[test]
fn the_same_document_without_the_sublist_agrees() {
    // The other half of the pair: one line shorter, and it always opened a
    // sublist because `- s1` was then the item's FIRST marker.
    assert_eq!(
        flat("- o\n\n  para\n  - s1\n"),
        "<ul> <li><p>o</p> <p>para</p> <ul> <li>s1</li> </ul> </li> </ul>"
    );
}

#[test]
fn the_tickets_own_spelling_with_a_table() {
    assert_eq!(
        flat("- o\n  - z\n  | a |\n  para\n  - s1\n"),
        "<ul> <li>o <ul> <li>z</li> </ul> <table> <tbody> <tr><td>a</td></tr> </tbody> </table> \
         para <ul> <li>s1</li> </ul> </li> </ul>"
    );
}

#[test]
fn an_ordered_marker_too_which_the_clause_calls_symmetric() {
    assert_eq!(
        flat("- o\n  - z\n\n  para\n  1. s1\n"),
        "<ul> <li><p>o</p> <ul> <li>z</li> </ul> <p>para</p> <ol> <li>s1</li> </ol> </li> </ul>"
    );
}

#[test]
fn a_task_marker_and_the_abutting_attribute_form() {
    assert!(flat("- o\n  - z\n\n  para\n  - [ ] s1\n").contains("<input"));
    assert!(flat("- o\n  - z\n\n  para\n  -{.k} s1\n").contains("class=\"k\""));
}

#[test]
fn a_sibling_marker_stays_a_sibling_and_stays_tight() {
    // The control: a marker of the sublist already open is not a new child of
    // the item, and must not be given a loosening separator.
    assert_eq!(
        flat("- o\n  - z\n  - w\n"),
        "<ul> <li>o <ul> <li>z</li> <li>w</li> </ul> </li> </ul>"
    );
    assert_eq!(
        flat("- o\n  - z\n  para\n  - s1\n"),
        "<ul> <li>o <ul> <li>z para</li> <li>s1</li> </ul> </li> </ul>"
    );
}

#[test]
fn column_zero_is_unchanged() {
    // Section 24 C3 is a divergence for a container's CONTENT column. The top
    // level is section 10 I2 and does not move.
    assert_eq!(
        flat("| a |\npara\n- s1\n"),
        "<table> <tbody> <tr><td>a</td></tr> </tbody> </table> <p>para - s1</p>"
    );
}

#[test]
fn below_the_content_column_a_marker_still_folds() {
    // Section 24 C3's other band: "BELOW content_column ... a list marker folds
    // as lazy item text". Corpus 05-lists-8.
    assert_eq!(
        flat("1. outer\n  1. inner\n"),
        "<ol> <li>outer 1. inner</li> </ol>"
    );
}

#[test]
fn a_marker_on_a_quote_lazy_continuation_is_still_text() {
    // carve-js#1200, which this does NOT overturn: the quote's open paragraph
    // claims the line before the item's content column does. The sublist arm is
    // waived for the lazy-continuation predicate precisely so this keeps
    // answering the old way, and this engine was always on the right side of it.
    assert_eq!(
        flat("- > q\n  - s\ntail\n"),
        "<ul> <li> <blockquote><p>q - s tail</p></blockquote> </li> </ul>"
    );
}

#[test]
fn it_still_opens_one_when_the_quote_left_no_paragraph_to_fold_into() {
    // The near miss #1200 names: a quote ending on a heading has nothing open,
    // so the marker reaches the item body and section 24 C3 opens the sublist.
    assert!(flat("- > # h\n  - s\n").contains("<ul> <li>"));
}
