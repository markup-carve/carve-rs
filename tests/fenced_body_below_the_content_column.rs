//! A FENCED BODY IS NOT A PARAGRAPH, so a line below a list item's content
//! column closes the item instead of folding into the code text (PART 9 §24,
//! markup-carve/carve#950, carve-rs#770).
//!
//! §24's STEP algorithm decides it without a new rule: the below-column line
//! supplies none of the body's indentation, so S1 stops at the ITEM and S2
//! never reaches the fenced body; S4 governs, and its lazy branch wants an open
//! PARAGRAPH, which a verbatim body is not. The containers close, the item
//! holds an EMPTY code block, and the residue re-parses in the surviving
//! context. The block-quote spelling of the shape has always answered this way.
//!
//! Corpus category 276 pins the seven rows this file does not repeat. What it
//! covers instead is what the corpus leaves open: the ordered-marker column,
//! the CLOSED fence (where the fold is still correct), a sibling item after the
//! break, the closer that itself sits below the column, and the fences neither
//! the marker line nor the lead paragraph opens - the ones the item's block
//! collectors have to follow on their own.

use carve::{to_html, to_html_with_options, Options};

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The same document through the position-carrying parse. The two collectors
/// are separate walks - a plain render runs one, positions the other - so a
/// guard written in only one of them renders two documents.
fn squash_with_positions(src: &str) -> String {
    let options = Options {
        positions: true,
        ..Default::default()
    };
    squash(&to_html_with_options(src, &options))
}

#[test]
fn a_below_column_line_closes_the_item_and_leaves_the_code_block_empty() {
    // Corpus 276 row 2, one column in.
    assert_eq!(
        squash(&to_html("- ```\n x\n ```\n")),
        "<ul> <li> <pre><code> </code></pre> </li> </ul> <p>x <code></code></p>"
    );
}

#[test]
fn the_ordered_marker_column_is_the_items_own() {
    // `1. ` puts the content column at 3, so a line one column in is below it
    // for the same reason - the column is read off the marker, not fixed at 2.
    assert_eq!(
        squash(&to_html("1. ```\n x\n ```\n")),
        "<ol> <li> <pre><code> </code></pre> </li> </ol> <p>x <code></code></p>"
    );
}

#[test]
fn the_guard_is_on_the_open_fence_not_on_the_marker_line() {
    // Row 6: the body has already collected a line AT the content column, so a
    // reader tracking "is a paragraph open" sees one again. Only the fence
    // answers this row and the marker-line one together.
    assert_eq!(
        squash(&to_html("- ```\n  x\n y\n  ```\n")),
        "<ul> <li> <pre><code>x </code></pre> </li> </ul> <p>y <code></code></p>"
    );
    // Row 7: the fence is opened on a CONTINUATION line. The item still closes
    // at the below-column line, and what the truncated item holds is §10 I4's
    // business - a fence with no closer left inside it does not interrupt, so
    // the delimiter run is paragraph text.
    assert_eq!(
        squash(&to_html("- a\n  ```\n  b\n y\n  ```\n")),
        "<ul> <li>a <code> b y </code></li> </ul>"
    );
}

#[test]
fn a_closer_below_the_content_column_is_not_this_fences_closer() {
    // The closer has to be inside the same container. Past the line that closes
    // the item there is none left, so by §10 I4 the fence does not interrupt
    // the lead paragraph at all.
    assert_eq!(
        squash(&to_html("- a\n  ```\n  b\n ```\n")),
        "<ul> <li>a <code> b </code></li> </ul>"
    );
}

#[test]
fn a_closed_fence_leaves_nothing_to_guard() {
    // THE CONTROL for the rule's scope. Once the closer has run, the item holds
    // no open fenced body and a below-column line folds in exactly as it always
    // did. A guard written on "this item once held a fence" fails here.
    assert_eq!(
        squash(&to_html("- a\n  ```\n  b\n  ```\n y\n")),
        "<ul> <li>a <code> b </code> y</li> </ul>"
    );
}

#[test]
fn at_the_content_column_nothing_moves() {
    // Corpus 276 row 3, the shape every other corpus case uses.
    assert_eq!(
        squash(&to_html("- ```\n  x\n  ```\n")),
        "<ul> <li> <pre><code>x </code></pre> </li> </ul>"
    );
    // Including a marker, which at the column is code text like any other line.
    assert_eq!(
        squash(&to_html("- ```\n  - b\n  ```\n")),
        "<ul> <li> <pre><code>- b </code></pre> </li> </ul>"
    );
}

