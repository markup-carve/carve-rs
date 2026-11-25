//! C2-M-037 (carve#6663, ruling A): every `<td>` of the `<thead>` that becomes
//! the `table_head` is a header cell, reported once at the `<thead>`.
//! CC-2100 to CC-2104 are inlined: the pinned spec commit predates them.

use carve::api::{self, ImportOptions};

/// Code, severity, channel and byte offset of one report.
type Entry = (String, String, String, Option<usize>);

fn import(html: &str) -> (String, Vec<Entry>) {
    let got = api::import_document(html, &ImportOptions::default()).unwrap();
    let entries = got
        .diagnostics
        .iter()
        .map(|d| {
            (
                d.code.to_string(),
                d.severity.to_string(),
                d.channel.to_string(),
                d.range.map(|r| r.start_offset),
            )
        })
        .collect();
    (got.source, entries)
}

fn repaired_at(offset: usize) -> Entry {
    (
        "import-structure-repaired".into(),
        "warning".into(),
        "render-loss".into(),
        Some(offset),
    )
}

#[test]
fn cc_2100_thead_row_of_td_cells() {
    let html = "<table><thead><tr><td>a</td><td>b</td></tr></thead><tbody><tr><td>1</td><td>2</td></tr></tbody></table>";
    assert_eq!(
        import(html),
        ("|= a |= b |\n| 1 | 2 |\n".into(), vec![repaired_at(7)])
    );
}

#[test]
fn cc_2101_empty_td_corner_and_td_labels() {
    let html = "<table><thead><tr><td></td><td>Q1</td><td>Q2</td></tr></thead><tbody><tr><th>North</th><td>1</td><td>2</td></tr></tbody></table>";
    assert_eq!(
        import(html),
        (
            "|= |= Q1 |= Q2 |\n|= North | 1 | 2 |\n".into(),
            vec![repaired_at(7)]
        )
    );
}

#[test]
fn cc_2102_empty_td_corner_beside_th_labels() {
    let html = "<table><thead><tr><td></td><th>Q1</th><th>Q2</th></tr></thead><tbody><tr><th>North</th><td>1</td><td>2</td></tr></tbody></table>";
    assert_eq!(
        import(html),
        (
            "|= |= Q1 |= Q2 |\n|= North | 1 | 2 |\n".into(),
            vec![repaired_at(7)]
        )
    );
}

#[test]
fn cc_2103_two_td_rows_report_once() {
    let html = "<table><thead><tr><td>a</td></tr><tr><td>b</td></tr></thead><tbody><tr><td>1</td></tr></tbody></table>";
    assert_eq!(
        import(html),
        ("|= a |\n|= b |\n| 1 |\n".into(), vec![repaired_at(7)])
    );
}

#[test]
fn cc_2104_td_of_a_further_thead_stays_data() {
    let html = "<table><thead><tr><th>h</th></tr></thead><thead><tr><td>x</td></tr></thead><tbody><tr><td>1</td></tr></tbody></table>";
    assert_eq!(
        import(html),
        (
            "{body-rows=1,1}\n|= h |\n| x |\n| 1 |\n".into(),
            vec![repaired_at(41)]
        )
    );
}

#[test]
fn th_only_thead_is_not_reported() {
    let html = "<table><thead><tr><th>a</th></tr></thead><tr><td>1</td></tr></table>";
    assert_eq!(import(html), ("|= a |\n| 1 |\n".into(), vec![]));
}

#[test]
fn td_thead_above_a_header_cell_foot_reads_back() {
    let html =
        "<table><thead><tr><td>a</td></tr></thead><tfoot><tr><th>F</th></tr></tfoot></table>";
    assert_eq!(
        import(html).0,
        "{footer-rows=1 body-rows}\n|= a |\n|= F |\n"
    );
}
