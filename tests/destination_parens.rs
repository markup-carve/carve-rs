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

/// Record a row that did not render as expected, so one run names every row
/// that moved instead of stopping at the first.
fn assert_row(source: &str, expected: &str, rows: &mut Vec<String>) {
    let actual = carve::to_html(source);
    let actual = actual.trim_end();
    if actual != expected {
        rows.push(format!(
            "{source:?}\n  expected {expected}\n  actual   {actual}"
        ));
    }
}

/// `balanced_parens` is built from `destination_char` too, so a `(` still open
/// when whitespace arrives can never be part of a destination and the tail has
/// none. The scan ended at the space and asked nothing about balance, so the
/// partial prefix went out as the href `(` with the title `)`
/// (markup-carve/carve-rs#1793).
///
/// Asserted as whole renderings: "no link" also describes an engine that
/// dropped the run, and the fallback is the author's characters as text.
#[test]
fn a_title_does_not_rescue_an_unclosed_destination_parenthesis() {
    let mut rows = Vec::new();
    for (source, expected) in [
        ("[t](( \")\")\n", "<p>[t](( \u{201c})\u{201d})</p>"),
        ("![t](( \")\")\n", "<p>![t](( \u{201c})\u{201d})</p>"),
        ("[t](( ')')\n", "<p>[t](( \u{2018})\u{2019})</p>"),
        // The attribute block belongs to the construct the tail opens, and
        // there is none.
        ("[t](( \")\"){.x}\n", "<p>[t](( \u{201c})\u{201d}){.x}</p>"),
        (
            "![a](( \")\"){.x}\n",
            "<p>![a](( \u{201c})\u{201d}){.x}</p>",
        ),
        // The pair closes after the quoted run, which leaves a destination
        // carrying a space and a tail ending later than the scan thought.
        ("[t](( \"a\") b)\n", "<p>[t](( \u{201c}a\u{201d}) b)</p>"),
        // No quoted run at all. This row reads the same with the balance check
        // gone, because what follows the space is neither a title nor the
        // tail's `)`; it is here for the shape, not as a witness.
        ("[t](a( b)\n", "<p>[t](a( b)</p>"),
    ] {
        assert_row(source, expected, &mut rows);
    }
    assert!(rows.is_empty(), "{}", rows.join("\n"));
}

/// Rows that keep the balance check pointed at the open pair rather than at
/// the parenthesis.
#[test]
fn a_balanced_pair_still_builds_the_construct() {
    let mut rows = Vec::new();
    for (source, expected) in [
        (
            "[t](a(b) \"c\")\n",
            "<p><a href=\"a(b)\" title=\"c\">t</a></p>",
        ),
        ("[t]((a))\n", "<p><a href=\"(a)\">t</a></p>"),
        // A quote inside the destination opens no title, so the `)` that ends
        // the tail is the one after the second quote.
        (
            "[t](a\"b \"c\")\n",
            "<p><a href=\"a&quot;b\" title=\"c\">t</a></p>",
        ),
        (
            "[t](/u \"T)\")\n",
            "<p><a href=\"/u\" title=\"T)\">t</a></p>",
        ),
        (
            "![a](http://a/b(c))\n",
            "<img src=\"http://a/b(c)\" alt=\"a\">",
        ),
    ] {
        assert_row(source, expected, &mut rows);
    }
    assert!(rows.is_empty(), "{}", rows.join("\n"));
}

fn round_trips(src: &str) {
    let out = carve::to_carve(src);
    assert_eq!(carve::to_html(&out), carve::to_html(src));
    assert_eq!(carve::to_carve(&out), out, "fmt is not idempotent");
}

// These drove the writer through a REFERENCE plus a definition, because a
// resolved reference was inlined and the destination therefore ended up inside
// `(...)`. It is not inlined any more (carve-rs#631), so the vehicle is an INLINE
// link - the claim is unchanged: the writer escapes a parenthesis exactly when the
// destination scan would misread it. The definition-line case is asserted below.
#[test]
fn the_writer_leaves_a_balanced_pair_bare() {
    let src = "[wiki](https://en.wikipedia.org/wiki/Foo_(bar))\n";
    let out = carve::to_carve(src);
    assert!(
        out.contains("(https://en.wikipedia.org/wiki/Foo_(bar))"),
        "expected a bare balanced pair, got: {out}"
    );
    round_trips(src);
}

#[test]
fn the_writer_leaves_a_definition_destination_unescaped() {
    // On a definition line the destination is not inside `(...)`, so a
    // parenthesis needs no escape and must not gain one. Byte-identical to
    // carve-js and carve-php.
    let src = "[x][w]\n\n[w]: http://a/b)c\n";
    assert_eq!(carve::to_carve(src), "[x][w]\n\n[w]: http://a/b)c\n");
    round_trips(src);
}

#[test]
fn the_writer_escapes_an_unbalanced_parenthesis() {
    let src = "[x](http://a/b\\)c)\n";
    assert!(carve::to_carve(src).contains("(http://a/b\\)c)"));
    round_trips(src);

    let src = "[x](http://a/b\\(c)\n";
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
