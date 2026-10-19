#[test]
fn csv_and_pipe_tables_project_named_and_positional_values() {
    let csv = "{table #sales}\n```csv\nQuarter,Revenue\nQ1,120\nQ2,150\n```\n\n@{#sales[\"Q2\"][\"Revenue\"]}{code}";
    let html = carve::to_html(csv);
    assert!(html.contains("<code>150</code>"), "{html}");
    assert!(carve::render_carve(&carve::parse(csv))
        .unwrap()
        .contains("@{#sales[\"Q2\"][\"Revenue\"]}{code}"));

    let pipe = "{#sales}\n|= Quarter |= Revenue |\n| Q1 | 120 |\n| Q2 | 150 |\n\n@{#sales[\"Q2\"][\"Revenue\"]} @{#sales[2][2]}";
    assert!(carve::to_html(pipe).contains("<p>150 150</p>"));
}

#[test]
fn row_keys_links_and_literal_fallback() {
    let row_key = "{table #sales row-key=Quarter}\n```csv\nRevenue,Quarter\n120,Q1\n150,Q2\n```\n\n@{#sales[\"Q2\"][\"Revenue\"]}";
    let html = carve::to_html(row_key);
    assert!(html.contains("<p>150</p>"), "{html}");
    assert!(!html.contains("row-key="), "{html}");

    let link = "{#s}\n|= K |= A\\|B |\n| Q]2 | 150 |\n\n[Value @{#s[\"Q]2\"][\"A|B\"]}](/report)";
    let html = carve::to_html(link);
    assert!(html.contains("<a href=\"/report\">Value 150</a>"), "{html}");

    let missing = "{#s}\n|= K |= V |\n| Q2 | 150 |\n\n@{#s[\"Q3\"][\"V\"]}";
    assert!(carve::to_html(missing).contains("@{#s[\"Q3\"][\"V\"]}"));
    assert!(carve::lint_carve(missing)
        .iter()
        .any(|warning| warning.rule == "unresolved-table-cell-ref"));
    let malformed = "@{#s[0][2]}";
    assert!(carve::to_html(malformed).contains(malformed));
    assert!(carve::lint_carve(malformed)
        .iter()
        .any(|warning| warning.rule == "malformed-table-cell-ref"));
}

#[test]
fn wire_roundtrip_and_header_rows() {
    let source = "{#s}\n|= K |= V |\n| Q1 | 111 |\n|= Group |= Header |\n| Q2 | 222 |\n\n@{#s[2][2]} @{#s[3][2]}";
    let html = carve::to_html(source);
    assert!(html.contains("<p>222 @{#s[3][2]}</p>"), "{html}");
    let wire = carve::to_json(&carve::parse(source));
    assert!(wire.contains("\"type\":\"table_cell_ref\""), "{wire}");
    let decoded = carve::from_json(&wire).unwrap();
    assert!(carve::render_html(&decoded)
        .unwrap()
        .contains("<p>222 @{#s[3][2]}</p>"));
    let invalid = wire.replacen("\"row\":2", "\"row\":1", 1);
    assert!(carve::from_json(&invalid).is_err());
    assert!(carve::render_carve(&decoded)
        .unwrap()
        .contains("@{#s[2][2]}"));
}
