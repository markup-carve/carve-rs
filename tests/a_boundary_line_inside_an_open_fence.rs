//! A boundary line inside an open fence does not end the container
//! (markup-carve/carve#983 corpus category 279, markup-carve/carve#985,
//! markup-carve/carve-rs#802).
//!
//! PART 9 §17 L3 names the block kinds a `+` continuation marker may attach -
//! "ONE block of ANY kind (paragraph, list, fenced code, table, block quote,
//! div, ...)" - and bounds the attachment "up to the next blank line, sibling
//! marker, or a further `+`". Those bound THE BLOCK. A fenced block ends at its
//! CLOSER, which is what makes it one block, so a boundary line written between
//! an opener and its closer is fence content and ends nothing.
//!
//! FIVE COLLECTORS ASKED THE SAME QUESTION AND THIS ENGINE ALREADY ANSWERED IT
//! ONCE. `parse_continuation_block`'s extent scan is fence-aware for all four
//! fence shapes - code/raw, `%%%`, `::: |` and `:::` - which is why the list
//! item was the only container that survived these rows. The footnote body, the
//! block quote and the definition body's two forms consulted no fence at all,
//! each with its own boundary set. They now share that one scan as
//! `attached_block_end`, whose `is_boundary` closure is the only per-container
//! part.
//!
//! A mutation reverting ONE caller fails only that caller's rows; a mutation
//! removing ONE fence shape from the shared scan fails that shape across EVERY
//! caller. That pair of opposite results is what "one spelling" means here.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

/// Collapse whitespace so a row asserts on structure rather than on the
/// renderer's indentation.
fn flat(src: &str) -> String {
    html(src).split_whitespace().collect::<Vec<_>>().join(" ")
}

// A code fence whose body holds a blank line: `a` and `b` are ONE block.
const CODE: &str = "```\na\n\nb\n```\n";
// A colon fence whose body holds a blank line: two paragraphs of ONE aside.
const COLON: &str = "::: note\na\n\nb\n:::\n";
// A comment fence whose body holds a blank line. It renders nothing at all.
const COMMENT: &str = "%%%\na\n\nb\n%%%\n";

const CODE_HTML: &str = "<pre><code>a b </code></pre>";
const COLON_HTML: &str =
    "<aside class=\"admonition note\" aria-label=\"Note\"> <p>a</p> <p>b</p> </aside>";

// ------------------------------------------------------------- block quote

#[test]
fn the_block_quote_collector_keeps_a_code_fence_whole() {
    assert_eq!(
        flat(&format!("> q\n+\n{CODE}")),
        format!("<blockquote> <p>q</p> {CODE_HTML} </blockquote>")
    );
}

#[test]
fn the_block_quote_collector_keeps_a_colon_fence_whole() {
    assert_eq!(
        flat(&format!("> q\n+\n{COLON}")),
        format!("<blockquote> <p>q</p> {COLON_HTML} </blockquote>")
    );
}

#[test]
fn the_block_quote_collector_keeps_a_comment_fence_whole() {
    assert_eq!(
        flat(&format!("> q\n+\n{COMMENT}")),
        "<blockquote> <p>q</p> </blockquote>"
    );
}

/// A DEFINITION LINE inside the fence body is body text and defines nothing.
/// §28 makes a fence body verbatim and §17 L6 collection cannot reach into it -
/// and L3's boundary list does not name a definition line at all.
#[test]
fn the_block_quote_collector_keeps_a_definition_line_as_fence_body() {
    assert_eq!(
        flat("> q\n+\n:::\na\n[^z]: zz\nb\n:::\n"),
        "<blockquote> <p>q</p> <div> <p>a</p> <p>b</p> </div> </blockquote>"
    );
}

// ---------------------------------------------------------- footnote body

fn note(body: &str) -> String {
    format!(
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p> \
         <section role=\"doc-endnotes\" aria-label=\"Footnotes\"> <hr> <ol> <li id=\"fn1\"> <p>n</p> {body} \
         <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p> </li> </ol> </section>"
    )
}

#[test]
fn the_footnote_collector_keeps_a_code_fence_whole() {
    assert_eq!(
        flat(&format!("[^f]: n\n+\n{CODE}\nsee[^f]\n")),
        note(CODE_HTML)
    );
}

