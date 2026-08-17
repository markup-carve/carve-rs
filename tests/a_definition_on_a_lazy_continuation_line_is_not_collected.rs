//! A footnote definition behind a LIST MARKER on a lazy continuation line is
//! paragraph text, and the definition pre-pass may not cut it out of the line
//! (markup-carve/carve-rs#1024).
//!
//! §10 says a list does NOT interrupt an open paragraph, so `r` then
//! `. [^f]: t` is one paragraph holding both lines. carve-rs agreed about the
//! paragraph and disagreed about the definition: the line-based pre-pass
//! stripped the `. ` marker, recognised a definition behind it and removed the
//! definition's text from the body. Both halves of that are visible damage -
//! the author's `[^f]: t` disappeared from the page, and a later `[^f]`
//! resolved against a note that only this engine believed existed.
//!
//! THE GUARD ASKS THE BLOCK PARSER; IT DOES NOT ENUMERATE. A first attempt
//! answered "is a paragraph open?" by listing the openers a line can be and
//! calling every unlisted line ordinary paragraph text. That tail is the whole
//! problem: an opener nobody listed answered "a paragraph is open", and that
//! answer SUPPRESSES a collection the engine used to make - a silent behavior
//! change rather than a missed improvement. Four missing openers were closed
//! and one review pass found three more of the same class; a custom
//! `match_block` extension is a fourth that cannot be listed at all. The list
//! is unbounded by construction.
//!
//! So `line_folds_into_an_open_paragraph` hands the run back to the block
//! parser twice, once without the line and once with it, and compares the two
//! open frames. A line that folds into an open paragraph adds no node anywhere;
//! a line that opens ANYTHING changes a count along that chain. The rows under
//! "shapes the enumeration got wrong" below are that difference, one test each -
//! aggregate green is exactly where one uncovered path hides.
//!
//! THE QUOTE IS THE CONTROL THAT DECIDES THE SCOPE. `>` DOES interrupt a
//! paragraph, so `r` then `> [^f]: t` opens a real quote and the definition IS
//! collected from it. The marker test that scopes the guard reads the RAW line,
//! so a quoted marker never reaches it, which is what keeps that row collecting.
//!
//! AND THE HEADING IS THE CONTROL THAT KILLS THE CHEAP SPELLING. A heading is
//! not blank and leaves no open paragraph, so "the previous line is not blank"
//! refuses a collection carve-js and carve-php both make. The link-reference
//! pre-pass in this engine still carries that spelling and answers `# r` /
//! `- [d]: u` differently from carve-js; that is filed separately rather than
//! swept in here.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

#[test]
fn the_definition_text_survives_and_defines_nothing() {
    // The ticket's document. Both failures show at once: `[^f]: t` is back on
    // the page, and the `[^f]` below stays literal because nothing defined it.
    assert_eq!(
        html("r\n. [^f]: t\n\n[^f] ref\n"),
        "<p>r\n. [^f]: t</p>\n<p>[^f] ref</p>"
    );
}

#[test]
fn every_list_spelling_answers_the_same_way() {
    // The marker dialect changes numbering only. All three were collected.
    assert_eq!(html("r\n- [^f]: t\n"), "<p>r\n- [^f]: t</p>");
    assert_eq!(html("r\n. [^f]: t\n"), "<p>r\n. [^f]: t</p>");
    assert_eq!(html("r\n1. [^f]: t\n"), "<p>r\n1. [^f]: t</p>");
}

#[test]
fn no_endnote_section_is_emitted() {
    // The second half of the damage, asserted on its own: a document with no
    // definition in it must not grow an endnotes section.
    let out = html("r\n. [^f]: t\n\n[^f] ref\n");
    assert!(
        !out.contains("doc-endnotes"),
        "a note nobody defined reached the endnotes: {out}"
    );
}

#[test]
fn the_marker_line_inside_a_container_is_lazy_text_too() {
    // The paragraph the marker continues does not have to be the document's.
    // Inside a div `r` opens one and `- [^f]: t` folds into it, which is where
    // asking the block parser earns its keep: the run is probed as written, so
    // the container it sits in is part of the answer rather than a case the
    // guard has to know about. carve-php renders this identically.
    assert_eq!(
        html("::: note\nr\n- [^f]: t\n:::\n\n[^f] ref\n"),
        "<aside class=\"admonition note\">\n  <p>r\n- [^f]: t</p>\n</aside>\n<p>[^f] ref</p>"
    );
}

