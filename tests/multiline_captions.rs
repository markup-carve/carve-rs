//! Multi-line captions follow the PARAGRAPH continuation model (not the heading
//! model): following plain lines fold in, a list marker folds in (a list needs
//! a blank line to interrupt), and a heading / blockquote / table / fence / div
//! / thematic break / `%%%` comment or a further `^ ` line ends the caption.

fn h(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

#[test]
fn folds_a_following_plain_line() {
    assert_eq!(
        h("![a](/u)\n^ cap\nmore"),
        "<figure>\n  <img src=\"/u\" alt=\"a\">\n  <figcaption>cap\nmore</figcaption>\n</figure>"
    );
}

#[test]
fn blank_line_ends_caption() {
    assert_eq!(
        h("![a](/u)\n^ cap\n\nmore"),
        "<figure>\n  <img src=\"/u\" alt=\"a\">\n  <figcaption>cap</figcaption>\n</figure>\n<p>more</p>"
    );
}

#[test]
fn list_marker_folds_in() {
    assert_eq!(
        h("![a](/u)\n^ cap\n- x"),
        "<figure>\n  <img src=\"/u\" alt=\"a\">\n  <figcaption>cap\n- x</figcaption>\n</figure>"
    );
}

#[test]
fn heading_ends_caption() {
    assert_eq!(
        h("![a](/u)\n^ cap\n# H"),
        "<figure>\n  <img src=\"/u\" alt=\"a\">\n  <figcaption>cap</figcaption>\n</figure>\n<section id=\"H\">\n  <h1>H</h1>\n</section>"
    );
}

#[test]
fn further_caret_line_ends_caption() {
    assert_eq!(
        h("![a](/u)\n^ cap\n^ two"),
        "<figure>\n  <img src=\"/u\" alt=\"a\">\n  <figcaption>cap</figcaption>\n</figure>\n<p>^ two</p>"
    );
}

#[test]
fn code_listing_multiline_caption() {
    assert_eq!(
        h("```\nx\n```\n^ cap\nmore"),
        "<figure>\n  <pre><code>x\n</code></pre>\n  <figcaption>cap\nmore</figcaption>\n</figure>"
    );
}

#[test]
fn reference_image_multiline_caption() {
    assert_eq!(
        h("![a][r]\n^ cap\nmore\n\n[r]: /u"),
        "<figure>\n  <img src=\"/u\" alt=\"a\">\n  <figcaption>cap\nmore</figcaption>\n</figure>"
    );
}