#[test]
fn the_footnote_collector_keeps_a_colon_fence_whole() {
    assert_eq!(
        flat(&format!("[^f]: n\n+\n{COLON}\nsee[^f]\n")),
        note(COLON_HTML)
    );
}

#[test]
fn the_footnote_collector_keeps_a_definition_line_as_fence_body() {
    assert_eq!(
        flat("[^f]: n\n+\n```\na\n[^z]: zz\nb\n```\n\nsee[^f]\n"),
        note("<pre><code>a [^z]: zz b </code></pre>")
    );
}

// -------------------------------------------------------- definition body

#[test]
fn the_definition_body_collector_keeps_a_code_fence_whole() {
    assert_eq!(
        flat(&format!(":: t\n:  d\n+\n{CODE}")),
        format!("<dl> <dt>t</dt> <dd> <p>d</p> {CODE_HTML} </dd> </dl>")
    );
}

#[test]
fn the_definition_body_collector_keeps_a_colon_fence_whole() {
    assert_eq!(
        flat(&format!(":: t\n:  d\n+\n{COLON}")),
        format!("<dl> <dt>t</dt> <dd> <p>d</p> {COLON_HTML} </dd> </dl>")
    );
}

#[test]
fn the_first_block_definition_collector_keeps_a_code_fence_whole() {
    assert_eq!(
        flat(&format!(":: t\n:  +\n{CODE}")),
        format!("<dl> <dt>t</dt> <dd> {CODE_HTML} </dd> </dl>")
    );
}

#[test]
fn the_first_block_definition_collector_keeps_a_colon_fence_whole() {
    assert_eq!(
        flat(&format!(":: t\n:  +\n{COLON}")),
        format!("<dl> <dt>t</dt> <dd> {COLON_HTML} </dd> </dl>")
    );
}

// ----------------------------------------------------------- the list item
//
// This container already answered every row here. They are measured beside the
// others so the shared scan is pinned from the caller that had it first: a
// change that broke it would otherwise only show up as a corpus failure.

#[test]
fn the_list_item_collector_keeps_a_code_fence_whole() {
    assert_eq!(
        flat(&format!("- x\n+\n{CODE}\nz\n")),
        format!("<ul> <li>x {CODE_HTML} </li> </ul> <p>z</p>")
    );
}

#[test]
fn the_list_item_collector_keeps_a_colon_fence_whole() {
    assert_eq!(
        flat(&format!("- x\n+\n{COLON}\nz\n")),
        format!("<ul> <li>x {COLON_HTML} </li> </ul> <p>z</p>")
    );
}

/// A `::: |` LINE BLOCK is the fourth fence shape the scan knows, and a
/// different detector reaches it than an ordinary `:::` - so a row that only
/// ever writes `::: note` leaves it unpinned. Found by a mutation that removed
/// the line-block branch and stayed green.
#[test]
fn the_list_item_collector_keeps_a_line_block_whole() {
    assert_eq!(
        flat("- x\n+\n::: |\na\n\nb\n:::\n\nz\n"),
        "<ul> <li>x <div class=\"line-block\"> <p>a</p> <p>b</p> </div> </li> </ul> <p>z</p>"
    );
}

#[test]
fn the_block_quote_collector_keeps_a_line_block_whole() {
    assert_eq!(
        flat("> q\n+\n::: |\na\n\nb\n:::\n"),
        "<blockquote> <p>q</p> <div class=\"line-block\"> <p>a</p> <p>b</p> </div> </blockquote>"
    );
}

/// THE LIST ITEM'S BOUNDARY SET HAS NO BLANK-LINE CLAUSE, so its rows above
/// pass whether or not the scan is fence-aware - a mutation reverting this
/// caller left every one of them green. What the fence rule decides HERE is a
/// SIBLING MARKER and a FURTHER `+` written inside the attached fence: §17 L5
/// makes a lone `+` inside a fenced container literal text, and §28 makes the
/// body verbatim, so both are content. Found by diagnosing that green mutation
/// rather than accepting it.
#[test]
fn a_marker_inside_the_attached_code_fence_is_code_text() {
    assert_eq!(
        flat("- x\n+\n```\na\n- m\nb\n```\n"),
        "<ul> <li>x <pre><code>a - m b </code></pre> </li> </ul>"
    );
}

