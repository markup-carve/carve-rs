//! markup-carve/carve#4470 (PART 9 §5 T6, CARVE-P12-069), corpus rows
//! 856-a-continuation-cell-past-the-last-column-is-a-new-cell.

fn first_row_cells(source: &str) -> serde_json::Value {
    let mut doc: serde_json::Value =
        serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
    fn strip(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                map.remove("pos");
                map.values_mut().for_each(strip);
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(strip),
            _ => {}
        }
    }
    strip(&mut doc);
    doc["children"][0]["rows"][0]["cells"].clone()
}

#[test]
fn a_cell_past_the_last_column_renders() {
    for (source, html) in [
        (
            "| a | b |\n+ c | d | e |\n",
            "<table>\n  <tbody>\n    <tr><td>a c</td><td>b d</td><td>e</td></tr>\n  </tbody>\n</table>",
        ),
        (
            "| a |\n+ b | | c |\n+ d | | e |\n",
            "<table>\n  <tbody>\n    <tr><td>a b d</td><td></td><td>c e</td></tr>\n  </tbody>\n</table>",
        ),
        (
            "| a |\n+ b | |\n",
            "<table>\n  <tbody>\n    <tr><td>a b</td></tr>\n  </tbody>\n</table>",
        ),
        (
            "| a | x |\n| b |\n+ c | ^ |\n",
            "<table>\n  <tbody>\n    <tr><td>a</td><td>x</td></tr>\n    <tr><td>b c</td><td>^</td></tr>\n  </tbody>\n</table>",
        ),
        (
            "|= a |\n+ b | c |\n| d | e |\n",
            "<table>\n  <thead>\n    <tr><th scope=\"col\">a b</th><th scope=\"col\">c</th></tr>\n  </thead>\n  <tbody>\n    <tr><td>d</td><td>e</td></tr>\n  </tbody>\n</table>",
        ),
        (
            "| a |\n+ b |\n+ c | d |\n",
            "<table>\n  <tbody>\n    <tr><td>a b c</td><td>d</td></tr>\n  </tbody>\n</table>",
        ),
    ] {
        assert_eq!(carve::to_html(source), html, "{source:?}");
    }
}

#[test]
fn the_appended_cell_starts_with_the_wrap_of_its_line() {
    assert_eq!(
        first_row_cells("| a |\n+ b | | c |\n"),
        serde_json::json!([
            {"type": "table_cell", "header": false, "children": [
                {"type": "text", "value": "a"}, {"type": "cell_wrap", "line": 1}, {"type": "text", "value": "b"}]},
            {"type": "table_cell", "header": false, "children": [{"type": "cell_wrap", "line": 1}]},
            {"type": "table_cell", "header": false, "children": [
                {"type": "cell_wrap", "line": 1}, {"type": "text", "value": "c"}]},
        ])
    );
}

#[test]
fn the_appended_cell_is_written_back_on_its_plus_line() {
    for source in [
        "| a | b |\n+ c | d | e |\n",
        "| a |\n+ b | | c |\n+ d | | e |\n",
        "| a | x |\n| b |\n+ c | ^ |\n",
        "|= a |\n+ b | c |\n| d | e |\n",
        "> | a |\n> + b | c |\n",
        "| a |\n+ b |\n+ c | d |\n",
    ] {
        assert_eq!(carve::to_carve(source), source, "{source:?}");
    }
}

#[test]
fn lint_reports_the_row_on_the_plus_line() {
    let findings = |source: &str| -> Vec<(usize, usize, String)> {
        carve::lint_carve(source)
            .into_iter()
            .map(|w| (w.line, w.column, w.rule.to_string()))
            .collect()
    };
    assert_eq!(
        findings("| a | b |\n+ c | d | e |\n"),
        [(2, 1, "table-row-cell-count".to_string())]
    );
    assert_eq!(
        findings("> | a |\n> + b | c |\n"),
        [(2, 3, "table-row-cell-count".to_string())]
    );
    assert!(findings("| a | b |\n+ c | d |\n").is_empty());
}
