fn html(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

#[test]
fn every_multiline_family_uses_the_over_indented_opener_as_its_base() {
    let cases = [
        ("# h", "<h1 id=\"h\">h</h1>"),
        ("> q\n   lazy", "<blockquote><p>q\nlazy</p></blockquote>"),
        ("```\n     c\n   ```", "<pre><code>  c\n</code></pre>"),
        ("```=html\n     <b>x</b>\n   ```", "<b>x</b>"),
        ("%%%\n     hidden\n   %%%", "<li>x</li>"),
        (
            "::: note\n   body\n   :::",
            "<aside class=\"admonition note\"",
        ),
        ("| A |\n   | b |", "<table>"),
        (":: term\n   :  def", "<dl>"),
        ("{.c}\n   # h", "<h1 class=\"c\" id=\"h\">h</h1>"),
        ("![a](u)", "<img src=\"u\" alt=\"a\">"),
    ];
    for (body, expected) in cases {
        let output = html(&format!("- x\n\n   {body}\n"));
        assert!(output.contains(expected), "{body:?}: {output}");
        assert!(!output.contains("hidden"), "{body:?}: {output}");
    }
}

#[test]
fn below_the_minimum_still_opens_nothing() {
    assert!(!html("1. x\n > q\n").contains("<blockquote>"));
}

#[test]
fn over_indented_definitions_register() {
    let output = html("- x\n\n   [r]: /u\n   [^n]: note\n\nSee [r][] and [^n].\n");
    assert!(output.contains("<a href=\"/u\">r</a>"), "{output}");
    assert!(output.contains("role=\"doc-noteref\""), "{output}");
}

#[test]
fn a_descendant_item_keeps_its_block() {
    let output = html("- - item\n\n    # exact\n");
    let inner_list = output[4..].find("<ul>").expect("nested list");
    let heading = output.find("<h1").expect("heading");
    assert!(heading > inner_list);
}

#[test]
fn a_block_below_the_descendant_returns_to_the_parent() {
    let output = html("- a\n  - b\n\n   > q\n");
    assert!(
        output.contains("</ul>\n    <blockquote><p>q</p></blockquote>"),
        "{output}"
    );
}

#[test]
fn captions_and_wrapped_definition_terms_keep_the_authored_base() {
    for (body, expected) in [
        ("| A |\n   | 1 |\n   ^ Cap", "<caption>Cap</caption>"),
        ("![a](u)\n   ^ Cap", "<figcaption>Cap</figcaption>"),
        (
            "```\n   code\n   ```\n   ^ Cap",
            "<figcaption>Cap</figcaption>",
        ),
        (":: term\n   more\n   :  def", "<dd>def</dd>"),
    ] {
        let output = html(&format!("- x\n\n   {body}\n"));
        assert!(output.contains(expected), "{body:?}: {output}");
    }
}

#[test]
fn an_over_indented_child_marker_keeps_its_exact_column_block() {
    let exact = html("- x\n\n  - child\n\n    # h\n");
    let over = html("- x\n\n   - child\n\n     # h\n");
    assert_eq!(over, exact);
}

#[test]
fn the_definition_prepass_and_block_parser_agree_on_a_mid_paragraph_fence() {
    let output = html("- x\n  para\n   ```\n   [r]: /u\n   ```\n\n[r][]\n");
    assert!(
        output.contains("<pre><code>[r]: /u\n</code></pre>"),
        "{output}"
    );
    assert!(output.contains("<p>[r][]</p>"), "{output}");
    assert!(!output.contains("href=\"/u\""), "{output}");
}
