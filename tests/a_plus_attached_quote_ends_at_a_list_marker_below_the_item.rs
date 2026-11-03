//! A list marker below an item's content column ends a `+`-attached quote and
//! the item, as it does for the indented quote (PART 0 S2, S3; carve#5057).

fn html(source: &str) -> String {
    carve::to_html(source)
        .lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("")
}

#[test]
fn the_marker_opens_a_list_after_the_item() {
    assert_eq!(
        html("1. a\n+\n> q\n  - y\n"),
        "<ol><li>a<blockquote><p>q</p></blockquote></li></ol><ul><li>y</li></ul>"
    );
    assert_eq!(
        html("- a\n+\n> q\n - y\n"),
        "<ul><li>a<blockquote><p>q</p></blockquote></li></ul><ul><li>y</li></ul>"
    );
    assert_eq!(
        html("1. a\n+\n> > q\n  1. y\n"),
        "<ol><li>a<blockquote><blockquote><p>q</p></blockquote></blockquote></li></ol><ol><li>y</li></ol>"
    );
}

#[test]
fn plain_text_still_continues_the_quote_lazily() {
    assert_eq!(
        html("1. a\n+\n> q\n  y\n"),
        "<ol><li>a<blockquote><p>qy</p></blockquote></li></ol>"
    );
}

#[test]
fn a_marker_at_the_content_column_nests_in_the_item() {
    assert_eq!(
        html("1. a\n+\n> q\n   - y\n"),
        html("1. a\n   > q\n   - y\n")
    );
}
