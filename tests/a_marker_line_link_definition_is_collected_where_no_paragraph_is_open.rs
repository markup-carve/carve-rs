//! A LINK-REFERENCE definition behind a list marker is collected wherever the
//! line above it leaves no open paragraph (markup-carve/carve#1425).
//!
//! NO OPEN PARAGRAPH, NO LAZY LINE (grammar PART 0) is the rule, and the
//! link-definition pre-pass used to ask a cheaper question instead: is the
//! previous line blank? A heading, a comment, a definition of any of the three
//! kinds, a table row, a definition-list term, an attribute line and a marker
//! line are all non-blank and every one of them leaves NO paragraph open, so the
//! cheap spelling refused a collection carve-js, carve-php and the executable
//! spec all make. The definition came back as visible item text and the
//! reference that pointed at it resolved to nothing.
//!
//! THE ANSWER IS ASKED OF THE BLOCK PARSER, not enumerated - the same
//! `line_folds_into_an_open_paragraph` probe the footnote pass has used since
//! carve-rs#1024, whose own docs explain why a list of openers has no end. That
//! ticket fixed the footnote kind and filed this one; this is that fix, so the
//! two definition kinds now answer alike.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

/// The ticket's document. Both halves of the defect show at once: the item held
/// the definition's text, and a reference to the label resolved to nothing
/// because the pre-pass never collected it.
#[test]
fn the_tickets_document() {
    assert_eq!(html("[f]: t\n* [d]: u\n"), "<ul>\n  <li></li>\n</ul>");
    assert_eq!(
        html("[f]: t\n* [d]: u\n\n[go][d]\n"),
        "<ul>\n  <li></li>\n</ul>\n<p><a href=\"u\">go</a></p>"
    );
}

/// The document-level definition above is not the only spelling that triggered
/// it - all three definition kinds leave no paragraph open, and the footnote and
/// abbreviation lines are already gone from the parser's input by the time this
/// pass runs, which is why "not blank" saw a non-blank placeholder there.
#[test]
fn every_definition_kind_above_leaves_no_paragraph() {
    for above in ["[f]: t", "[^f]: t", "*[AB]: t"] {
        let out = html(&format!("{above}\n* [d]: u\n\n[go][d]\n"));
        assert!(
            out.contains("<a href=\"u\">go</a>"),
            "the definition was not collected under {above:?}: {out}"
        );
        assert!(
            !out.contains("[d]: u"),
            "the definition stayed as item text under {above:?}: {out}"
        );
    }
}

/// The marker dialect changes numbering only. An ordered marker does not
/// interrupt a paragraph either, so it took the same wrong answer.
#[test]
fn every_list_spelling_answers_the_same_way() {
    for marker in ["* ", "- ", "1. ", ". "] {
        let out = html(&format!("[f]: t\n{marker}[d]: u\n\n[go][d]\n"));
        assert!(
            out.contains("<a href=\"u\">go</a>"),
            "the definition was not collected under {marker:?}: {out}"
        );
    }
}

// ---------------------------------------------------------------------------
// SHAPES THE CHEAP SPELLING GOT WRONG. Each line above the marker leaves no
// open paragraph, so the marker opens a REAL item and the definition in it is
// collected. One test per shape: the failure mode is per shape, so a single row
// reaching one of them would report the rest as covered.
// ---------------------------------------------------------------------------

fn assert_collected(src: &str) {
    let out = html(src);
    assert!(
        out.contains("<a href=\"u\">go</a>"),
        "the reference did not resolve: {out}"
    );
    assert!(
        !out.contains("[d]: u"),
        "the definition stayed as item text: {out}"
    );
}

#[test]
fn a_heading_above_leaves_no_paragraph() {
    assert_collected("# h\n* [d]: u\n\n[go][d]\n");
}

#[test]
fn a_comment_above_leaves_no_paragraph() {
    assert_collected("%% c\n* [d]: u\n\n[go][d]\n");
}

#[test]
fn a_thematic_break_above_leaves_no_paragraph() {
    assert_collected("---\n* [d]: u\n\n[go][d]\n");
}

#[test]
fn a_table_row_above_leaves_no_paragraph() {
    assert_collected("| a |\n* [d]: u\n\n[go][d]\n");
}

#[test]
fn a_definition_list_term_above_leaves_no_paragraph() {
    assert_collected(":: t\n* [d]: u\n\n[go][d]\n");
}

#[test]
fn an_attribute_line_above_leaves_no_paragraph() {
    // The attributes land on the list the marker opens, which the rendered
    // `<ul class="k">` shows - so the line is a floater and not paragraph text.
    let out = html("{.k}\n* [d]: u\n\n[go][d]\n");
    assert!(out.contains("<ul class=\"k\">"), "{out}");
    assert!(out.contains("<a href=\"u\">go</a>"), "{out}");
}

