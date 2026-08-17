//! A block-attribute block written across two lines (`{.k` / `#x}`) is a
//! floating attribute like the single-line spelling, so it interrupts an open
//! paragraph and it is what a container's body ENDS in
//! (markup-carve/carve-rs#1039, markup-carve/carve#1281's `329-...-6`).
//!
//! Only the single-line form was ever tested. The wrapped one folded into the
//! open paragraph as literal text, so the author's braces reached the page and
//! the attributes reached nothing.
//!
//! TWO PREDICATES HAD THE SAME GAP and both are needed for the corpus document.
//! One decides whether the block INTERRUPTS, which is what stops it becoming
//! paragraph text. The other decides whether a container's body ENDS in an
//! attribute block, which is what lets a following column-0 line close the
//! container instead of folding in (PART 1 S4, ruled uniform in
//! markup-carve/carve#1280). Fixing either alone leaves the document wrong in a
//! different way, and each has its own mutation.
//!
//! carve-js answers the top-level rows below and was the oracle for them. The
//! corpus is the oracle for the container row, where no engine was correct: at
//! the revisions measured (2026-08-17) carve-js attaches the attribute to the
//! folded line INSIDE the `dd`, and carve-php reads the block as literal text
//! exactly as this engine did.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

#[test]
fn a_wrapped_block_interrupts_an_open_paragraph() {
    // The top-level shape, and the one carve-js already answered this way.
    assert_eq!(
        html("d\n{.k\n#x}\ntail\n"),
        "<p>d</p>\n<p class=\"k\" id=\"x\">tail</p>"
    );
}

#[test]
fn a_containers_body_may_end_in_a_wrapped_block() {
    // The corpus document. The block leaves no open paragraph, so the column-0
    // line ends the `dl`; having no block to attach to, the attribute is
    // dropped in scope.
    assert_eq!(
        html(":: t\n:  d\n   {.k\n   #x}\ntail\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>d</dd>\n</dl>\n<p>tail</p>"
    );
}

#[test]
fn a_wrapped_block_still_attaches_to_the_block_below_it() {
    // Interrupting is not dropping: with a block after it, the attribute lands.
    assert_eq!(html("{.k\n#x}\ntail\n"), "<p class=\"k\" id=\"x\">tail</p>");
}

// ---------------------------------------------------------------------------
// CONTROLS. Each passed before the fix and pins a row it must not move.
// ---------------------------------------------------------------------------

#[test]
fn control_the_single_line_spelling_is_unchanged() {
    assert_eq!(html("d\n{.k}\ntail\n"), "<p>d</p>\n<p class=\"k\">tail</p>");
    assert_eq!(
        html(":: t\n:  d\n   {.k}\ntail\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>d</dd>\n</dl>\n<p>tail</p>"
    );
}

#[test]
fn control_an_indented_brace_line_is_still_paragraph_text() {
    // The strict column-0 rule (PART 9 section 24 C3). An indented `{...}` is
    // lazy paragraph text, and the wrapped reader is flush-left only - so this
    // must not start a block either.
    //
    // Asserted on the braces rather than on the whole string: `#x` renders as a
    // tag span here, which is inline behavior this rule has nothing to do with.
    let out = html("d\n  {.k\n  #x}\n");
    assert!(out.starts_with("<p>d\n{.k\n"), "{out}");
    assert!(!out.contains("class=\"k\""), "{out}");
}

#[test]
fn control_a_blank_line_refuses_the_join() {
    // An attribute block is refused at a blank line, so these are two
    // paragraphs' worth of literal text rather than one block.
    let out = html("d\n{.k\n\n#x}\n");
    assert!(out.contains("{.k"), "{out}");
}

#[test]
fn control_a_complete_but_invalid_single_line_is_not_rescued() {
    // A line that already closes with `}` and was rejected must NOT be joined
    // with later lines - that path parses an interior `}{` as an unquoted value
    // and swallows the run. The wrapped reader keeps that refusal.
    let out = html("d\n{k=v}{+i+}\ntail\n");
    assert!(out.contains("{k=v}"), "{out}");
}

#[test]
fn control_a_quote_left_open_across_the_break_refuses_the_block() {
    // A quoted value stops at the newline (PART 4, markup-carve/carve#888), so
    // this is not an attribute block and stays text.
    let out = html("d\n{k=\"v\n#x}\ntail\n");
    assert!(out.contains("{k="), "{out}");
}
