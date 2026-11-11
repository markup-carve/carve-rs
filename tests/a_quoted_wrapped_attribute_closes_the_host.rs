use carve::to_html;

#[test]
fn a_quoted_wrapped_attribute_releases_an_outer_item() {
    let source = "- > - b\n  > {.x\n  >  .y}\ntail\n";
    assert_eq!(
        to_html(source),
        "<ul>\n  <li>\n    <blockquote>\n      <ul>\n        <li>b</li>\n      </ul>\n    </blockquote>\n  </li>\n</ul>\n<p>tail</p>"
    );
}

#[test]
fn each_valid_wrapped_quote_column_releases_the_outer_item() {
    for indent in 0..=3 {
        let opener = format!("{}{{.x", " ".repeat(indent));
        let closer = format!("{} .y}}", " ".repeat(indent));
        let source = format!("- > - b\n  > {opener}\n  > {closer}\ntail\n");
        let html = to_html(&source);
        assert!(html.ends_with("</ul>\n<p>tail</p>"), "{source:?}: {html}");
        assert!(!html.contains("{.x"), "{source:?}: {html}");
    }
}

#[test]
fn a_refused_run_remains_text() {
    let source = "- > - b\n  > {.x\n  > .y}\ntail\n";
    let html = to_html(source);
    assert!(html.contains("<li>b\n{.x\n.y}\ntail</li>"), "{html}");
}

#[test]
fn a_parent_quote_attribute_closes_a_child_code_span() {
    let source = "- > - `a\n  > {.x\n  >  .y}\ntail\n";
    assert_eq!(
        to_html(source),
        "<ul>\n  <li>\n    <blockquote>\n      <ul>\n        <li><code>a</code></li>\n      </ul>\n    </blockquote>\n  </li>\n</ul>\n<p>tail</p>"
    );
}
