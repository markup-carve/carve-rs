use carve::{html_to_carve, to_html_with_options, HtmlImportMode, HtmlImportOptions, Options};

#[test]
fn footnote_carriers_restore_labels_and_inline_bodies_in_every_mode() {
    for source in [
        "a[^label]\n\n[^label]: note\n",
        "a^[note]\n",
        "a^[*note*]\n",
    ] {
        let html = to_html_with_options(source, &Options::default().with_roundtrip(true));
        for prefix in ["data-carve-", "data-djot-"] {
            let html = html.replace("data-carve-", prefix);
            for mode in [
                HtmlImportMode::Safe,
                HtmlImportMode::Semantic,
                HtmlImportMode::Roundtrip,
            ] {
                let result = html_to_carve(
                    &html,
                    &HtmlImportOptions {
                        mode,
                        ..Default::default()
                    },
                )
                .unwrap();
                assert_eq!(result.value, source);
                assert!(result.report.diagnostics.is_empty(), "{:?}", result.report);
            }
        }
    }
}

#[test]
fn invalid_and_conflicting_carriers_report_loss_without_dropping_notes() {
    let html = to_html_with_options(
        "a[^label]\n\n[^label]: note\n",
        &Options::default().with_roundtrip(true),
    );
    for html in [
        html.replacen(
            "data-carve-footnote-label=\"label\"",
            "data-carve-footnote-label=\"other\"",
            1,
        ),
        html.replace(
            "data-carve-footnote-label=\"label\"",
            "data-carve-footnote-label=\"bad[label\"",
        ),
        html.replace(
            "<li id=\"fn1\"",
            "<li data-carve-inline-footnote=\"invalid\" id=\"fn1\"",
        ),
    ] {
        let result = html_to_carve(&html, &HtmlImportOptions::default()).unwrap();
        assert!(result.value.contains("note"), "{}", result.value);
        assert!(!result.report.diagnostics.is_empty());
    }
}

#[test]
fn duplicate_labels_do_not_overwrite_a_definition() {
    let html = to_html_with_options(
        "a[^first] b[^second]\n\n[^first]: one\n\n[^second]: two\n",
        &Options::default().with_roundtrip(true),
    );
    let html = html.replace(
        "data-carve-footnote-label=\"second\"",
        "data-carve-footnote-label=\"first\"",
    );
    let result = html_to_carve(&html, &HtmlImportOptions::default()).unwrap();
    assert!(
        result.value.contains("one") && result.value.contains("two"),
        "{}",
        result.value
    );
    assert!(!result.report.diagnostics.is_empty());
}

#[test]
fn an_inline_marker_on_a_multi_paragraph_body_reports_loss() {
    let html = to_html_with_options(
        "a[^label]\n\n[^label]: one\n\n  two\n",
        &Options::default().with_roundtrip(true),
    );
    let html = html.replace(
        "<li id=\"fn1\"",
        "<li data-carve-inline-footnote=\"1\" id=\"fn1\"",
    );
    let result = html_to_carve(&html, &HtmlImportOptions::default()).unwrap();
    assert!(result.value.contains("one") && result.value.contains("two"));
    assert!(!result.report.diagnostics.is_empty());
}

#[test]
fn inline_carriers_with_multiple_references_fall_back_to_one_definition() {
    let html = to_html_with_options(
        "a[^label] b[^label]\n\n[^label]: note\n",
        &Options::default().with_roundtrip(true),
    );
    let html = html.replace(
        "<li id=\"fn1\"",
        "<li data-carve-inline-footnote=\"1\" id=\"fn1\"",
    );
    let result = html_to_carve(&html, &HtmlImportOptions::default()).unwrap();
    assert_eq!(result.value.matches("note").count(), 1, "{}", result.value);
    assert!(result.value.contains("[^label]: note"));
    assert!(!result.report.diagnostics.is_empty());
}
