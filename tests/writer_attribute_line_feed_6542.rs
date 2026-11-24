use carve::api::{Carve, WriteOptions, decode_tree};

#[test]
fn supplied_attribute_line_feeds_keep_the_span_and_report_the_value() {
    let tree = decode_tree(r#"{"type":"document","astVersion":"2.0","children":[{"type":"paragraph","children":[{"type":"span","attrs":{"classes":["c"],"keyValues":[["data-k","x\n\ny"]]},"children":[{"type":"text","value":"t"}]}]}]}"#).unwrap();
    let written = Carve::new()
        .write_with_diagnostics(&tree, &WriteOptions::default())
        .unwrap();
    assert_eq!(written.output, "[t]{.c data-k=\"x  y\"}\n");
    assert_eq!(written.diagnostics.len(), 1);
    assert_eq!(written.diagnostics[0].code, "field-unspellable");
    assert_eq!(
        written.diagnostics[0].path.as_deref(),
        Some("/children/0/children/0/attrs/keyValues/0/1")
    );
}

#[test]
fn imported_attribute_line_feeds_keep_span_and_paragraph_attributes() {
    for (html, expected, path) in [
        (
            "<p><span title=\"a\nb\">[ x</span></p>",
            "[\\[ x]{title=\"a b\"}\n",
            "/children/0/children/0/attrs/keyValues/0/1",
        ),
        (
            "<p title=\"a\nb\">x</p>",
            "{title=\"a b\"}\nx\n",
            "/children/0/attrs/keyValues/0/1",
        ),
    ] {
        let imported =
            carve::api::import_document(html, &carve::api::ImportOptions::default()).unwrap();
        let written = Carve::new()
            .write_with_diagnostics(&imported.tree, &WriteOptions::default())
            .unwrap();
        assert_eq!(written.output, expected);
        assert!(
            written
                .diagnostics
                .iter()
                .any(|d| d.code == "field-unspellable" && d.path.as_deref() == Some(path))
        );
    }
}
