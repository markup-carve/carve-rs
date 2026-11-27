use carve::api::{self, ImportOptions};

#[test]
fn ranged_comment_uses_reader_order_for_duplicate_snippet_ordinals() {
    let inputs = [
        "<details><p>a <span data-carve-snippet=\"1:inline:$v\">X</span></p><summary>s <span class=\"comment-start\" data-comment-id=\"1\">n</span><span data-carve-snippet=\"1:inline:$v\"></span><span class=\"comment-end\" data-comment-id=\"1\"></span>t</summary></details>",
        "<section role=\"doc-endnotes\"><ol><li><p>q <span data-carve-snippet=\"1:inline:$v\">X</span></p></li></ol></section><p><span class=\"comment-start\" data-comment-id=\"1\">n</span><span data-carve-snippet=\"1:inline:$v\"></span><span class=\"comment-end\" data-comment-id=\"1\"></span>y</p>",
    ];
    for input in inputs {
        let imported = api::import_document(input, &ImportOptions::default()).expect("import");
        assert!(
            imported.diagnostics.iter().any(|d| {
                d.code == "import-structure-repaired"
                    && d.message == "Ranged comment read as a point comment; its range is lost"
            }),
            "{input}: {} {:?}",
            imported.source,
            imported.diagnostics
        );
    }
}

#[test]
fn dropped_comment_end_content_cannot_claim_a_snippet_ordinal() {
    let input = "<p><span class=\"comment-start\" data-comment-id=\"1\">a</span><span class=\"comment-end\" data-comment-id=\"1\"><span data-carve-snippet=\"1:inline:$v\"></span></span><span class=\"comment-start\" data-comment-id=\"2\">b</span><span data-carve-snippet=\"1:inline:$v\"></span><span class=\"comment-end\" data-comment-id=\"2\"></span></p>";
    let imported = api::import_document(input, &ImportOptions::default()).expect("import");
    assert!(imported.source.contains("{{ $v }}"));
    assert!(
        imported.diagnostics.iter().any(|d| {
            d.code == "import-structure-repaired"
                && d.message == "Ranged comment read as a point comment; its range is lost"
        }),
        "{} {:?}",
        imported.source,
        imported.diagnostics
    );
}

#[test]
fn reader_order_carrier_choice_is_stable_across_1000_range_variants() {
    let mut count = 0;
    for shape in 0..2 {
        for ordinal in 1..=5 {
            for comment_id in 1..=5 {
                for lead in ["n", "note", "mark", "x", "hello"] {
                    for tail in ["t", "tail", "z", "end"] {
                        let start = format!(
                            "<span class=\"comment-start\" data-comment-id=\"{comment_id}\">{lead}</span>"
                        );
                        let end = format!(
                            "<span class=\"comment-end\" data-comment-id=\"{comment_id}\"></span>"
                        );
                        let use_ =
                            format!("<span data-carve-snippet=\"{ordinal}:inline:$v\"></span>");
                        let earlier =
                            format!("<span data-carve-snippet=\"{ordinal}:inline:$v\">X</span>");
                        let html = if shape == 0 {
                            format!(
                                "<details><p>a {earlier}</p><summary>s {start}{use_}{end}{tail}</summary></details>"
                            )
                        } else {
                            format!(
                                "<section role=\"doc-endnotes\"><ol><li><p>q {earlier}</p></li></ol></section><p>{start}{use_}{end}{tail}</p>"
                            )
                        };
                        let imported =
                            api::import_document(&html, &ImportOptions::default()).expect("import");
                        assert!(
                            imported.diagnostics.iter().any(|d| {
                                d.code == "import-structure-repaired"
                                    && d.message
                                        == "Ranged comment read as a point comment; its range is lost"
                            }),
                            "{html}: {}",
                            imported.source
                        );
                        count += 1;
                    }
                }
            }
        }
    }
    assert_eq!(count, 1000);
}
