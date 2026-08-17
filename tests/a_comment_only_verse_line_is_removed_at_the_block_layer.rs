//! A comment-only line in a line block is decided BEFORE any inline content
//! exists (grammar PART 9 §23, IT IS REMOVED AT THE BLOCK LAYER; carve#1333).
//!
//! The defect these pin was a PUBLICATION: with the removal left to the inline
//! parser, §21's verbatim exclusion claimed the line first, so a stray backtick
//! anywhere above put the comment's own text into the rendered code span. The
//! document's only defect was the typo.
//!
//! The invariants are the ones a render assertion cannot see. `to_html` is
//! equal across the writer for every shape here even when the writer is wrong,
//! which is why these shapes survived: what separates a correct tree from a
//! plausible one is `parse(fmt(x)) == parse(x)`, and the bytes.

/// PART 11 §1's two properties, asserted on the TREE rather than the render.
fn round_trips(source: &str) {
    let out = carve::to_carve(source);
    // The TREE, not the whole `Document`: `source_len` counts the bytes that
    // were read, and a writer that changes bytes is the thing under test.
    assert_eq!(
        carve::parse(&out).children,
        carve::parse(source).children,
        "parse(fmt(x)) != parse(x)\n  source: {source:?}\n  fmt:    {out:?}"
    );
    assert_eq!(
        carve::to_carve(&out),
        out,
        "fmt is not idempotent\n  source: {source:?}"
    );
}

/// The reported shape: an unclosed run opened one line ABOVE the comment.
///
/// The run reaches the end of the block (carve#1282) and carries the emptied
/// line as a newline, like every other break it swallows. What it may never
/// carry is `secret`.
#[test]
fn an_unclosed_run_above_it_cannot_claim_the_comment() {
    let html = carve::to_html("::: |\na `b\n%% secret\nc\n:::\n");

    assert!(
        !html.contains("secret"),
        "the comment was published: {html}"
    );
    assert_eq!(
        html,
        "<div class=\"line-block\">\n  <p>a <code>b\n\nc</code></p>\n</div>"
    );
}

/// The control beside it: the same three lines with no run open. Both halves of
/// the pair have to hold, or the fix is just "do not open a code span".
#[test]
fn a_later_comment_line_leaves_an_empty_verse_line() {
    assert_eq!(
        carve::to_html("::: |\na\n%% secret\nc\n:::\n"),
        "<div class=\"line-block\">\n  <p>a<br>\n<br>\nc</p>\n</div>"
    );
    round_trips("::: |\na\n%% secret\nc\n:::\n");
}

/// A comment on the stanza's FIRST body line. The line was written, so the
/// stanza keeps its shape and the boundary below it still hardens.
#[test]
fn a_first_comment_line_leaves_an_empty_verse_line() {
    assert_eq!(
        carve::to_html("::: |\n%% first\na\n:::\n"),
        "<div class=\"line-block\">\n  <p><br>\na</p>\n</div>"
    );
    round_trips("::: |\n%% first\na\n:::\n");
}

/// AN EMPTY VERSE LINE IS A LINE, NOT A BREAK: a comment ending the stanza adds
/// nothing after the break that already separates it from the line before.
#[test]
fn a_comment_ending_the_stanza_adds_no_second_break() {
    assert_eq!(
        carve::to_html("::: |\na\n%% c\n:::\n"),
        "<div class=\"line-block\">\n  <p>a<br>\n</p>\n</div>"
    );
    round_trips("::: |\na\n%% c\n:::\n");
}

/// LEADING WHITESPACE IS CONTENT HERE, so `comment_line`'s optional whitespace
/// prefix has nothing to consume in verse: only a line whose FIRST character is
/// `%` is a comment line, and an indented one is ordinary text whose leading run
/// serializes as NBSPs like any other.
#[test]
fn an_indented_comment_line_stays_verse() {
    let html = carve::to_html("::: |\n  %% indented\na\n:::\n");

    assert!(
        html.contains("&nbsp;&nbsp;%% indented"),
        "the indented line was removed: {html}"
    );
    round_trips("::: |\n  %% indented\na\n:::\n");
}

/// REMOVED FROM THE RENDER, NOT FROM THE TREE: the author wrote it, so PART 12's
/// `comment` node records it - at the same column, because a writer that indents
/// it by a space publishes the very text the removal exists to hide.
#[test]
fn the_comment_is_still_a_node_and_writes_back_at_its_own_column() {
    let json = carve::to_json(&carve::parse("::: |\na\n%% secret\nc\n:::\n"));

    assert!(
        json.contains("\"type\":\"comment\""),
        "the comment left no node: {json}"
    );
    assert_eq!(
        carve::to_carve("::: |\na\n%% secret\nc\n:::\n"),
        "::: |\na\n%% secret\nc\n:::\n"
    );

    // And it is placed where the author wrote it, over the marker and its
    // content: the emptied line gives the node no span of its own.
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options("::: |\na\n%% secret\nc\n:::\n", &options);
    let mut found = 0;
    for block in &doc.children {
        let carve::ast::BlockNode::LineBlock(lb) = block else {
            continue;
        };
        for child in &lb.children {
            let carve::ast::BlockNode::Paragraph(p) = child else {
                continue;
            };
            for inline in &p.children {
                let carve::ast::InlineNode::Comment(c) = inline else {
                    continue;
                };
                found += 1;
                let pos = c.pos.expect("the comment is placed");
                assert_eq!(pos.start_line, 3);
                assert_eq!(pos.start_column, 1);
                assert_eq!(pos.end_line, 3);
                // `%% secret` is 9 characters.
                assert_eq!(pos.end_column, 10);
            }
        }
    }
    assert_eq!(found, 1, "expected exactly one comment node");
}

/// A TRAILING COMMENT IS A DIFFERENT CONSTRUCT and this clause does not reach
/// it: `x %% secret` is `inline_comment`, and inside a verbatim run there is no
/// comment there at all. An engine may leave a `%%` standing inside a run, and
/// may never delete author bytes out of one.
#[test]
fn a_trailing_comment_inside_a_run_stays_content() {
    assert_eq!(
        carve::to_html("::: |\na `b\nx %% secret\nc\n:::\n"),
        "<div class=\"line-block\">\n  <p>a <code>b\nx %% secret\nc</code></p>\n</div>"
    );
}
