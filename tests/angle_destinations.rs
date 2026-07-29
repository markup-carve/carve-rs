//! A destination written `<...>` can carry characters a bare run cannot.
//!
//! A bare destination stops at the first `)` or whitespace - deliberate, and
//! corpus 107 pins it - so a URL containing a parenthesis could not be
//! expressed at all. Formatting one truncated the href and leaked the rest into
//! the text, in all three engines (carve issue 377).

fn href_of(src: &str) -> String {
    let html = carve::to_html(src);
    let start = html.find("href=\"").expect("a link") + 6;
    let rest = &html[start..];
    rest[..rest.find('"').expect("closing quote")].to_string()
}

#[test]
fn carries_a_parenthesis() {
    assert_eq!(
        href_of("[a](<https://x/Foo_(bar)>)\n"),
        "https://x/Foo_(bar)"
    );
}

#[test]
fn carries_a_space() {
    assert_eq!(href_of("[a](<https://x/a b>)\n"), "https://x/a b");
}

#[test]
fn leaves_a_bare_destination_alone() {
    assert_eq!(href_of("[a](https://x/plain)\n"), "https://x/plain");
}

#[test]
fn does_not_change_where_a_bare_destination_stops() {
    // corpus 107 pins this: the run ends at the first `(`.
    assert_eq!(href_of("[a](http://a/b(c))\n"), "http://a/b(c");
}

#[test]
fn an_unclosed_angle_stays_ordinary_content() {
    // No closing `>`, so the bare scan runs and stops at the `)`. The leading
    // `<` is ordinary content, and the HTML renderer escapes it in the
    // attribute.
    assert_eq!(href_of("[a](<https://x/plain)\n"), "&lt;https://x/plain");
}

#[test]
fn the_writer_reaches_for_it_only_when_it_has_to() {
    let src = "[wiki][w]\n\n[w]:  https://en.wikipedia.org/wiki/Foo_(bar)\n";
    let out = carve::to_carve(src);
    assert!(
        out.contains("(<https://en.wikipedia.org/wiki/Foo_(bar)>)"),
        "expected the angle form, got: {out}"
    );
    assert_eq!(carve::to_html(&out), carve::to_html(src));
    assert_eq!(carve::to_carve(&out), out, "fmt is not idempotent");

    // An ordinary URL stays bare.
    assert_eq!(
        carve::to_carve("[a](https://x/plain)\n"),
        "[a](https://x/plain)\n"
    );
}
