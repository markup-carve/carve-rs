//! A block-opener interrupts a multi-line heading and starts that block,
//! exactly as it interrupts a paragraph (§10). Only plain text folds into the
//! heading; an ordered marker folds too (it never interrupts). Matches carve-js,
//! carve-php, and canonical djot.

#[test]
fn block_opener_interrupts_heading() {
    assert_eq!(
        carve::to_html("# H\n- item"),
        "<section id=\"h\">\n  <h1>H</h1>\n  <ul>\n    <li>item</li>\n  </ul>\n</section>"
    );
    assert_eq!(
        carve::to_html("# H\n> q"),
        "<section id=\"h\">\n  <h1>H</h1>\n  <blockquote><p>q</p></blockquote>\n</section>"
    );
}

#[test]
fn plain_text_still_folds_into_heading() {
    assert_eq!(
        carve::to_html("# H\nplain words"),
        "<section id=\"h-plain-words\">\n  <h1>H\nplain words</h1>\n</section>"
    );
}

#[test]
fn ordered_marker_folds_it_never_interrupts() {
    assert_eq!(
        carve::to_html("# H\n1. one"),
        "<section id=\"h-1-one\">\n  <h1>H\n1. one</h1>\n</section>"
    );
}
