//! CARVE-P9-080: a bare `>` is the quote's blank line, so a flush-left line
//! after it has no paragraph to continue and ends the host (carve-rs#3668).

#[test]
fn a_description_hosting_the_quote_ends() {
    assert_eq!(
        carve::to_html(":: t\n: > c\n  >\nb\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <blockquote><p>c</p></blockquote>\n  </dd>\n</dl>\n<p>b</p>"
    );
}

#[test]
fn an_item_hosting_the_quote_ends() {
    assert_eq!(
        carve::to_html("- > c\n  >\nb\n"),
        "<ul>\n  <li>\n    <blockquote><p>c</p></blockquote>\n  </li>\n</ul>\n<p>b</p>"
    );
}

#[test]
fn a_reopened_quote_paragraph_still_takes_the_lazy_line() {
    assert_eq!(
        carve::to_html("- > c\n  >\n  > d\nb\n"),
        carve::to_html("- > c\n  >\n  > d\n  > b\n")
    );
}