#[test]
fn a_marker_line_already_folded_into_the_paragraph_keeps_the_next_one_folded() {
    // `- a` after `r` is itself lazy text, so all three lines are ONE paragraph
    // and the definition on the third is text like the rest of it. A guard that
    // read only the previous LINE saw a list marker there, concluded no
    // paragraph was open and collected - the enumeration's answer, and wrong.
    // carve-php renders this identically.
    assert_eq!(
        html("r\n- a\n- [^f]: t\n\n[^f] ref\n"),
        "<p>r\n- a\n- [^f]: t</p>\n<p>[^f] ref</p>"
    );
}

// ---------------------------------------------------------------------------
// CONTROLS. Each of these already passed before the fix, and each one fails if
// the guard is written one notch too wide.
// ---------------------------------------------------------------------------

#[test]
fn control_a_quote_marker_still_collects() {
    // `>` interrupts the paragraph, so the quote is real and the definition is
    // collected from inside it - leaving the quote empty. A guard that keyed on
    // the stripped prefix rather than the RAW line breaks this row.
    assert_eq!(
        html("r\n> [^f]: t\n\n[^f] ref\n"),
        "<p>r</p>\n<blockquote>\n\n</blockquote>\n<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> ref</p>\n<section role=\"doc-endnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>t<a href=\"#fnref1\" role=\"doc-backlink\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn control_a_quoted_list_marker_still_collects() {
    // `> - [^f]: t` after a paragraph: the quote interrupts first, so the marker
    // inside it is not lazy text. `detect_list_marker_full` reads the RAW line,
    // which is what keeps this row on the collecting side.
    let out = html("r\n> - [^f]: t\n\n[^f] ref\n");
    assert!(out.contains("doc-endnotes"), "{out}");
}

#[test]
fn control_a_heading_leaves_no_paragraph_so_the_marker_is_a_real_item() {
    // The guard's reason for existing. A heading is not blank; if "not blank"
    // stood in for "leaves an open paragraph", this collection would be refused
    // and the definition would come back as literal item text.
    let out = html("# r\n- [^f]: t\n\n[^f] ref\n");
    assert!(out.contains("doc-endnotes"), "{out}");
    assert!(
        !out.contains("[^f]: t"),
        "the definition stayed literal: {out}"
    );
}

#[test]
fn control_a_blank_line_before_the_marker_still_collects() {
    // No open paragraph to be lazy about, so the item is a real one.
    let out = html("r\n\n- [^f]: t\n\n[^f] ref\n");
    assert!(out.contains("doc-endnotes"), "{out}");
}

#[test]
fn control_a_sibling_item_still_collects() {
    // `- a` is a list item, not a top-level paragraph, so the item below it is
    // a sibling rather than a lazy continuation.
    let out = html("- a\n- [^f]: t\n\n[^f] ref\n");
    assert!(out.contains("doc-endnotes"), "{out}");
}

#[test]
fn control_a_thematic_break_leaves_no_paragraph() {
    let out = html("***\n- [^f]: t\n\n[^f] ref\n");
    assert!(out.contains("doc-endnotes"), "{out}");
}

#[test]
fn control_an_unmarked_definition_after_a_paragraph_still_collects() {
    // No marker at all: the definition is at column 0 and every engine collects
    // it, leaving the paragraph with only `r`. The guard must not reach this.
    assert_eq!(html("r\n[^f]: t\n"), "<p>r</p>");
}

#[test]
fn control_a_link_reference_definition_is_unaffected() {
    // The ticket's own "it is not the definition kind" control. This path
    // already refused the collection, and must go on refusing it.
    assert_eq!(html("r\n. [d]: u\n"), "<p>r\n. [d]: u</p>");
}

// ---------------------------------------------------------------------------
// SHAPES THE ENUMERATION GOT WRONG. Every row here is a line that leaves NO
// paragraph open, followed by a marker that therefore opens a REAL item whose
// definition is collected - and every one of them was measured against a
// pre-fix binary as a collection the engine used to make. Four came out of
// closing the first draft's list, three more out of one review pass over it,
// which is the evidence that the list has no end. They are separate tests
// because the failure mode is per shape: one row reaching only one of them
// would report the rest as covered.
// ---------------------------------------------------------------------------

/// A COLON CONTAINER opens a container and leaves no paragraph, so the item
/// inside it is a real item.
#[test]
fn a_colon_container_opener_leaves_no_paragraph() {
    let out = html("::: note\n- [^f]: t\n:::\n\n[^f] ref\n");
    assert!(out.contains("doc-endnotes"), "{out}");
    assert!(
        !out.contains("[^f]: t"),
        "the definition stayed in the item: {out}"
    );
}

/// A HARDBREAKS BLOCK opener is the same class as the colon container.
#[test]
fn a_hardbreaks_block_opener_leaves_no_paragraph() {
    let out = html("::: \\\n- [^f]: t\n:::\n\n[^f] ref\n");
    assert!(out.contains("doc-endnotes"), "{out}");
}

/// A LINK-REFERENCE DEFINITION is a definition line, not paragraph text. It is
/// also the row that says the probe reproduces the PIPELINE rather than just
/// calling the parser: link definitions are stripped downstream of the footnote
/// pass, so a probe that skipped that stage would read `[a]: /u` as the
/// paragraph the block parser never sees.
#[test]
fn a_link_reference_definition_leaves_no_paragraph() {
    let out = html("[a]: /u\n- [^f]: t\n\n[^f] ref\n");
    assert!(out.contains("doc-endnotes"), "{out}");
    assert!(
        !out.contains("[^f]: t"),
        "the definition stayed in the item: {out}"
    );
}

/// A DEFINITION THIS PASS ALREADY COLLECTED is gone from the parser's input, so
/// the run the probe reads has to be the extracted body rather than the raw
/// source. The CONTINUATION line is what makes the two differ: `  more` belongs
/// to the note above it and is lifted out with it, but read raw it is an
/// indented line under a definition-shaped one - which is a paragraph, and a
/// paragraph suppresses the collection below it.
#[test]
fn a_definition_already_collected_leaves_no_paragraph() {
    let out = html("[^a]: n\n  more\n- [^f]: t\n\n[^f] ref\n");
    assert!(out.contains("doc-endnotes"), "{out}");
    assert!(
        !out.contains("[^f]: t"),
        "the definition stayed in the item: {out}"
    );
}

/// A SIBLING ITEM AFTER A LAZY CONTINUATION. `  lazy` IS ordinary paragraph
/// text, and the marker below it is still a sibling rather than a lazy
/// continuation, because the paragraph it left open belongs to the item it
/// leaves. The open frame says so by the ITEM COUNT: the list holds one item
/// without this line and two with it.
#[test]
fn a_sibling_item_after_a_lazy_continuation_still_collects() {
    let out = html("- a\n  lazy\n- [^f]: t\n\n[^f] ref\n");
    assert!(out.contains("doc-endnotes"), "{out}");
    assert!(
        !out.contains("[^f]: t"),
        "the definition stayed in the item: {out}"
    );
}

/// A NESTED LIST AT THE ITEM'S CONTENT COLUMN, where the two raw indents are
/// EQUAL - so an indent comparison cannot separate it from a lazy continuation
/// and the first draft suppressed it. The open frame separates them without
/// measuring anything: the marker adds a list inside the item, so the item's
/// block count goes from one to two.
#[test]
fn a_nested_list_at_the_items_content_column_still_collects() {
    let out = html("- a\n  lazy\n  - [^f]: t\n\n[^f] ref\n");
    assert!(out.contains("doc-endnotes"), "{out}");
    assert!(
        !out.contains("[^f]: t"),
        "the definition stayed in the item: {out}"
    );
}

/// AN ABBREVIATION DEFINITION is an opener the first draft's list did not name.
/// It is also why the probe runs at the DOCUMENT level: `*[HTML]: x` is only
/// recognised there, so a probe one level down reads it as a paragraph and
/// answers this row the wrong way.
#[test]
fn an_abbreviation_definition_leaves_no_paragraph() {
    let out = html("*[HTML]: Hyper Text\n- [^f]: t\n\n[^f] ref\n");
    assert!(out.contains("doc-endnotes"), "{out}");
    assert!(
        !out.contains("[^f]: t"),
        "the definition stayed in the item: {out}"
    );
}

/// THE CLOSING LINE OF A WRAPPED ATTRIBUTE BLOCK. `#y}` is the second line of a
/// `{.x` block, not paragraph text, and the attributes land on the list the
/// marker opens - which the rendered `<ul class="x" id="y">` shows.
#[test]
fn the_closing_line_of_a_wrapped_attribute_block_leaves_no_paragraph() {
    let out = html("{.x\n#y}\n- [^f]: t\n\n[^f] ref\n");
    assert!(out.contains("doc-endnotes"), "{out}");
    assert!(out.contains("<ul class=\"x\" id=\"y\">"), "{out}");
    assert!(
        !out.contains("[^f]: t"),
        "the definition stayed in the item: {out}"
    );
}

/// A BLOCK AN EXTENSION DEFINES is the shape no list could ever have carried,
/// because the pre-pass does not know the extension's syntax. The probe parses
/// with the caller's options, so the extension's own `match_block` answers -
/// and `@@@ x` leaves no paragraph open, so the item below it is real.
#[test]
fn a_block_only_an_extension_recognises_leaves_no_paragraph() {
    use carve::{BlockMatch, BlockNode, CarveExtension, MatcherContext, Options, Paragraph};

    struct Fence;
    impl CarveExtension for Fence {
        fn name(&self) -> &'static str {
            "fence"
        }

        fn match_block(
            &self,
            lines: &[&str],
            start: usize,
            ctx: &MatcherContext<'_>,
        ) -> Option<BlockMatch> {
            let content = (*lines.get(start)?).strip_prefix("@@@ ")?;
            Some(BlockMatch {
                node: BlockNode::Paragraph(Paragraph {
                    children: ctx.parse_inlines(content),
                    ..Default::default()
                }),
                lines_consumed: 1,
            })
        }
    }

    // The node the extension builds is a PARAGRAPH, which is the sharp version
    // of this row: the line looks like paragraph text to any enumeration and
    // the block it produces is a paragraph, and the marker below it still opens
    // a real item - because the extension CONSUMED the line, so there is no
    // open paragraph for the marker to continue. Only the parser knows that.
    let fence = Fence;
    let options = Options {
        extensions: vec![&fence],
        ..Default::default()
    };
    let out = carve::to_html_with_options("@@@ x\n- [^f]: t\n\n[^f] ref\n", &options)
        .trim()
        .to_string();
    assert!(out.contains("doc-endnotes"), "{out}");
    assert!(
        !out.contains("[^f]: t"),
        "the definition stayed in the item: {out}"
    );
}

