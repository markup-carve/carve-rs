use carve::to_html;

fn host_html(host: &str, follower: &str, tail: &str) -> String {
    to_html(&format!("{host}\n> q\n{follower}\n{tail}"))
}

#[test]
fn an_interrupting_line_after_an_attached_quote_opens_in_its_host() {
    for (host, tail, close) in [
        ("- +", "", "</li>"),
        ("[^1]: +", "\n[^1]\n", "</li>"),
        (":: t\n: +", "", "</dd>"),
    ] {
        let html = host_html(host, "  # g", tail);
        let quote = html.find("</blockquote>").unwrap();
        let heading = html.find("<h1 id=\"g\">g</h1>").unwrap();
        assert!(quote < heading && heading < html[quote..].find(close).unwrap() + quote, "{host}: {html}");
    }
}

#[test]
fn a_caption_at_the_host_column_captions_an_attached_quote() {
    for (host, tail) in [
        ("- +", ""),
        ("[^1]: +", "\n[^1]\n"),
        (":: t\n: +", ""),
    ] {
        let html = host_html(host, "  ^ cap", tail);
        assert!(html.contains("<figure>"), "{host}: {html}");
        assert!(html.contains("<blockquote><p>q</p></blockquote>"), "{host}: {html}");
        assert!(html.contains("<figcaption>cap</figcaption>"), "{host}: {html}");
    }
    let below = to_html("- +\n> q\n   ^ cap\n");
    assert!(below.contains("^ cap</p></blockquote>"), "{below}");
}

#[test]
fn an_outer_host_resumes_after_a_nested_attachment_ends() {
    for (host, tail, close) in [
        ("- +", "", "</li>"),
        ("[^1]: +", "\n[^1]\n", "</li>"),
        (":: t\n: +", "", "</dd>"),
    ] {
        let source = format!("{host}\n> - +\n> ```\n> ```\n\n  # outer\n{tail}");
        let html = to_html(&source);
        let quote = html.find("</blockquote>").unwrap();
        let heading = html.find("<h1 id=\"outer\">outer</h1>").unwrap();
        assert!(quote < heading && heading < html[quote..].find(close).unwrap() + quote, "{host}: {html}");
    }
    let html = to_html("- +\n> - +\n> ```\n> ```\n\n  - y\n");
    assert!(html.contains("</blockquote>\n    <ul>\n      <li>y</li>"), "{html}");
}
