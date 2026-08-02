//! What follows a heading (§10). A heading is SINGLE-LINE (PART 2): it ends at
//! the newline, so the next line simply begins whatever block it begins -- a
//! list marker opens a sibling list, and plain text opens a paragraph. Matches
//! carve-js and carve-php.

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
fn a_same_level_hash_marker_starts_a_second_heading() {
    // Djot merges a same-count `#` line into the open heading ("may be preceded
    // by the same number of `#` characters"). Carve does not: each heading line
    // is its own heading, and its id comes from that line alone.
    assert_eq!(
        carve::to_html("## H\n## more"),
        "<section id=\"H\">\n  <h2>H</h2>\n</section>\n<section id=\"more\">\n  <h2>more</h2>\n</section>"
    );
    assert_eq!(
        carve::to_html("# H\n# more"),
        "<section id=\"H\">\n  <h1>H</h1>\n</section>\n<section id=\"more\">\n  <h1>more</h1>\n</section>"
    );
    // A no-`#` plain-text line is a paragraph inside the section.
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

#[test]
fn the_auto_id_comes_from_the_heading_line_alone() {
    // The folding rule derived the id from the heading text PLUS every folded
    // line, so `[Heading][]` references and TOC anchors keyed on text the author
    // never put in the title, with nothing reporting it.
    assert_eq!(
        carve::to_html("# Title\nSome text.\n"),
        "<section id=\"Title\">\n  <h1>Title</h1>\n  <p>Some text.</p>\n</section>"
    );
}

#[test]
fn a_caption_line_after_a_heading_is_literal_text() {
    assert_eq!(
        carve::to_html("# H\n^ cap\n"),
        "<section id=\"H\">\n  <h1>H</h1>\n  <p>^ cap</p>\n</section>"
    );
}
