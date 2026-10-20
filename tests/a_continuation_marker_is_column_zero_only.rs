use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Deserialize, Serialize)]
struct Row {
    source: String,
    html: String,
}

fn normalize(html: &str) -> String {
    let mut in_pre = false;
    let mut out = Vec::new();
    for line in html.trim().split('\n') {
        if in_pre {
            out.push(line.to_string());
        } else if !line.trim().is_empty() {
            out.push(line.trim().to_string());
        }
        if line.contains("<pre ") || line.contains("<pre>") {
            in_pre = true;
        }
        if line.contains("</pre>") {
            in_pre = false;
        }
    }
    out.join("\n")
}

pub(crate) fn assert_legacy_model(source: &str, actual: &str) {
    let rows: Vec<Row> =
        serde_json::from_str(include_str!("fixtures/column_zero_legacy_cases.json")).unwrap();
    let expected = rows
        .iter()
        .find(|row| row.source == source)
        .unwrap_or_else(|| panic!("missing model row: {source:?}"));
    assert_eq!(normalize(actual), normalize(&expected.html), "{source:?}");
}

#[test]
fn column_zero_model_rows() {
    let data = std::env::var("CARVE_COL0_ORACLE")
        .map(|path| fs::read_to_string(path).unwrap())
        .unwrap_or_else(|_| include_str!("fixtures/column_zero_representative.json").to_string());
    let rows: Vec<Row> = serde_json::from_str(&data).unwrap();
    let mut differences = Vec::new();
    for row in &rows {
        let actual = carve::to_html(&row.source);
        if normalize(&actual) != normalize(&row.html)
            || normalize(&carve::to_html_with_options(
                &row.source,
                &carve::Options::default().with_positions(true),
            )) != normalize(&row.html)
        {
            differences.push(Row {
                source: row.source.clone(),
                html: actual,
            });
        }
    }
    if let Ok(path) = std::env::var("CARVE_COL0_DIFFERENCES") {
        fs::write(path, serde_json::to_string_pretty(&differences).unwrap()).unwrap();
    }
    assert!(
        differences.is_empty(),
        "{} of {} rows differ",
        differences.len(),
        rows.len()
    );
}

#[test]
fn legacy_column_zero_cases_match_model() {
    let rows: Vec<Row> =
        serde_json::from_str(include_str!("fixtures/column_zero_legacy_cases.json")).unwrap();
    let differences: Vec<_> = rows
        .iter()
        .filter(|row| {
            normalize(&carve::to_html(&row.source)) != normalize(&row.html)
                || normalize(&carve::to_html_with_options(
                    &row.source,
                    &carve::Options::default().with_positions(true),
                )) != normalize(&row.html)
        })
        .map(|row| row.source.as_str())
        .collect();
    assert!(
        differences.is_empty(),
        "{} legacy rows differ: {differences:?}",
        differences.len()
    );
}

#[test]
fn original_2946_shapes_match_model() {
    assert_model_rows(include_str!("fixtures/column_zero_2946_model.json"));
}

#[test]
fn task_item_shapes_match_model() {
    assert_model_rows(include_str!("fixtures/column_zero_task_items.json"));
}

fn assert_model_rows(data: &str) {
    let rows: Vec<Row> = serde_json::from_str(data).unwrap();
    let differences: Vec<_> = rows
        .iter()
        .filter(|row| {
            normalize(&carve::to_html(&row.source)) != normalize(&row.html)
                || normalize(&carve::to_html_with_options(
                    &row.source,
                    &carve::Options::default().with_positions(true),
                )) != normalize(&row.html)
        })
        .map(|row| row.source.as_str())
        .collect();
    assert!(
        differences.is_empty(),
        "{} of {} rows differ: {differences:?}",
        differences.len(),
        rows.len()
    );
}

#[test]
fn nested_tight_items_write_blocks_without_indented_markers() {
    for source in [
        "- a\n  - > one\n\n    > two\n",
        "- a\n  - first\n\n    second\n",
        "- a\n  - ```\n    x\n    ```\n\n    > two\n",
    ] {
        let written = carve::to_carve(source);
        assert_eq!(
            normalize(&carve::to_html(&written)),
            normalize(&carve::to_html(source)),
            "{source:?}: {written:?}"
        );
        assert!(
            written
                .lines()
                .all(|line| line.trim() != "+" || line.starts_with('+')),
            "{written:?}"
        );
    }
}

#[test]
fn nested_tightness_loss_is_reported() {
    let mut doc = carve::parse("- a\n  - b\n");
    let mut child_doc = carve::parse("> one\n\nsecond\n");
    let children = std::mem::take(&mut child_doc.children);
    let carve::BlockNode::List(outer) = &mut doc.children[0] else {
        panic!("outer list");
    };
    let inner = outer.items[0]
        .children
        .iter_mut()
        .find_map(|block| match block {
            carve::BlockNode::List(list) => Some(list),
            _ => None,
        })
        .expect("inner list");
    assert!(inner.tight);
    inner.items[0].children = children;
    let report = carve::with_render_loss_report(
        carve::RenderTarget::Carve,
        carve::CheckedRenderOptions::default(),
        || carve::render_carve(&doc).expect("writes Carve"),
    )
    .expect("loss report");
    assert!(report
        .losses
        .iter()
        .any(|loss| loss.code == "list-tightness-lost"));
    assert!(report
        .value
        .lines()
        .all(|line| line.trim() != "+" || line.starts_with('+')));
}