#[test]
fn a_further_plus_inside_the_attached_code_fence_is_code_text() {
    assert_eq!(
        flat("- x\n+\n```\na\n+\nb\n```\n"),
        "<ul> <li>x <pre><code>a + b </code></pre> </li> </ul>"
    );
}

#[test]
fn a_marker_inside_the_attached_colon_fence_is_body_text() {
    assert_eq!(
        flat("- x\n+\n::: note\na\n- m\nb\n:::\n"),
        "<ul> <li>x <aside class=\"admonition note\" aria-label=\"Note\"> <p>a - m b</p> </aside> </li> </ul>"
    );
}

// ------------------------------------------------- the item's INDENTED body

/// Not a `+` path: the indented body is collected line by line against a
/// running tracker, and the boundary at issue is the sibling marker rather than
/// the blank. §24 S1/S2 place a line by the column it REACHES and never by its
/// first character.
#[test]
fn the_indented_body_keeps_a_colon_fence_whole_across_a_marker() {
    assert_eq!(
        flat("- x\n  :::\n  a\n  - m\n  b\n  :::\n"),
        "<ul> <li>x <div> <p>a - m b</p> </div> </li> </ul>"
    );
}

#[test]
fn the_indented_body_keeps_a_code_fence_whole_across_a_marker() {
    assert_eq!(
        flat("- x\n  ```\n  a\n  - m\n  b\n  ```\n"),
        "<ul> <li>x <pre><code>a - m b </code></pre> </li> </ul>"
    );
}

/// A COLON LINE INSIDE A CODE OR COMMENT SPAN OPENS AND CLOSES NOTHING. §28
/// makes both bodies verbatim, which is the same reading that keeps a MARKER in
/// one from being a marker.
///
/// This is the item tracker's own failure mode, found by probing it rather than
/// by a corpus row: a `:::` written inside a code fence pushed onto the width
/// stack and never came off, so the item's next REAL `:::` opener matched that
/// ghost as a closer, the stack emptied one level early, and the marker gate
/// severed the very div the tracker exists to keep whole. The plain-body row
/// beside it is the control - the two documents differ by three characters
/// inside a verbatim body and must render the same.
#[test]
fn a_colon_line_inside_a_code_fence_does_not_disturb_the_item_colon_tracker() {
    let with_colon = flat("- x\n  ```\n  :::\n  ```\n  :::\n  a\n  - m\n  b\n  :::\n");
    let with_plain = flat("- x\n  ```\n  zzz\n  ```\n  :::\n  a\n  - m\n  b\n  :::\n");

    assert_eq!(
        with_colon,
        "<ul> <li>x <pre><code>::: </code></pre> <div> <p>a - m b</p> </div> </li> </ul>"
    );
    assert_eq!(
        with_plain,
        "<ul> <li>x <pre><code>zzz </code></pre> <div> <p>a - m b</p> </div> </li> </ul>"
    );
}

#[test]
fn a_colon_line_inside_a_comment_fence_does_not_disturb_the_item_colon_tracker() {
    assert_eq!(
        flat("- x\n  %%%\n  :::\n  %%%\n  :::\n  a\n  - m\n  b\n  :::\n"),
        "<ul> <li>x <div> <p>a - m b</p> </div> </li> </ul>"
    );
}

// ------------------------------------------- markup-carve/carve#985 looseness
//
// Measured with ONE WORD OF LEAD TEXT on the item, and with the genuine-loose
// control below. Without that control a pass proves nothing: an engine that
// simply stopped loosening items would satisfy every row here.

#[test]
fn a_blank_inside_an_indented_comment_fence_does_not_loosen_the_item() {
    assert_eq!(
        flat("- x\n  %%%\n  a\n\n  b\n  %%%\n"),
        "<ul> <li>x</li> </ul>"
    );
}

