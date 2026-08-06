//! A list item's paragraph that renders to NOTHING contributes no line
//! (carve-rs#670, corpus 226).
//!
//! `render_list_item` already dropped a `Part::Block` whose HTML was empty
//! (#429, carve-rs#532) and exempted every `Part::Inline`. A paragraph is an
//! Inline part, so one that rendered to nothing survived the filter and
//! published a stray blank line inside the `<li>`:
//!
//! ```text
//! <li>a
//!
//!   </li>
//! ```
//!
//! Two shapes reach it, and both are "content that renders nothing":
//!
//! - a `+`-attached block whose whole content was a COLLECTED DEFINITION. The
//!   prepass blanks that line, so the attached block parses to an empty
//!   paragraph. Leaving the blank behind is exactly the trace spec
//!   markup-carve/carve#801 says a collected definition must not leave.
//! - a `+`-attached COMMENT, which renders nothing for the ordinary reason.
//!
//! carve-js and carve-php publish `<li>a</li>` for both.
//!
//! The filter tests `is_empty`, NOT `trim().is_empty()`: Rust's `trim` takes
//! Unicode whitespace, so a no-break space would be dropped as blank - and an
//! item holding one is an item with content.

fn first_list(html: &str) -> String {
    let start = html.find("<ul>").expect("a list");
    let end = html.find("</ul>").expect("its end");
    html[start..end + 5].to_string()
}

#[test]
fn a_definition_attached_by_a_continuation_marker_leaves_no_line() {
    let html = carve::to_html("- a\n+\n[r]: /u\n\nsee [t][r]\n");
    assert_eq!(first_list(&html), "<ul>\n  <li>a</li>\n</ul>");
    // The definition is still COLLECTED - the item keeping no trace of it is
    // the other half of the same rule.
    assert!(
        html.contains("<a href=\"/u\">t</a>"),
        "the reference stopped resolving:\n{html}"
    );
}

#[test]
fn a_comment_attached_by_a_continuation_marker_leaves_no_line() {
    let html = carve::to_html("- a\n+\n%% c\n\nx\n");
    assert_eq!(first_list(&html), "<ul>\n  <li>a</li>\n</ul>");
}

#[test]
fn a_no_break_space_is_content_and_survives() {
    // The reason the filter cannot use `trim`. A NBSP-only item renders it.
    let html = carve::to_html("- \u{a0}\n\nx\n");
    assert!(
        html.contains("&nbsp;") || html.contains('\u{a0}'),
        "an item holding a no-break space was dropped as blank:\n{html}"
    );
}

#[test]
fn visible_attached_content_is_unchanged() {
    // The common case: a `+`-attached paragraph still attaches.
    let html = carve::to_html("- a\n+\nb\n\nx\n");
    assert_eq!(first_list(&html), "<ul>\n  <li>a\n    b\n  </li>\n</ul>");
}

#[test]
fn an_item_whose_only_content_renders_to_nothing_is_empty_not_blank() {
    // Every part filtered out leaves `<li></li>` rather than an item holding a
    // blank line.
    let html = carve::to_html("- %% c\n\nx\n");
    assert!(
        !html.contains("<li>\n"),
        "an item of nothing published a blank line:\n{html}"
    );
}

/// The empty paragraph must not be in the TREE either, not merely filtered out
/// of the HTML (carve-rs#670 was fixed in the renderer; the node stayed).
///
/// Spec markup-carve/carve#801 says the item keeps no trace of a collected
/// definition, and a block node the author never wrote is a trace - a consumer
/// reading the AST sees an empty paragraph in the item. The three-way shape
/// comparison in the spec repo showed this engine standing alone on corpus 226
/// for exactly that reason while its HTML already matched.
#[test]
fn the_item_carries_no_empty_paragraph_in_the_tree() {
    let json = carve::to_json(&carve::parse("- a\n+\n[r]: /u\n\nsee [t][r]\n"));
    // The item holds ONE block - its own paragraph - and the definition leaves
    // nothing behind it.
    let item_blocks = json.matches("\"type\":\"paragraph\"").count();
    assert!(
        json.contains("\"type\":\"list_item\""),
        "expected a list item:\n{json}"
    );
    // Two paragraphs in the whole document: the item's `a`, and the `see …`
    // line. A third would be the empty one.
    assert_eq!(
        item_blocks, 2,
        "expected exactly two paragraphs in the document, got {item_blocks}:\n{json}"
    );
}
