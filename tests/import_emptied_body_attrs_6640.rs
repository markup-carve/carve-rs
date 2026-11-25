//! C2-M-037, C2-M-082 (carve#6640): the attributes of a `<tbody>` that the head
//! move empties are dropped with it and reported once, at the `<tbody>`.
//! CC-2075 and CC-2076 are inlined: the pinned spec commit predates them.

use carve::api::{self, ImportOptions};

/// Byte offset of each report of `code`.
fn import(html: &str) -> (String, Vec<(String, String, String, Option<usize>)>) {
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

fn dropped_at(offset: usize) -> (String, String, String, Option<usize>) {
    (
        "import-attribute-dropped".into(),
        "warning".into(),
        "render-loss".into(),
        Some(offset),
    )
}

#[test]
fn cc_2075_class_of_an_emptied_tbody() {
    let html = "<table><tbody class=\"x\"><tr><th>A</th><th>B</th></tr></tbody></table>";
    assert_eq!(import(html), ("|= A |= B |\n".into(), vec![dropped_at(7)]));
}

#[test]
fn cc_2076_id_and_class_give_one_entry() {
    let html = "<table><tbody id=\"a\" class=\"x\"><tr><th>A</th></tr></tbody></table>";
    assert_eq!(import(html), ("|= A |\n".into(), vec![dropped_at(7)]));
}

#[test]
fn every_attribute_kind_gives_one_entry() {
    for attrs in [
        " id=\"a\"",
        " style=\"color:red\"",
        " data-k=\"1\"",
        " lang=\"de\"",
        " title=\"t\"",
        " class=\"x y\" data-k=\"v\" style=\"color:red\"",
    ] {
        let html = format!("<table><tbody{attrs}><tr><th>A</th></tr></tbody></table>");
        assert_eq!(
            import(&html),
            ("|= A |\n".into(), vec![dropped_at(7)]),
            "{html}"
        );
    }
}

#[test]
fn emptied_tbody_without_attributes_gives_no_entry() {
    let html = "<table><tbody><tr><th>A</th><th>B</th></tr></tbody></table>";
    assert_eq!(import(html), ("|= A |= B |\n".into(), vec![]));
}

#[test]
fn a_tbody_that_keeps_rows_keeps_its_attributes() {
    let html = "<table><tbody class=\"x\"><tr><th>A</th></tr><tr><td>b</td></tr></tbody></table>";
    let got = api::import_document(html, &ImportOptions::default());
    let codes: Vec<String> = match &got {
        Ok(g) => g.diagnostics.iter().map(|d| d.code.to_string()).collect(),
        Err(_) => vec!["refused".into()],
    };
    assert!(
        !codes.iter().any(|c| c == "import-attribute-dropped"),
        "{codes:?}"
    );
}
