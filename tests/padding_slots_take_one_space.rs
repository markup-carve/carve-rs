//! FOUR PADDING SLOTS TAKE EXACTLY ONE SPACE (carve#912).
//!
//! Four productions spell their padding slot as exactly ONE `space`, and this
//! engine accepted a RUN at every one of them. The ruling is that the
//! productions are right and the lax readers narrow.
//!
//! | production | slot |
//! | --- | --- |
//! | `link_title = space, ('"' ...)` | before the quoted title, INLINE and at a reference definition |
//! | `image_title = link_title` | before the quoted title on an image |
//! | `fenced_code_block = code_fence_open, [space], [code_fence_info]` | before the info string |
//! | `frontmatter_open = "---", [space], [frontmatter_format]` | before the format token |
//! | `reference_definition = ..., [space, attributes], newline` | before the trailing attribute block |
//!
//! Five sites, four productions: `link_title` is one production read at two
//! places, and every engine has had those two disagree before (carve#888).
//!
//! The failure mode is the one PART 7 already names, not a new one - the slot
//! does not match, so the construct does not form and every character survives
//! as text. Each site's CONTROL (the one-space form) is asserted beside it: the
//! narrowing must not break the form the language actually uses, and a patch
//! that makes the two-space documents pass while moving a control has
//! overshot.
//!
//! Cardinality is PER-PRODUCTION, not global. The two metadata slots inside
//! `code_fence_info` are spelled `space+` and stay a run, as does the colon
//! fence's separator (carve#900) and the definition markers' separator
//! (carve#892). Controls for all three are at the bottom of this file.

use carve::to_html;

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------
// link_title, inline
// ---------------------------------------------------------------------------

#[test]
fn an_inline_link_title_takes_exactly_one_space() {
    // The slot does not match, the quoted run is left unconsumed, and the
    // bracket run is not a link at all.
    assert_eq!(
        to_html("[t](/u  \"T\")\n").trim(),
        "<p>[t](/u  \u{201c}T\u{201d})</p>"
    );
}

#[test]
fn control_an_inline_link_title_with_one_space() {
    assert_eq!(
        to_html("[t](/u \"T\")\n").trim(),
        "<p><a href=\"/u\" title=\"T\">t</a></p>"
    );
}

#[test]
fn an_image_title_takes_exactly_one_space() {
    assert_eq!(
        to_html("![a](/p.png  \"T\")\n").trim(),
        "<p>![a](/p.png  \u{201c}T\u{201d})</p>"
    );
}

#[test]
fn control_an_image_title_with_one_space() {
    assert_eq!(
        to_html("![a](/p.png \"T\")\n").trim(),
        "<img src=\"/p.png\" alt=\"a\" title=\"T\">"
    );
}

// ---------------------------------------------------------------------------
// fenced_code_block
// ---------------------------------------------------------------------------

#[test]
fn a_code_fence_opener_takes_exactly_one_space() {
    // `language_info` cannot match a space, the opener matches no shape, and
    // the INVALID-FENCE FALLBACK applies: an inline verbatim span in a
    // paragraph.
    assert_eq!(
        to_html("```  php\nx = 1\n```\n").trim(),
        "<p><code>  php\nx = 1\n</code></p>"
    );
}

#[test]
fn control_a_code_fence_opener_with_one_space() {
    assert_eq!(
        to_html("``` php\nx = 1\n```\n").trim(),
        "<pre><code class=\"language-php\">x = 1\n</code></pre>"
    );
}

// ---------------------------------------------------------------------------
// frontmatter_open
// ---------------------------------------------------------------------------

#[test]
fn a_frontmatter_opener_takes_exactly_one_space() {
    // `frontmatter_format = (letter | digit)+` cannot match a space, so the
    // line is not a typed opener. It is not a thematic break either, so it is
    // ordinary paragraph text and the metadata lines fold into it.
    assert_eq!(
        to_html("---  yaml\ntitle: T\n---\n\nbody\n").trim(),
        "<p>\u{2014}  yaml\ntitle: T</p>\n<hr>\n<p>body</p>"
    );
}

#[test]
fn control_a_frontmatter_opener_with_one_space() {
    assert_eq!(
        to_html("--- yaml\ntitle: T\n---\n\nbody\n").trim(),
        "<p>body</p>"
    );
}

// ---------------------------------------------------------------------------
// reference_definition: the title slot and the trailing attribute block
// ---------------------------------------------------------------------------

#[test]
fn a_reference_definition_title_takes_exactly_one_space() {
    // The title is not a title. (carve#911 anchors this line at end of line on
    // top of that, which turns the whole line into a paragraph; the two
    // compose, and this asserts the half that is this ruling's - no title.)
    let html = to_html("[a]: /u  \"T\"\n\n[a][]\n");
    assert!(!html.contains("title="), "{html}");
}

#[test]
fn a_reference_definition_attribute_block_takes_exactly_one_space() {
    let html = to_html("[a]: /u  {.c}\n\n[a][]\n");
    assert!(!html.contains("class="), "{html}");
}

#[test]
fn control_a_reference_definition_with_one_space_at_both_slots() {
    assert_eq!(
        to_html("[a]: /u \"T\"\n\n[a][]\n").trim(),
        "<p><a href=\"/u\" title=\"T\">a</a></p>"
    );
    assert_eq!(
        to_html("[a]: /u {.c}\n\n[a][]\n").trim(),
        "<p><a href=\"/u\" class=\"c\">a</a></p>"
    );
}

// ---------------------------------------------------------------------------
// CONTROLS: what a padding slot is NOT
// ---------------------------------------------------------------------------

#[test]
fn control_a_run_with_nothing_after_it_is_the_line_ending_not_this_slot() {
    // Nothing is being padded, so these are not the slot and do not narrow.
    assert_eq!(to_html("[t](/u  )\n").trim(), "<p><a href=\"/u\">t</a></p>");
    assert_eq!(
        to_html("```  \nx\n```\n").trim(),
        "<pre><code>x\n</code></pre>"
    );
    assert_eq!(
        to_html("---  \ntitle: T\n---\n\nbody\n").trim(),
        "<p>body</p>"
    );
}

#[test]
fn control_the_code_fence_metadata_slots_are_still_runs() {
    // `code_fence_info`'s two internal slots are spelled `space+` and are NOT
    // in scope. A patch that narrows "every whitespace run on a fence opener"
    // breaks these.
    assert_eq!(
        to_html("``` php  \"T\"\nx\n```\n").trim(),
        "<pre title=\"T\"><code class=\"language-php\">x\n</code></pre>"
    );
    assert_eq!(
        squash(&to_html("``` php  \"T\"  [lbl]\nx\n```\n")),
        squash(&to_html("``` php \"T\" [lbl]\nx\n```\n"))
    );
}

#[test]
fn control_a_marker_separator_is_still_a_run() {
    // carve#892 ruled the OPPOSITE way for the definition markers, and
    // carve#900 for the colon fence. Read the production, not the role.
    let html = to_html("*[HTML]:   Hyper Text\n\nHTML\n");
    assert!(html.contains("title=\"Hyper Text\""), "{html}");
    let html = to_html("x[^f]\n\n[^f]:   note\n");
    assert!(html.contains("<p>note<a href=\"#fnref1\""), "{html}");
    assert_eq!(
        squash(&to_html(":::   note\nbody\n:::\n")),
        squash(&to_html("::: note\nbody\n:::\n"))
    );
}
