use carve::{parse, render_carve, render_html, BlockNode};

#[test]
fn table_body_source_preserves_groups_and_headers() {
    let source = "{widths=33.3,66.7 header-rows=1 body-rows=1,1 body-header-rows=1,0 body-header-cols=1,0 footer-rows=1}\n| H | G |\n| BH | BG |\n| a | b |\n| c | d |\n| F | T |\n^ Caption\n";
    let doc = parse(source);
    let BlockNode::Table(table) = &doc.children[0] else {
        panic!("not a table")
    };
    let groups = table.row_groups.as_ref().unwrap();
    assert_eq!((groups.head_rows, groups.foot_rows), (1, 1));
    assert_eq!(groups.bodies.len(), 2);
    assert_eq!(groups.bodies[0].head_rows, 1);
    assert_eq!(groups.bodies[0].row_head_columns, Some(1));
    let html = render_html(&doc).unwrap();
    assert_eq!(html.matches("<tbody>").count(), 2);
    assert!(!html.contains("body-rows="));
    let written = render_carve(&doc).unwrap();
    let reparsed = parse(&written);
    let BlockNode::Table(table) = &reparsed.children[0] else {
        panic!("not a table")
    };
    assert_eq!(table.row_groups.as_ref().unwrap(), groups);
    assert_eq!(render_html(&reparsed).unwrap(), html);
    assert_eq!(render_carve(&reparsed).unwrap(), written);
}

#[test]
fn table_body_source_supplies_missing_attrs_without_mutation() {
    for attrs in [
        "body-rows=1,1",
        "body-rows=0,1 body-header-rows=0,1 body-header-cols=,0",
        "header-rows=1 footer-rows=1 body-rows=\"\"",
    ] {
        let source = format!("{{{attrs}}}\n| a | b |\n| c | d |\n");
        let mut doc = parse(&source);
        let BlockNode::Table(table) = &mut doc.children[0] else {
            panic!("not a table")
        };
        let groups = table.row_groups.clone().unwrap();
        table.attrs = None;
        let before = carve::ast_json::to_json(&doc);
        let written = render_carve(&doc).unwrap();
        let reparsed = parse(&written);
        let BlockNode::Table(table) = &reparsed.children[0] else {
            panic!("not a table")
        };
        assert_eq!(table.row_groups.as_ref().unwrap(), &groups, "{written}");
        assert_eq!(carve::ast_json::to_json(&doc), before);
    }
}

#[test]
fn table_body_source_rejects_invalid_lists() {
    for attrs in [
        "body-rows=1",
        "body-rows=1,1 body-header-rows=0",
        "body-rows=1,1 body-header-cols=x,0",
        "body-rows=1,-1",
        "body-header-rows=1",
        "body-rows=9007199254740992",
        "header-rows=1 body-rows=x",
        "header-rows=1 body-header-rows=1",
    ] {
        let doc = parse(&format!("{{{attrs}}}\n| a | b |\n| c | d |\n"));
        let BlockNode::Table(table) = &doc.children[0] else {
            panic!("not a table")
        };
        assert!(table.row_groups.is_none(), "{attrs}");
        assert!(render_html(&doc).unwrap().contains("body-"), "{attrs}");
    }
}

#[test]
fn table_body_source_diagnoses_authored_conflicts() {
    let mut doc = parse("{body-rows=1,1}\n| a |\n| b |\n");
    let BlockNode::Table(table) = &mut doc.children[0] else {
        panic!("not a table")
    };
    table
        .attrs
        .as_mut()
        .unwrap()
        .key_values
        .insert("body-rows".into(), "1".into());
    let report = carve::conversion_diagnostics::conversion_diagnostics(&doc, 100).unwrap();
    assert_eq!(report.total_diagnostics, 1);
    assert_eq!(report.diagnostics[0].field.as_deref(), Some("rowGroups"));
    assert!(render_carve(&doc).unwrap().contains("body-rows=1"));
}

