use carve::to_html;

#[test]
fn a_refused_quoted_description_plus_closes_the_quote() {
    assert_eq!(
        to_html("> :: t\n> : +\npara\n").trim_end(),
        "<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd></dd>\n  </dl>\n</blockquote>\n<p>para</p>"
    );
    for prefix in ["> ", "> > ", "> > > "] {
        let source = format!("{prefix}:: t\n{prefix}: +\npara\n");
        let html = to_html(&source);
        assert!(
            html.trim_end().ends_with("</blockquote>\n<p>para</p>"),
            "{source:?}: {html}"
        );
        assert!(
            !html.contains("<blockquote><p>para</p>"),
            "{source:?}: {html}"
        );
    }
}

#[test]
fn a_refused_list_hosted_description_plus_closes_the_quote() {
    for source in [
        "> - :: t\n>   : +\npara\n",
        "> 1. :: t\n>    : +\npara\n",
        "> - x\n>\n>   :: t\n>   : +\npara\n",
        "> - > :: t\n>   > : +\npara\n",
    ] {
        let html = to_html(source);
        assert!(
            html.trim_end().ends_with("</blockquote>\n<p>para</p>"),
            "{source:?}: {html}"
        );
    }
}

#[test]
fn refused_first_block_leaves_an_intervening_list_in_the_description() {
    for marker in ["-", "*", "1.", "- [x]"] {
        let source = format!(":: t\n: {marker} +\n   - z\n");
        let html = to_html(&source);
        let full = carve::try_to_html_with_options(&source, &carve::Options::default()).unwrap();
        assert_eq!(html, full, "{source:?}");
        assert!(html.contains("<dd>\n"), "{source:?}: {html}");
        assert!(html.contains("z</li>"), "{source:?}: {html}");
        assert!(!html.contains("<p>- z</p>"), "{source:?}: {html}");
    }
}

#[test]
fn a_document_column_line_does_not_attach_to_a_nested_item() {
    let html = to_html(":: t\n: - +\nx\n");
    // The refused marker renders nothing and `x` ends the description (carve#5013).
    assert!(
        html.contains("<li></li>") && html.contains("<p>x</p>"),
        "{html}"
    );
}

#[test]
fn a_refused_second_plus_renders_nothing_in_the_attached_item_paragraph() {
    for source in ["> - q\n> +\n> x\n> +\n", "::: box\n- q\n+\nx\n+\n:::\n"] {
        let html = to_html(source);
        assert!(
            html.contains("q\n") && html.contains("x\n"),
            "{source:?}: {html}"
        );
        assert!(!html.contains('+'), "{source:?}: {html}");
        assert!(!html.contains("<p>+</p>"), "{source:?}: {html}");
    }
}

#[test]
fn a_refused_plus_keeps_a_note_attached_list_after_a_closed_child_fence() {
    let source = "[^a]: +\n- li\n  ```\n  x\n  ```\n+\n - sib\n\nsee[^a]\n";
    let expected = "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>li\n          <pre><code>x\n</code></pre>\n        </li>\n        <li>sib</li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(to_html(source), expected);
}

#[test]
fn a_marker_column_sibling_after_a_refused_plus_stays_in_the_note() {
    let source = "[^a]: +\n- li\n  ```\n  x\n  ```\n+\n- sib\n\nsee[^a]\n";
    let expected = "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li><p>li</p>\n          <pre><code>x\n</code></pre>\n        </li>\n        <li><p>sib</p></li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(to_html(source), expected);
}
