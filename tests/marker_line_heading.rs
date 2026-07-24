//! A heading WITH content that opens on a list item's MARKER LINE (`- # H`)
//! parses as a heading block, exactly like a blockquote, fenced code, `:::`
//! container, thematic break, or table opening on the marker line. Previously
//! carve-rs was the sole implementation that left the heading as inline text
//! (`<li># H</li>`); carve-js and carve-php already emitted the heading. Bare or
//! whitespace-only remainders, and a tab separator, stay inline text.

#[test]
fn heading_with_content_opens_on_marker_line() {
    assert_eq!(
        carve::to_html("- # H\n"),
        "<ul>\n  <li>\n    <h1 id=\"H\">H</h1>\n  </li>\n</ul>"
    );
}

#[test]
fn deeper_heading_level_opens_on_marker_line() {
    assert_eq!(
        carve::to_html("- ###### x\n"),
        "<ul>\n  <li>\n    <h6 id=\"x\">x</h6>\n  </li>\n</ul>"
    );
}

#[test]
fn ordered_marker_line_heading_opens() {
    assert_eq!(
        carve::to_html("1. # H\n"),
        "<ol>\n  <li>\n    <h1 id=\"H\">H</h1>\n  </li>\n</ol>"
    );
}

#[test]
fn nested_item_marker_line_heading_opens() {
    assert_eq!(
        carve::to_html("- a\n  - # Nested\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>\n        <h1 id=\"Nested\">Nested</h1>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn bare_hash_on_marker_line_stays_text() {
    // No content after the marker: not a heading, stays inline text (tight).
    assert_eq!(carve::to_html("- #\n"), "<ul>\n  <li>#</li>\n</ul>");
}

#[test]
fn hash_space_no_content_on_marker_line_stays_text() {
    assert_eq!(carve::to_html("- # \n"), "<ul>\n  <li>#</li>\n</ul>");
}

#[test]
fn tab_separated_hash_on_marker_line_stays_text() {
    // A tab is not the required heading space, so `#\tH` is not a heading.
    assert_eq!(carve::to_html("- #\tH\n"), "<ul>\n  <li>#\tH</li>\n</ul>");
}

#[test]
fn flush_left_lazy_text_folds_into_marker_line_heading() {
    // A heading folds trailing flush-left plain text as continuation, so the
    // lazy line stays INSIDE the item and inside the heading (matches carve-js
    // / carve-php), rather than floating out to a top-level paragraph.
    assert_eq!(
        carve::to_html("- # H\nlazy\n"),
        "<ul>\n  <li>\n    <h1 id=\"H-lazy\">H\nlazy</h1>\n  </li>\n</ul>"
    );
}

#[test]
fn blank_line_closes_marker_line_heading_and_ends_item() {
    // A blank line ends the heading (§heading rule 2); the following text is a
    // separate top-level block, not folded into the heading.
    assert_eq!(
        carve::to_html("- # H\n\nsep\n"),
        "<ul>\n  <li>\n    <h1 id=\"H\">H</h1>\n  </li>\n</ul>\n<p>sep</p>"
    );
}

#[test]
fn blank_before_sibling_loosens_after_marker_line_heading() {
    // The blank separating a marker-line-heading item from its sibling loosens
    // the list, so the sibling's text is wrapped in <p> (matches carve-js /
    // carve-php). The single-line heading leaves no indented continuation, so
    // the branch must re-raise the swallowed blank separator.
    assert_eq!(
        carve::to_html("- # H\n\n- b\n"),
        "<ul>\n  <li>\n    <h1 id=\"H\">H</h1>\n  </li>\n  <li><p>b</p></li>\n</ul>"
    );
}

#[test]
fn blank_before_sibling_loosens_after_marker_line_thematic_break() {
    // Same looseness rule for the other single-line marker-line block (thematic
    // break). This path was already tight-buggy before marker-line headings
    // existed; the fix corrects both.
    assert_eq!(
        carve::to_html("- ---\n\n- b\n"),
        "<ul>\n  <li>\n    <hr>\n  </li>\n  <li><p>b</p></li>\n</ul>"
    );
}
