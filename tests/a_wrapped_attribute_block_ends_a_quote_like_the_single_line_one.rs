//! ONE BLOCK IS ONE BLOCK HOWEVER MANY LINES IT TAKES (§15 A5). A standalone
//! attribute block written `{.k}` and one written `{.k` / `#x}` are the same
//! block, so a block quote whose last content is one of them has to end the
//! same way for both.
//!
//! It did not. `b6ff319` closed the wrapped gap at the top level and for a
//! definition body, and inside a block quote the wrapped spelling went on
//! attaching FORWARD, out of the container: the quote kept collecting, the
//! column-0 line folded in, and the author's attributes landed on it INSIDE the
//! quote they were written to end (carve-rs#1050). The single-line spelling of
//! the identical document, one line shorter, closed the quote.
//!
//! THE PAIRING IS WHAT IS ASSERTED. Every row below writes both spellings of the
//! same document and demands the same bytes, because a literal on each side
//! would let them drift apart again - which is how the second answer got in.
//!
//! `ParaOpen` decides from ONE line and passes an empty rest slice to
//! `interrupts_paragraph_with_rest`, so the block's extent is tracked in the
//! quote collector beside the code fence's. That is the same shape for the same
//! reason: a construct whose extent outruns the line the predicate is handed.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

/// The wrapped spelling of `single`, with the block's one line split in two.
fn assert_both_spellings(single: &str, wrapped: &str, expected: &str) {
    assert_eq!(html(single), expected, "single-line spelling");
    assert_eq!(html(wrapped), expected, "wrapped spelling");
}

#[test]
fn a_block_after_quoted_content_ends_the_quote() {
    assert_both_spellings(
        "> q\n> {.k #x}\ntail\n",
        "> q\n> {.k\n> #x}\ntail\n",
        "<blockquote><p>q</p></blockquote>\n<p>tail</p>",
    );
}

#[test]
fn a_block_at_the_top_of_a_quote_ends_it_too() {
    // Nothing precedes it, so the quote holds no paragraph at all and the
    // column-0 line reaches no container.
    assert_both_spellings(
        "> {.k #x}\ntail\n",
        "> {.k\n> #x}\ntail\n",
        "<blockquote>\n\n</blockquote>\n<p>tail</p>",
    );
}

#[test]
fn a_block_still_attaches_to_the_next_block_inside_the_quote() {
    // ENDING THE PARAGRAPH IS NOT LEAVING THE QUOTE. The attributes float
    // forward to the block below them; when that block is inside the quote,
    // that is where they land. Only the escape out of the container was wrong.
    assert_both_spellings(
        "> q\n> {.k #x}\n> after\n",
        "> q\n> {.k\n> #x}\n> after\n",
        "<blockquote>\n  <p>q</p>\n  <p class=\"k\" id=\"x\">after</p>\n</blockquote>",
    );
}

#[test]
fn a_block_at_depth_two_answers_like_one_at_depth_one() {
    // DEPTH IS NOT A PARAMETER (markup-carve/carve#506). The lookahead walks to
    // the innermost quoted content, exactly as `ParaOpen` does, so a block
    // written under two markers ends the paragraph the same way.
    assert_both_spellings(
        "> > q\n> > {.k #x}\ntail\n",
        "> > q\n> > {.k\n> > #x}\ntail\n",
        "<blockquote>\n  <blockquote><p>q</p></blockquote>\n</blockquote>\n<p>tail</p>",
    );
}

#[test]
fn a_second_block_after_a_run_that_opened_nothing() {
    // The scan's own short-circuit under test: `{.k` opens nothing here, and the
    // window it walked is skipped for the lines behind it - but `{.j` / `#y}`
    // still has to be found, because the walk met a line that CAN close a block
    // and therefore proved nothing about the lines it passed.
    assert_both_spellings(
        "> a\n> {.k\n> b\n> {.j #y}\ntail\n",
        "> a\n> {.k\n> b\n> {.j\n> #y}\ntail\n",
        "<blockquote><p>a\n{.k\nb</p></blockquote>\n<p>tail</p>",
    );
}

#[test]
fn control_braces_that_never_close_stay_paragraph_text() {
    // Not an attribute block at all, so nothing ends and the column-0 line folds
    // in as lazy continuation. This is the row that stops the fix from reading
    // as "any quoted line starting with a brace closes the paragraph".
    assert_eq!(
        html("> q\n> {.k\n> and this never closes\ntail\n"),
        "<blockquote><p>q\n{.k\nand this never closes\ntail</p></blockquote>"
    );
}

#[test]
fn control_a_blank_line_inside_the_braces_refuses_the_block() {
    // PART 4 refuses a block across a blank line, and a blank ends the quote
    // anyway. Both halves of the document stay quotes and the braces are text.
    assert_eq!(
        html("> q\n> {.k\n\n> #x}\ntail\n"),
        "<blockquote><p>q\n{.k</p></blockquote>\n<blockquote><p><span class=\"tag\"><strong>#x</strong></span>}\ntail</p></blockquote>"
    );
}

#[test]
fn control_an_indented_wrapped_block_is_lazy_text() {
    // FLUSH-LEFT ONLY, under the strict column-0 rule: an indented `{…}` inside
    // the quote is paragraph text, so the quote goes on collecting. The
    // single-line spelling already answered this way and the wrapped one has to
    // agree with it, not with the flush-left rows above.
    let expected = "<blockquote><p>q\n{.k\n<span class=\"tag\"><strong>#x</strong></span>}\ntail</p></blockquote>";
    assert_eq!(html("> q\n>  {.k\n>  #x}\ntail\n"), expected);
}

#[test]
fn a_block_whose_closer_sits_at_a_shallower_depth() {
    // THE LOOKAHEAD READS THE INNERMOST CONTENT OF EACH LINE, so an opener at
    // depth two and a closer at depth one join. Refusing the join on a depth
    // mismatch was considered and rejected: it puts this document straight back
    // on the answer the ticket is about, with `tail` INSIDE the inner quote
    // wearing attributes written to end it, which is what this engine published
    // before the fix.
    //
    // The two brace lines then reach the quote's own body parse, where an
    // attribute block with nothing after it attaches to nothing and is dropped.
    // That half is untouched here and carve-js agrees with it byte for byte.
    // What the row pins is the STRUCTURE: the paragraph closes and the
    // column-0 line reaches no container.
    assert_eq!(
        html("> > q\n> > {.k\n> #x}\ntail\n"),
        "<blockquote>\n  <blockquote><p>q</p></blockquote>\n</blockquote>\n<p>tail</p>"
    );
    // The SINGLE-LINE spelling is a different document, not a twin: `{.k}` is
    // complete at depth two, so `#x}` is ordinary content of the OUTER quote
    // and the flush-left line folds into it. The lookahead refuses a complete
    // line outright, which is what keeps these two apart.
    assert_eq!(
        html("> > q\n> > {.k}\n> #x}\ntail\n"),
        "<blockquote>\n  <blockquote><p>q</p></blockquote>\n  <p><span class=\"tag\"><strong>#x</strong></span>}\ntail</p>\n</blockquote>"
    );
}

#[test]
fn control_a_quoted_paragraph_still_takes_the_fold() {
    // The shape the whole rule is about NOT closing. Without it every row above
    // would still pass if the quote simply stopped folding.
    assert_eq!(
        html("> q\n> more\ntail\n"),
        "<blockquote><p>q\nmore\ntail</p></blockquote>"
    );
}
