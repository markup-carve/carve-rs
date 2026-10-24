#[test]
fn a_description_below_a_rebased_term_stays_text() {
    let expected = "<dl>\n  <dt>a</dt>\n  <dd>\n    <p>b</p>\n    <dl>\n      <dt>t</dt>\n    </dl>\n    <p>: zq</p>\n  </dd>\n</dl>";
    for indent in ["\t", "    ", "  \t"] {
        let source = format!(":: a\n: b\n{indent}:: t\n  : zq\n");
        assert_eq!(carve::to_html(&source), expected, "indent {indent:?}");
        let formatted = carve::to_carve(&source);
        assert_eq!(carve::to_html(&formatted), expected, "indent {indent:?}");
        assert_eq!(carve::to_carve(&formatted), formatted, "indent {indent:?}");
    }
}

#[test]
fn a_description_at_the_rebased_terms_column_still_attaches() {
    let source = ":: a\n: b\n    :: t\n    : zq\n";
    let html = carve::to_html(source);
    assert!(html.contains("<dt>t</dt>\n      <dd>zq</dd>"));
}

#[test]
fn lazy_lines_below_a_rebased_term_keep_their_existing_reading() {
    let source = "- x\n\n\t:: t\n : zq\n";
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li>x\n    <dl>\n      <dt>t\n: zq</dt>\n    </dl>\n  </li>\n</ul>"
    );

    let source = ":: a\n: b\n    :: t\n    : d\n : zq\n";
    let html = carve::to_html(source);
    assert!(html.contains("<dd>d\n: zq</dd>"), "{html}");
    assert!(!html.contains("&nbsp;L&nbsp;"), "{html}");
}

#[test]
fn literal_description_lines_survive_markdown_import() {
    let source = ":: a\n: b\n\t:: t\n  : zq\n";
    let markdown =
        carve::to_markdown_with_options(source, &carve::Options::default().with_roundtrip(true));
    assert!(markdown.contains("\\: zq"), "{markdown}");
    assert_eq!(
        carve::to_html(&carve::markdown_to_carve(&markdown)),
        carve::to_html(source)
    );

    for (source, expected) in [("\\: text\n", ": text"), ("\\:: term\n", "\\:: term")] {
        let markdown = carve::to_markdown(source);
        assert!(markdown.starts_with(expected), "{markdown}");
        assert_eq!(
            carve::to_html(&carve::markdown_to_carve(&markdown)),
            carve::to_html(source)
        );
    }

    let source = "- one\n  :: term\n:  def\n";
    let markdown =
        carve::to_markdown_with_options(source, &carve::Options::default().with_roundtrip(true));
    assert!(markdown.contains("\n:  def\n"), "{markdown}");
    assert_eq!(
        carve::to_html(&carve::markdown_to_carve(&markdown)),
        carve::to_html(source)
    );

    let source = ":: a\n: b\n  \\: \n";
    let markdown =
        carve::to_markdown_with_options(source, &carve::Options::default().with_roundtrip(true));
    assert!(markdown.contains("\\:"), "{markdown}");
    assert_eq!(
        carve::to_html(&carve::markdown_to_carve(&markdown)),
        carve::to_html(source)
    );

    let source = ":: a\n: b\n  \\:\n";
    let markdown =
        carve::to_markdown_with_options(source, &carve::Options::default().with_roundtrip(true));
    assert!(markdown.contains("\\:\n"), "{markdown}");
    assert_eq!(
        carve::to_html(&carve::markdown_to_carve(&markdown)),
        carve::to_html(source)
    );

    let source = ":: a\n: b\n\n- item\n\n  :: t\n  : d\n";
    let markdown =
        carve::to_markdown_with_options(source, &carve::Options::default().with_roundtrip(true));
    assert_eq!(
        carve::to_html(&carve::markdown_to_carve(&markdown)),
        carve::to_html(source),
        "{markdown}"
    );
}

#[test]
fn a_bare_term_marker_needs_no_markdown_escape() {
    for source in ["\\::\n", "\\::\nc\n"] {
        let markdown = carve::to_markdown(source);
        assert!(markdown.starts_with("::\n"), "{markdown}");
        assert!(!markdown.contains("\\::\n"), "{markdown}");
        assert_eq!(
            carve::to_html(&carve::markdown_to_carve(&markdown)),
            carve::to_html(source),
            "{source:?}"
        );
    }
}
