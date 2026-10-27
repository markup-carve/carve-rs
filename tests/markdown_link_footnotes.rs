use carve::{markdown_to_carve, migrate_markdown, to_html};

#[test]
fn markdown_link_label_keeps_its_footnote_reference() {
    for (source, expected_link) in [
        (
            "[t[^1]](u)\n\n[^1]: n\n",
            "<a href=\"u\">t<sup id=\"fnref1\">1</sup></a>",
        ),
        (
            "[a &amp; [^note] **bold**](/u)\n\n[^note]: n\n",
            "<a href=\"/u\">a &amp; <sup id=\"fnref1\">1</sup> <strong>bold</strong></a>",
        ),
        (
            "[t[^1]][target]\n\n[target]: /u\n[^1]: n\n",
            "<a href=\"/u\">t<sup id=\"fnref1\">1</sup></a>",
        ),
        (
            "[t[^1][^2]](u)\n\n[^1]: a\n[^2]: b\n",
            "<a href=\"u\">t<sup id=\"fnref1\">1</sup><sup id=\"fnref2\">2</sup></a>",
        ),
    ] {
        let migrated = migrate_markdown(source);
        let html = to_html(&migrated.value);
        assert!(html.contains(expected_link), "{source:?}: {html}");
        assert!(
            !migrated
                .report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "structure-unspellable"),
            "{source:?}: {:?}",
            migrated.report.diagnostics
        );
    }
}

#[test]
fn markdown_code_and_plain_footnotes_do_not_become_links() {
    let source = "`[t[^1]](u)` and [^1]\n\n[^1]: n\n";
    let migrated = markdown_to_carve(source);
    let html = to_html(&migrated);
    assert!(html.contains("<code>[t[^1]](u)</code>"), "{html}");
    assert!(!html.contains("<a href=\"u\">"), "{html}");
    assert!(html.contains("role=\"doc-noteref\""), "{html}");
}