#[test]
fn table_body_source_reports_import_attribute_conflicts() {
    for html in [
        "<table header-rows=\"1\"><tbody><tr><td>1</td></tr></tbody><tfoot><tr><td>2</td></tr></tfoot></table>",
        "<table body-rows=\"9\"><tbody><tr><td>1</td></tr></tbody><tbody><tr><td>2</td></tr></tbody></table>",
    ] {
        let result = carve::html_to_carve(html, &carve::HtmlImportOptions::default()).unwrap();
        assert!(result.report.diagnostics.iter().any(|d| d.code == carve::HtmlImportDiagnosticCode::StructureUnspellable), "{:?}", result.report.diagnostics);
    }
}

#[test]
fn table_body_source_adjusts_import_counts_after_blank_rows() {
    for html in [
        "<table><tbody><tr><td>a</td></tr><tr><td></td></tr></tbody><tbody><tr><td>b</td></tr></tbody></table>",
        "<table><thead><tr><th></th></tr></thead><tbody><tr><td>a</td></tr></tbody><tfoot><tr><td>f</td></tr></tfoot></table>",
    ] {
        let result = carve::html_to_carve(html, &carve::HtmlImportOptions::default()).unwrap();
        let parsed = parse(&result.value);
        let BlockNode::Table(table) = &parsed.children[0] else { panic!("not a table: {}", result.value) };
        let groups = table.row_groups.as_ref().unwrap();
        assert_eq!(groups.head_rows, 0, "{}", result.value);
        assert_eq!(groups.head_rows + groups.foot_rows + groups.bodies.iter().map(|b| b.head_rows + b.body_rows).sum::<usize>(), table.rows.len());
        assert!(result.report.diagnostics.iter().any(|d| d.message.contains("every cell is empty")));
    }
}

#[test]
fn table_body_source_promotes_row_headers_across_spans() {
    let doc = parse("{body-rows=2,1 body-header-rows=0,1 body-header-cols=1,0}\n| a | b |\n| ^ | c |\n| ^ | D |\n| e | f |\n");
    let html = render_html(&doc).unwrap();
    assert!(
        html.contains("<th scope=\"row\" rowspan=\"3\">a</th>"),
        "{html}"
    );
    assert!(html.contains("<th scope=\"col\">D</th>"), "{html}");
}

#[test]
fn table_body_source_distinguishes_implicit_and_explicit_empty_bodies() {
    for (metadata, count) in [
        ("header-rows=1 footer-rows=1", 0),
        ("header-rows=1 footer-rows=1 body-rows=0", 1),
    ] {
        let source = format!("{{{metadata}}}\n| H | G |\n| F | T |\n");
        let doc = parse(&source);
        let BlockNode::Table(table) = &doc.children[0] else {
            panic!("not a table")
        };
        assert_eq!(table.row_groups.as_ref().unwrap().bodies.len(), count);
        assert_eq!(render_html(&doc).unwrap().contains("<tbody>"), count == 1);
        assert_eq!(render_carve(&doc).unwrap(), source);
    }
}

#[test]
fn table_body_source_keeps_unordered_imported_attributes() {
    for suffix in ["", "^ Caption\n"] {
        let mut doc = parse(&format!("{{#data .wide widths=33.3,66.7 header-rows=1 footer-rows=1}}\n| H | G |\n| a | b |\n| F | T |\n{suffix}"));
        let BlockNode::Table(table) = &mut doc.children[0] else {
            panic!("not a table")
        };
        let attrs = table.attrs.as_mut().unwrap();
        attrs.order.clear();
        attrs.key_values.clear();
        let written = render_carve(&doc).unwrap();
        assert!(written.contains("#data .wide"), "{written}");
        let reparsed = parse(&written);
        let BlockNode::Table(table) = &reparsed.children[0] else {
            panic!("not a table")
        };
        assert_eq!(table.attrs.as_ref().unwrap().id.as_deref(), Some("data"));
        assert_eq!(table.attrs.as_ref().unwrap().classes, vec!["wide"]);
        assert_eq!(render_html(&reparsed).unwrap(), render_html(&doc).unwrap());
    }
}
