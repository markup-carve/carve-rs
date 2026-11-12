//! A quoted definition leaves its paragraph open, so an unmarked line below it
//! is lazy continuation and stays in the quote, as in cmark-gfm 0.29.0.gfm.13
//! (carve#2187). carve-js#5001 moved to this reading.

#[test]
fn a_lazy_line_stays_in_the_quote() {
    assert_eq!(
        carve::markdown_to_carve("> [a]: /u\nlazy\n"),
        "> lazy\n\n[a]: /u\n"
    );
    assert_eq!(
        carve::markdown_to_carve("> [a]: /u \"t\"\n   lazy\n\n[y][a]\n"),
        "> lazy\n\n[y][a]\n\n[a]: /u \"t\"\n"
    );
    assert_eq!(
        carve::markdown_to_carve("> [a]: /u\n> [b]: /v\nlazy\n>\n> next\n"),
        "> lazy\n>\n> next\n\n[a]: /u\n\n[b]: /v\n"
    );
    assert_eq!(
        carve::markdown_to_carve("> > [a]: /u\nlazy\n"),
        "> > lazy\n\n[a]: /u\n"
    );
}

#[test]
fn a_lazy_line_stays_in_a_quoted_item() {
    assert_eq!(
        carve::markdown_to_carve("> - [a]: /u\n  lazy\n"),
        "> - lazy\n\n[a]: /u\n"
    );
    assert_eq!(
        carve::markdown_to_carve("- > [a]: /u\nlazy\n"),
        "- > lazy\n\n[a]: /u\n"
    );
}

#[test]
fn a_block_start_ends_the_quote() {
    assert_eq!(
        carve::markdown_to_carve("> [a]: /u\n# h\n"),
        ">\n\n# h\n\n[a]: /u\n"
    );
    assert_eq!(
        carve::markdown_to_carve("> [a]: /u\n***\n"),
        ">\n\n***\n\n[a]: /u\n"
    );
}

#[test]
fn indented_html_stays_lazy_text() {
    assert_eq!(
        carve::to_html(&carve::markdown_to_carve("> [a]: /u\n    <div>x</div>\n")),
        "<blockquote><p><div>x</div></p></blockquote>"
    );
}
