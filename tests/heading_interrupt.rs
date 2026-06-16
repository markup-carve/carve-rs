//! Heading continuation (§10). A block-opener that interrupts a paragraph also
//! ends a multi-line heading and starts that block. Symmetric list
//! interruption: a LIST marker (bullet OR ordered) ENDS the heading and starts
//! a sibling list -- it does NOT fold in (a list marker folds only into a
//! PARAGRAPH). Plain text folds into the heading. Matches carve-js, carve-php,
//! and canonical djot.

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
fn plain_text_still_folds_into_heading() {
    assert_eq!(
        carve::to_html("# H\nplain words"),
        "<section id=\"H-plain-words\">\n  <h1>H\nplain words</h1>\n</section>"
    );
}
