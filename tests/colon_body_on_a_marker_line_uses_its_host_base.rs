fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

const ASIDE: &str = "<aside class=\"admonition note\" aria-label=\"Note\">";
const BACKLINK: &str =
    "<p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>";

fn footnote(inner: &str) -> String {
    format!("<p>[r][] [^p] <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p><section role=\"doc-endnotes\" aria-label=\"Footnotes\"><hr><ol><li id=\"fn1\">{ASIDE}{inner}</aside>{BACKLINK}</li></ol></section>")
}

#[test]
fn footnote_colon_body_opens_a_heading() {
    assert_eq!(
        html("[r][] [^p] [^c]\n[^c]: ::: note\n   # H\n"),
        footnote("<h1 id=\"H\">H</h1>")
    );
}

#[test]
fn footnote_colon_body_ignores_an_attribute_line() {
    assert_eq!(
        html("[r][] [^p] [^c]\n[^c]: ::: note\n   {#x}\n"),
        footnote("")
    );
}

#[test]
fn description_colon_body_opens_a_heading_and_leaves() {
    assert_eq!(
        html(":: t\n: ::: note\n   # H\nx\n"),
        format!("<dl><dt>t</dt><dd>{ASIDE}<h1 id=\"H\">H</h1></aside></dd></dl><p>x</p>")
    );
}

#[test]
fn ordered_item_colon_body_opens_a_table() {
    assert_eq!(
        html("1. ::: note\n    | a |\n"),
        format!(
            "<ol><li>{ASIDE}<table><tbody><tr><td>a</td></tr></tbody></table></aside></li></ol>"
        )
    );
}

#[test]
fn description_colon_body_opens_a_rule() {
    assert_eq!(
        html(":: t\n: ::: note\n   ---\n"),
        format!("<dl><dt>t</dt><dd>{ASIDE}<hr></aside></dd></dl>")
    );
}

#[test]
fn bullet_item_colon_body_opens_a_table() {
    assert_eq!(
        html("- ::: note\n   | a |\n"),
        format!(
            "<ul><li>{ASIDE}<table><tbody><tr><td>a</td></tr></tbody></table></aside></li></ul>"
        )
    );
}

#[test]
fn ordered_item_colon_body_ends_paragraph_at_attribute_line() {
    assert_eq!(
        html("1. ::: note\n   para\n       {#x}\n```\n"),
        format!("<ol><li>{ASIDE}<p>para</p></aside></li></ol><pre><code></code></pre>")
    );
}

#[test]
fn bullet_item_colon_body_ends_paragraph_at_attribute_line() {
    assert_eq!(
        html("- ::: note\n  para\n      {#x}\n"),
        format!("<ul><li>{ASIDE}<p>para</p></aside></li></ul>")
    );
}
