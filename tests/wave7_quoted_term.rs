#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn an_indented_unmarked_term_folds_into_the_open_quoted_term() {
    for pad in 1..=4 {
        for positions in [false, true] {
            let source = format!("> - :: a\n{}:: b\n>   : p\n", " ".repeat(pad));
            let doc = carve::parse_with_options(
                &source,
                &carve::Options::default().with_positions(positions),
            );
            let html = carve::render_html(&doc).unwrap();
            assert!(html.contains("<dt>a\n:: b</dt>"), "{source:?}: {html}");
            assert!(!html.contains('\0'));
            if positions {
                let tree: serde_json::Value = serde_json::from_str(&carve::to_json(&doc)).unwrap();
                let continuation = &tree["children"][0]["children"][0]["items"][0]["children"][0]
                    ["items"][0]["children"][2];
                assert_eq!(continuation["value"], ":: b");
                assert_eq!(continuation["pos"]["startLine"], 2);
                assert_eq!(continuation["pos"]["startColumn"], pad + 1);
                assert_eq!(continuation["pos"]["startOffset"], 9 + pad);
            }
        }
    }
    let html = carve::to_html("> - :: a\n>   :: b\n>   : p\n");
    assert!(html.contains("<dt>a</dt>\n        <dt>b</dt>"));
}

#[test]
#[ignore = "rule A ledger: carve-rs#3525"]
fn quoted_term_rows_match_the_checker() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/wave7_quoted_term.json")).unwrap();
    for row in rows.as_array().unwrap() {
        assert_eq!(
            carve::to_html(row["source"].as_str().unwrap()).trim(),
            row["html"].as_str().unwrap()
        );
    }
}
