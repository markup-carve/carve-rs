//! What follows a heading (§10). A heading ends at its newline (PART 2
//! SINGLE-LINE HEADINGS, carve#451), so every following line simply begins
//! whatever block it begins: a list marker starts a sibling list, a quote
//! starts a quote, and plain text starts a paragraph. Nothing folds in, which
//! is the point -- the fold used to swallow the paragraph and the heading id
//! with it. Matches carve-js and carve-php; diverges from canonical djot.

#[test]
fn list_marker_ends_heading_and_starts_sibling_list() {
    // A bullet ends the heading and starts a sibling list.
    assert_eq!(
        carve::to_html("# H\n- item"),
        "<section id=\"H\">\n  <h1>H</h1>\n  <ul>\n    <li>item</li>\n  </ul>\n</section>"
    );
    // An ordered marker behaves identically (symmetric): it ends the heading.
    assert_eq!(
        carve::to_html("# H\n1. one"),
        "<section id=\"H\">\n  <h1>H</h1>\n  <ol>\n    <li>one</li>\n  </ol>\n</section>"
    );
    // A blockquote also ends the heading.
    assert_eq!(
        carve::to_html("# H\n> q"),
        "<section id=\"H\">\n  <h1>H</h1>\n  <blockquote><p>q</p></blockquote>\n</section>"
    );
}

#[test]
fn plain_text_after_a_heading_is_a_paragraph() {
    assert_eq!(
        carve::to_html("# H\nplain words"),
        "<section id=\"H\">\n  <h1>H</h1>\n  <p>plain words</p>\n</section>"
    );
}

#[test]
fn no_hash_marker_line_continues_a_heading() {
    // A same-count `#` line was Djot's explicit continuation form. It is now
    // simply the next heading, at that level.
    assert_eq!(
        carve::to_html("## H\n## more"),
        "<section id=\"H\">\n  <h2>H</h2>\n</section>\n<section id=\"more\">\n  <h2>more</h2>\n</section>"
    );
    assert_eq!(
        carve::to_html("# H\n# more"),
        "<section id=\"H\">\n  <h1>H</h1>\n</section>\n<section id=\"more\">\n  <h1>more</h1>\n</section>"
    );
    // A no-`#` plain-text line is a paragraph in the section.
    assert_eq!(
        carve::to_html("## H\nmore"),
        "<section id=\"H\">\n  <h2>H</h2>\n  <p>more</p>\n</section>"
    );

    // A DIFFERENT `#` count (fewer) ends the heading and starts a NEW one. A
    // shallower heading closes the section rather than nesting.
    assert_eq!(
        carve::to_html("## H\n# more"),
        "<section id=\"H\">\n  <h2>H</h2>\n</section>\n<section id=\"more\">\n  <h1>more</h1>\n</section>"
    );
    assert_eq!(
        carve::to_html("### H\n# more"),
        "<section id=\"H\">\n  <h3>H</h3>\n</section>\n<section id=\"more\">\n  <h1>more</h1>\n</section>"
    );
    // A DIFFERENT `#` count (more) likewise starts a new heading; a deeper
    // heading nests inside the current section.
    assert_eq!(
        carve::to_html("## H\n### more"),
        "<section id=\"H\">\n  <h2>H</h2>\n  <section id=\"more\">\n    <h3>more</h3>\n  </section>\n</section>"
    );
}