#[test]
fn a_below_column_marker_is_a_below_column_line() {
    // §24's S1 walks the stack by the indentation a line SUPPLIES, which has
    // nothing to do with what the line says. So an indented marker below the
    // content column closes the item exactly as prose does, and only then is
    // the residue classified - where it opens a list of its own (C4 Rule B).
    // Reading the marker first nested it inside the item the open fence closes.
    assert_eq!(
        squash(&to_html("- ```\n - b\n ```\n")),
        "<ul> <li> <pre><code> </code></pre> </li> </ul> \
         <ul> <li>b <code></code></li> </ul>"
    );
    assert_eq!(
        squash(&to_html("- ```\n 1. b\n ```\n")),
        "<ul> <li> <pre><code> </code></pre> </li> </ul> \
         <ol> <li>b <code></code></li> </ol>"
    );
    assert_eq!(
        squash(&to_html("- a\n  ```\n  c\n - b\n  ```\n")),
        "<ul> <li>a <code> c - b </code></li> </ul>"
    );
    // A SIBLING marker at the base column ends the item too, and the list it
    // belongs to carries on - the same answer the item collector's own
    // sibling-marker break has always given.
    assert_eq!(
        squash(&to_html("- ```\n- b\n ```\n")),
        "<ul> <li> <pre><code> </code></pre> </li> <li>b <code></code></li> </ul>"
    );
}

#[test]
fn the_collectors_follow_a_fence_the_marker_line_did_not_open() {
    // The seed covers a fence written ON the marker line and the lead-paragraph
    // loop covers one ABSORBED into the item's first paragraph. Neither reaches
    // a fence the item's BLOCK collector takes: after another marker-line block,
    // or after a blank line. The collectors follow openers themselves for these,
    // and without that the below-column line folds into the code text.
    assert_eq!(
        squash(&to_html("- # h\n  ```\n  b\n y\n  ```\n")),
        "<ul> <li> <h1 id=\"h\">h</h1> <pre><code>b </code></pre> </li> </ul> \
         <p>y <code></code></p>"
    );
    assert_eq!(
        squash(&to_html("- a\n\n  ```\n  b\n y\n  ```\n")),
        "<ul> <li>a <pre><code>b </code></pre> </li> </ul> <p>y <code></code></p>"
    );
    // And they follow CLOSERS: the item's first fence closes, a second one
    // opens, and it is the second that carries the item to the below-column
    // line. A tracker that only ever opens ends the item at the first one.
    assert_eq!(
        squash(&to_html("- ```\n  x\n  ```\n  ```\n  z\n y\n  ```\n")),
        "<ul> <li> <pre><code>x </code></pre> <pre><code>z </code></pre> </li> </ul> \
         <p>y <code></code></p>"
    );
}

#[test]
fn a_sibling_item_carries_none_of_the_previous_items_fence() {
    assert_eq!(
        squash(&to_html("- ```\n  x\n  ```\n- b\n")),
        "<ul> <li> <pre><code>x </code></pre> </li> <li>b</li> </ul>"
    );
    // An UNTERMINATED fence in the first item is still open when the sibling
    // marker ends that item, so the state has to be dropped at the marker. Kept,
    // it closes the list under the NEXT item and `z` leaves it.
    assert_eq!(
        squash(&to_html("- ```\n  x\n- b\n z\n")),
        "<ul> <li> <pre><code>x </code></pre> </li> <li>b z</li> </ul>"
    );
    // And the sibling's own closer-less fence is paragraph text after that
    // drop, so its below-column lazy line remains in the sibling (§10 I4,
    // corpus 367).
    assert_eq!(
        squash(&to_html("- ```\n  x\n- b\n  ```\n  q\n y\n")),
        "<ul> <li> <pre><code>x </code></pre> </li> <li>b <code> q y</code></li> </ul>"
    );
}

#[test]
fn the_position_carrying_parse_reads_the_same_document() {
    for src in [
        "- ```\n x\n ```\n",
        "- ```\n  x\n y\n  ```\n",
        "- a\n  ```\n  b\n y\n  ```\n",
        "- # h\n  ```\n  b\n y\n  ```\n",
        "- a\n  ```\n  b\n  ```\n y\n",
    ] {
        assert_eq!(squash(&to_html(src)), squash_with_positions(src), "{src:?}");
    }
}
