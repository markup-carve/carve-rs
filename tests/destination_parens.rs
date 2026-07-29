//! A destination's parentheses balance, so a URL that carries one needs no
//! escape and no second spelling (carve issue 377). Djot and CommonMark both do
//! this; the cases below were checked against djot 0.3.2 and commonmark.js.

fn href_of(src: &str) -> String {
    let html = carve::to_html(src);
    let start = html.find("href=\"").expect("a link") + 6;
    let rest = &html[start..];
    rest[..rest.find('"').expect("closing quote")].to_string()
}

#[test]
fn keeps_a_parenthesized_tail_inside_the_url() {
    assert_eq!(
        href_of("[x](https://en.wikipedia.org/wiki/Foo_(bar))\n"),
        "https://en.wikipedia.org/wiki/Foo_(bar)"
    );
}

#[test]
fn nests_to_any_depth() {
    assert_eq!(href_of("[x](a(b(c))d)\n"), "a(b(c))d");
}

#[test]
fn ends_at_a_parenthesis_with_no_opener_left() {
    assert_eq!(carve::to_html("[x](e)f)\n"), "<p><a href=\"e\">x</a>f)</p>");
}

#[test]
fn an_unclosed_opener_does_not_swallow_the_line() {
    assert_eq!(carve::to_html("[t](url(more\n"), "<p>[t](url(more</p>");
}

#[test]
fn an_escaped_parenthesis_is_content_not_nesting() {
    assert_eq!(href_of("[x](http://a/b\\)c)\n"), "http://a/b)c");
    assert_eq!(href_of("[x](http://a/b\\(c)\n"), "http://a/b(c");
}

#[test]
fn a_backslash_before_anything_else_is_literal() {
    assert_eq!(href_of("[x](a\\qb)\n"), "a\\qb");
    assert_eq!(href_of("[x](a\\\\b)\n"), "a\\b");
}

#[test]
fn whitespace_still_ends_the_destination_so_a_title_can_follow() {
    assert_eq!(
        carve::to_html("[x](/u \"t\")\n"),
        "<p><a href=\"/u\" title=\"t\">x</a></p>"
    );
}

fn round_trips(src: &str) {
    let out = carve::to_carve(src);
    assert_eq!(carve::to_html(&out), carve::to_html(src));
    assert_eq!(carve::to_carve(&out), out, "fmt is not idempotent");
}

#[test]
fn the_writer_leaves_a_balanced_pair_bare() {
    let src = "[wiki][w]\n\n[w]:  https://en.wikipedia.org/wiki/Foo_(bar)\n";
    let out = carve::to_carve(src);
    assert!(
        out.contains("(https://en.wikipedia.org/wiki/Foo_(bar))"),
        "expected a bare balanced pair, got: {out}"
    );
    round_trips(src);
}

#[test]
fn the_writer_escapes_an_unbalanced_parenthesis() {
    let src = "[x][w]\n\n[w]:  http://a/b)c\n";
    assert!(carve::to_carve(src).contains("(http://a/b\\)c)"));
    round_trips(src);

    let src = "[x][w]\n\n[w]:  http://a/b(c\n";
    assert!(carve::to_carve(src).contains("(http://a/b\\(c)"));
    round_trips(src);
}

#[test]
fn the_writer_leaves_an_ordinary_destination_alone() {
    assert_eq!(
        carve::to_carve("[a](https://x/plain)\n"),
        "[a](https://x/plain)\n"
    );
}