/// THE KNOWN REMAINING SHAPE, pinned so it is a decision rather than a
/// surprise. Inside a quote the defect the ticket describes is still present:
/// the text is lost and a note nobody defined appears. The marker test reads
/// the RAW line, which is exactly what keeps
/// `control_a_quoted_list_marker_still_collects` above green, so nothing here
/// ever reaches the guard. `line_folds_into_an_open_paragraph` answers this one
/// correctly if it is ever asked - the quote is just another container in the
/// run it probes - so widening the scope is a small change, but it needs its
/// own controls and its own measurement rather than a ride on this one.
#[test]
fn known_remaining_the_same_shape_inside_a_quote_is_not_reached() {
    let out = html("> r\n> - [^f]: t\n\n[^f] ref\n");
    assert!(
        out.contains("doc-endnotes"),
        "the quote spelling started being guarded - update this row and the guard's docs together: {out}"
    );
}

/// THE FAIL-SAFE DIRECTION, asserted rather than claimed. The probe budget is
/// what a document can exhaust, and exhausting it must COLLECT - the answer the
/// engine gave before the guard existed - never suppress. A long blank-free run
/// of definition-shaped marker lines is the document that spends it, so the
/// last of them is collected while the first ones are text. Both halves are
/// asserted here: whichever way a future change moves the budget, this row says
/// that running out is not allowed to start deleting lines.
#[test]
fn running_out_of_probe_budget_collects_rather_than_suppresses() {
    let mut src = String::from("r\n");
    for n in 0..4000 {
        src.push_str(&format!("- [^f{n}]: t{n}\n"));
    }
    src.push_str("\n[^f3999] ref\n");
    let out = html(&src);
    assert!(
        out.contains("doc-endnotes"),
        "the budget ran out and suppressed instead of collecting: {}",
        &out[..out.len().min(400)]
    );
}