#[test]
fn a_colon_container_opener_above_leaves_no_paragraph() {
    assert_collected("::: d\n* [d]: u\n:::\n\n[go][d]\n");
}

#[test]
fn a_sibling_item_above_leaves_no_paragraph() {
    // `- a` is a list item, not a top-level paragraph, so the marker below it
    // opens a sibling rather than continuing anything.
    assert_collected("- a\n- [d]: u\n\n[go][d]\n");
}

#[test]
fn item_prose_above_a_column_zero_marker_leaves_no_paragraph_it_can_continue() {
    // The paragraph `  more` leaves open belongs to the ITEM, and a column-0
    // marker cannot continue it - the item closes and a new list opens. The
    // probe answers this by the frame rather than by comparing indents.
    assert_collected("- a\n  more\n* [d]: u\n\n[go][d]\n");
}

#[test]
fn a_lazy_continuation_above_a_column_zero_marker_answers_the_same_way() {
    // Same shape with the item's prose written flush left as a lazy line.
    assert_collected("- a\nlazy\n* [d]: u\n\n[go][d]\n");
}

// ---------------------------------------------------------------------------
// CONTROLS. Every row here passed before the fix and fails if the guard is
// written one notch too wide - a paragraph really is open, so the marker line
// is lazy text and the definition in it defines nothing.
// ---------------------------------------------------------------------------

#[test]
fn control_a_top_level_paragraph_keeps_the_marker_line_lazy() {
    // §10: a list does not interrupt an open paragraph, so both lines are one
    // paragraph and `[d]: u` is part of its text.
    assert_eq!(html("para\n* [d]: u\n"), "<p>para\n* [d]: u</p>");
    assert_eq!(html("r\n. [d]: u\n"), "<p>r\n. [d]: u</p>");
}

#[test]
fn control_an_indented_marker_after_a_paragraph_is_lazy_too() {
    assert_eq!(html("para\n  * [d]: u\n"), "<p>para\n* [d]: u</p>");
}

#[test]
fn control_a_quoted_paragraph_keeps_the_marker_line_lazy() {
    // The paragraph the marker folds into does not have to be the document's.
    assert_eq!(
        html("> q\n* [d]: u\n"),
        "<blockquote><p>q\n* [d]: u</p></blockquote>"
    );
}

#[test]
fn control_the_reference_does_not_resolve_from_a_lazy_line() {
    // The other half of the control: text on the page AND no definition, so a
    // reference to the label stays unresolved.
    let out = html("para\n* [d]: u\n\n[go][d]\n");
    assert!(out.contains("[d]: u"), "{out}");
    assert!(!out.contains("<a href=\"u\">"), "{out}");
}

#[test]
fn control_a_code_fence_keeps_its_content() {
    // An unterminated fence still opens (§ "an opener always opens"), so the
    // marker line is code text. Nothing may cut a definition out of it.
    assert_eq!(
        html("```\n* [d]: u\n"),
        "<pre><code>* [d]: u\n</code></pre>"
    );
}

#[test]
fn control_a_quoted_marker_still_collects() {
    // The marker test reads the RAW line, so `> - [d]: u` never reaches the
    // guard - the same scope the footnote pass has. Documented rather than
    // changed here.
    let out = html("para\n> - [d]: u\n\n[go][d]\n");
    assert!(out.contains("<a href=\"u\">go</a>"), "{out}");
}

#[test]
fn control_an_unmarked_definition_after_a_paragraph_still_collects() {
    // No marker at all: the definition is at column 0, it interrupts the
    // paragraph, and every engine collects it. The guard must not reach it.
    assert_eq!(
        html("para\n[d]: u\n\n[go][d]\n"),
        "<p>para</p>\n<p><a href=\"u\">go</a></p>"
    );
}

/// THE FAIL-SAFE DIRECTION, asserted rather than claimed. The probe budget is
/// what a document can exhaust, and exhausting it must COLLECT - the answer the
/// pass gave before the guard existed - never suppress an author's line. A long
/// blank-free run of definition-shaped marker lines is the document that spends
/// it.
#[test]
fn running_out_of_probe_budget_collects_rather_than_suppresses() {
    let mut src = String::from("r\n");
    for n in 0..4000 {
        src.push_str(&format!("- [d{n}]: u{n}\n"));
    }
    src.push_str("\n[go][d3999]\n");
    let out = html(&src);
    assert!(
        out.contains("<a href=\"u3999\">go</a>"),
        "the budget ran out and suppressed instead of collecting: {}",
        &out[..out.len().min(400)]
    );
}
