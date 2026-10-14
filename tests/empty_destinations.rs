use carve::{from_json, lint_carve, parse, render_html, to_carve, to_html, to_json, to_markdown};

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
        "[t](%20%09)"
    );
    assert_eq!(
        carve::html_to_carve("<img src=\" \t\" alt=\"a\">", &options)
            .unwrap()
            .value
            .trim(),
        "![a](%20%09)"
    );
    assert_eq!(
        carve::markdown_to_carve("[t][r]\n\n[r]: <> \"T\"\n").trim(),
        "[t][r]\n\n[r]:"
    );
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
