//! PART 0 CARVE-P0-011: a block at an item's content column ends the paragraph
//! a marker-line `+` pulled to column 0, as it does after an own-line `+`
//! (markup-carve/carve#5349, ruling on markup-carve/carve#5331).

use carve::to_html;

#[test]
fn a_heading_at_the_content_column_opens_in_the_item() {
    assert_eq!(
        to_html("- +\nh\n  # H\n"),
        "<ul>\n  <li>h\n    <h1 id=\"H\">H</h1>\n  </li>\n</ul>"
    );
}

#[test]
fn a_column_zero_line_after_it_leaves_the_list() {
    let html = to_html("- +\nh\n  # H\nz\n");
    assert!(html.ends_with("</ul>\n<p>z</p>"), "{html}");
}

#[test]
fn every_host_ends_the_pulled_paragraph() {
    for (src, block) in [
        ("1. +\nh\n   # H\n", "<h1 id=\"H\">H</h1>"),
        ("- +\nh\n  > q\n", "<blockquote><p>q</p></blockquote>"),
        ("- +\nh\n  | a |\n", "<table>"),
        ("- a\n  - +\n  h\n    # H\n", "<h1 id=\"H\">H</h1>"),
    ] {
        let html = to_html(src);
        assert!(html.contains(block), "{src:?}: {html}");
    }
}

#[test]
fn text_at_the_content_column_still_folds() {
    assert_eq!(to_html("- +\nh\n  k\n"), "<ul>\n  <li>h\nk</li>\n</ul>");
}

#[test]
fn a_colon_fence_at_the_content_column_opens_in_either_spelling() {
    for src in [
        "- +\nh\n  ::: note\n  x\n  :::\n",
        "- a\n+\nh\n  ::: note\n  x\n  :::\n",
    ] {
        let html = to_html(src);
        assert!(
            html.contains("<aside class=\"admonition note\""),
            "{src:?}: {html}"
        );
        assert!(!html.contains("::: note"), "{src:?}: {html}");
    }
}