#[test]
fn a_blank_inside_an_indented_colon_fence_does_not_loosen_the_item() {
    assert_eq!(
        flat("- x\n  :::\n  a\n\n  b\n  :::\n"),
        "<ul> <li>x <div> <p>a</p> <p>b</p> </div> </li> </ul>"
    );
}

/// AN UNTERMINATED OPENER RUNS TO THE END OF THE ITEM, and its interior blank
/// is its own content - so the item is TIGHT, exactly as it is with the closer
/// written two tests above. This asserted LOOSE until markup-carve/carve#1632
/// ruled that an explicit closer is a spelling change tightness may not move
/// across, and pinned this document as corpus
/// `362-an-unterminated-container-does-not-extend-the-item-past-a-blank-line-5`
/// (markup-carve/carve-rs#1307).
#[test]
fn an_unterminated_colon_fence_reaches_to_the_end_of_the_item() {
    assert_eq!(
        flat("- x\n  :::\n  a\n\n  b\n"),
        "<ul> <li>x <div> <p>a</p> <p>b</p> </div> </li> </ul>"
    );
    // And the closer is a spelling: the same document with `  :::` appended
    // reads the same way.
    assert_eq!(
        flat("- x\n  :::\n  a\n\n  b\n  :::\n"),
        flat("- x\n  :::\n  a\n\n  b\n")
    );
}

/// THE LATCH GUARD THE TEST ABOVE USED TO CARRY, kept and made to discriminate.
///
/// Skipping an opener's span must not suppress a LATER loosening. The worry was
/// that answering end-of-input for an unterminated opener would swallow the rest
/// of the item, including a blank after a genuinely CLOSED span further down.
///
/// It cannot: once an opener has no closer, everything below it IS its content,
/// so there is no later item-level blank to miss. What has to keep working is
/// the CLOSED case, where the span ends and item-level content resumes - and it
/// is checked for all three fence kinds, because the skip is spelled once per
/// kind. Each of these is loose, and each would go tight if the skip ran on to
/// the end of the item instead of stopping at the closer.
#[test]
fn a_closed_span_does_not_suppress_the_loosening_after_it() {
    // EVERY ROW IS EVALUATED. Asserting inside the loop stops the test at the
    // first failing row, so the rows below it would not be measured at all - and
    // "the test went red" would only ever prove something about the first one.
    let mut suppressed = Vec::new();
    for source in [
        "- x\n  ::: a\n  b\n  :::\n\n  c\n",
        "- x\n  %%%\n  b\n  %%%\n\n  c\n",
        "- x\n  ```\n  b\n  ```\n\n  c\n",
    ] {
        if !flat(source).contains("<li><p>x</p>") {
            suppressed.push(format!("{source:?} -> {}", flat(source)));
        }
    }
    assert!(
        suppressed.is_empty(),
        "a closed span suppressed the loosening after it:\n{}",
        suppressed.join("\n")
    );
}

/// THE GENUINE-LOOSE CONTROL.
#[test]
fn a_genuine_blank_still_loosens_the_item() {
    assert_eq!(
        flat("- x\n\n  y\n"),
        "<ul> <li><p>x</p> <p>y</p> </li> </ul>"
    );
}

// ------------------------------------------------------------------ controls
//
// Each of these holds byte-identically BEFORE the fix. They pin the part of L3
// the fix must NOT move.

#[test]
fn an_unfenced_attached_block_still_ends_at_the_blank_line() {
    assert_eq!(
        flat("- x\n+\np\n\nz\n"),
        "<ul> <li>x p </li> </ul> <p>z</p>"
    );
}

#[test]
fn an_attached_block_still_ends_at_a_sibling_marker() {
    assert_eq!(
        flat("- x\n+\np\n- y\n"),
        "<ul> <li>x p </li> <li>y</li> </ul>"
    );
}

#[test]
fn an_attached_block_still_ends_at_a_further_plus() {
    assert_eq!(flat("- x\n+\np\n+\nq\n"), "<ul> <li>x p q </li> </ul>");
}

#[test]
fn a_quote_attached_block_still_ends_at_a_quote_line() {
    assert_eq!(
        flat("> q\n+\np\n> r\n"),
        "<blockquote> <p>q</p> <p>p</p> <p>r</p> </blockquote>"
    );
}
