use carve::{
    from_json, lint_carve, markdown_to_ast, parse, render_html, to_carve, to_html, to_json,
    to_markdown,
};

#[test]
fn inline_empty_destinations_are_nodes_and_canonicalize() {
    for (source, html, canonical) in [
        ("[t]()", "<p><a href=\"\">t</a></p>", "[t]()"),
        ("[t]( )", "<p><a href=\"\">t</a></p>", "[t]()"),
        (
            "[t]( \"T\")",
            "<p><a href=\"\" title=\"T\">t</a></p>",
            "[t]( \"T\")",
        ),
        ("![a]()", "<img src=\"\" alt=\"a\">", "![a]()"),
    ] {
        assert_eq!(to_html(source).trim(), html, "{source}");
        assert_eq!(to_carve(source).trim(), canonical, "{source}");
    }
}

#[test]
fn a_bare_empty_definition_resolves_and_survives_json() {
    for source in ["[t][r]\n\n[r]:", "![a][r]\n\n[r]:"] {
        let document = parse(source);
        let decoded = from_json(&to_json(&document)).unwrap();
        assert_eq!(
            render_html(&document).unwrap(),
            render_html(&decoded).unwrap()
        );
        assert_eq!(to_carve(source).trim(), source);
    }
    assert_eq!(
        to_html("[t][r]\n\n[r]:").trim(),
        "<p><a href=\"\">t</a></p>"
    );
    assert_eq!(
        to_html("![a][r]\n\n[r]:").trim(),
        "<img src=\"\" alt=\"a\">"
    );
    assert_eq!(to_html("[t][r]").trim(), "<p>[t][r]</p>");
    let warnings = lint_carve("[t][r]\n\n[r]:");
    assert!(warnings
        .iter()
        .any(|warning| warning.rule == "empty-reference-definition"));
    assert!(!warnings
        .iter()
        .any(|warning| warning.rule == "unresolved-reference-link"));
}

#[test]
fn markdown_export_spells_an_empty_destination_explicitly() {
    assert_eq!(to_markdown("[t]()").trim(), "[t](<>)");
    assert_eq!(to_markdown("![a]()").trim(), "![a](<>)");
}

#[test]
fn foreign_imports_keep_explicit_empty_destinations() {
    assert_eq!(carve::markdown_to_carve("[t](<>)"), "[t]()\n");
    assert_eq!(
        carve::migrate_djot("[t][r]\n\n[r]:\n").value.trim(),
        "[t][r]\n\n[r]:"
    );
    let options = carve::HtmlImportOptions::default();
    assert_eq!(
        carve::html_to_carve("<a href=\"\">t</a>", &options)
            .unwrap()
            .value
            .trim(),
        "[t]()"
    );
    assert_eq!(
        carve::html_to_carve("<img src=\"\" alt=\"a\">", &options)
            .unwrap()
            .value
            .trim(),
        "![a]()"
    );
    assert_eq!(
        carve::html_to_carve("<a href=\" \t\">t</a>", &options)
            .unwrap()
            .value
            .trim(),
        "[t]()"
    );
    assert_eq!(
        carve::html_to_carve("<img src=\" \t\" alt=\"a\">", &options)
            .unwrap()
            .value
            .trim(),
        "![a]()"
    );
    assert_eq!(
        carve::markdown_to_carve("[t][r]\n\n[r]: <> \"T\"\n").trim(),
        "[t]( \"T\")"
    );
}

#[test]
fn markdown_autolink_brackets_keep_encoded_destinations() {
    for (source, destination) in [
        (
            "[foo<https://example.com/?search=](uri)>\n",
            "https://example.com/?search=%5D(uri)",
        ),
        (
            "[foo<https://example.com/?search=][ref]>\n\n[ref]: /uri\n",
            "https://example.com/?search=%5D%5Bref%5D",
        ),
    ] {
        let document = markdown_to_ast(source);
        let expected = format!("href=\"{destination}\"");
        assert!(
            render_html(&document).unwrap().contains(&expected),
            "{source}"
        );
        let converted = carve::migrate_markdown(source);
        assert!(to_html(&converted.value).contains(&expected), "{source}");
        assert!(converted.value.contains("search=\\]"), "{source}");
    }
    let unicode = "<https://example.com/ä>";
    assert_eq!(carve::migrate_markdown(unicode).value.trim(), unicode);
    assert!(to_json(&markdown_to_ast(unicode)).contains("https://example.com/ä"));
}

#[test]
fn empty_definition_writer_reports_each_discarded_field() {
    let wire = serde_json::json!({"type":"document","srcByteLength":0,"children":[
        {"type":"link_reference_definition","label":"r","href":"","title":"T","attrs":{"id":"x"}}
    ]});
    let doc = from_json(&wire.to_string()).unwrap();
    assert_eq!(carve::render_carve(&doc).unwrap().trim(), "[r]:");
    let report = carve::conversion_diagnostics(&doc, 100).unwrap();
    let fields: Vec<_> = report
        .diagnostics
        .iter()
        .filter_map(|d| d.field.as_deref())
        .collect();
    assert!(fields.contains(&"title"));
    assert!(fields.contains(&"attrs"));
}

#[test]
fn foreign_ast_imports_resolve_empty_references() {
    let document = markdown_to_ast("[t][r] and ![a][r]\n\n[r]: <>");
    assert_eq!(
        render_html(&document).unwrap().trim(),
        "<p><a href=\"\">t</a> and <img src=\"\" alt=\"a\"></p>"
    );

    let options = carve::HtmlImportOptions::default();
    let link =
        carve::html_to_ast("<p><a href=\"\" data-carve-ref=\"r\">t</a></p>", &options).unwrap();
    assert_eq!(
        render_html(&link.value).unwrap().trim(),
        "<p><a href=\"\">t</a></p>"
    );
    let image =
        carve::html_to_ast("<img src=\"\" alt=\"a\" data-carve-ref=\"r\">", &options).unwrap();
    assert_eq!(
        render_html(&image.value).unwrap().trim(),
        "<img src=\"\" alt=\"a\">"
    );
}
