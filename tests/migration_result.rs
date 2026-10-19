use carve::{
    migrate_bbcode, migrate_djot, migrate_html, migrate_markdown, HtmlImportMode,
    HtmlImportOptions, HtmlImportSeverity, MigrationConfidence, MigrationFidelity, SourceFormat,
};

#[test]
fn bbcode_quote_user_ids_are_reported_at_source_lines() {
    let source =
        "intro\r\n[quote=\"alice\" post_id=12 user_id=3]q[/quote]\n[quote=bob user_id=4]r[/quote]";
    let result = migrate_bbcode(source).unwrap();
    let losses: Vec<_> = result
        .report
        .diagnostics
        .iter()
        .filter(|row| row.code == "bbcode-quote-field-dropped")
        .collect();
    assert_eq!(losses.len(), 2);
    assert_eq!(losses[0].path.as_deref(), Some("line:2"));
    assert_eq!(losses[1].path.as_deref(), Some("line:3"));
    assert!(losses.iter().all(|row| row.message.contains("user_id")));
    assert!(losses.iter().all(|row| {
        row.fidelity == MigrationFidelity::Dropped && row.confidence == MigrationConfidence::Exact
    }));
}

#[test]
fn bbcode_quote_user_ids_inside_literal_content_are_not_reported() {
    for source in [
        "[code][quote=user_id=7]x[/quote][/code]",
        "[c][quote=user_id=7]x[/icode]",
        "[noparse][quote=user_id=7]x[/quote][/noparse]",
        "[quote name=\"a user_id=3\"]x[/quote]",
    ] {
        let result = migrate_bbcode(source).unwrap();
        assert!(!result
            .report
            .diagnostics
            .iter()
            .any(|row| row.code == "bbcode-quote-field-dropped"));
    }
    assert!(
        migrate_bbcode("[url=https://example.test][quote=alice user_id=7]x[/quote][/url]")
            .unwrap()
            .report
            .diagnostics
            .iter()
            .any(|row| row.code == "bbcode-quote-field-dropped")
    );
    for source in [
        "[url][quote=alice user_id=7]x[/quote][/url]",
        "[img][quote=alice user_id=7]x[/quote][/img]",
        "[quote=alice user_id=7]x",
    ] {
        assert!(migrate_bbcode(source)
            .unwrap()
            .report
            .diagnostics
            .iter()
            .any(|row| row.code == "bbcode-quote-field-dropped"));
    }
}

#[test]
fn every_source_format_returns_the_same_result_shape() {
    let markdown = migrate_markdown("**strong**");
    assert_eq!(markdown.value, "*strong*\n");
    assert_eq!(markdown.report.schema_version, 2);
    assert_eq!(markdown.report.source_format, SourceFormat::Markdown);
    assert_eq!(markdown.report.diagnostics.len(), 2);
    assert_eq!(
        markdown.report.diagnostics[0].fidelity,
        MigrationFidelity::Preserved
    );
    assert_eq!(
        markdown.report.diagnostics[0].confidence,
        MigrationConfidence::Exact
    );
    assert_eq!(
        markdown.report.diagnostics[0].severity,
        HtmlImportSeverity::Info
    );

    let djot = migrate_djot("_emphasis_");
    assert_eq!(djot.value, "/emphasis/");
    assert_eq!(djot.report.source_format, SourceFormat::Djot);
    assert_eq!(djot.report.diagnostics[0].code, "fidelity-unverified");

    let bbcode = migrate_bbcode("[b]strong[/b]").expect("BBCode import succeeds");
    assert_eq!(bbcode.report.schema_version, 2);
    assert_eq!(bbcode.report.source_format, SourceFormat::Bbcode);
    assert_eq!(
        bbcode.report.diagnostics[0].fidelity,
        MigrationFidelity::Dropped
    );

    let html = migrate_html("<p><blink>text</blink></p>", &HtmlImportOptions::default())
        .expect("HTML import succeeds");
    assert_eq!(html.report.source_format, SourceFormat::Html);
    assert_eq!(html.report.mode, Some(HtmlImportMode::Safe));
    assert!(!html.report.diagnostics.is_empty());
}

#[test]
fn html_report_pins_degraded_preserved_and_truncated_classifications() {
    let unwrapped = migrate_html("<ruby>x<rt>y</rt></ruby>", &HtmlImportOptions::default())
        .expect("HTML import succeeds");
    assert!(unwrapped.report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "element-unwrapped"
            && diagnostic.fidelity == MigrationFidelity::Degraded
            && diagnostic.confidence == MigrationConfidence::Exact
    }));

    let preserved = migrate_html(
        "<unknown onclick=\"x()\">x</unknown>",
        &HtmlImportOptions {
            mode: HtmlImportMode::Roundtrip,
            ..Default::default()
        },
    )
    .expect("roundtrip HTML import succeeds");
    assert!(preserved.report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "raw-preserved" && diagnostic.fidelity == MigrationFidelity::Degraded
    }));
    assert!(preserved.report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "attribute-preserved"
            && diagnostic.fidelity == MigrationFidelity::Preserved
    }));

    let truncated = migrate_html(
        "<script>x</script><script>y</script>",
        &HtmlImportOptions {
            max_diagnostics: 1,
            ..Default::default()
        },
    )
    .expect("bounded HTML import succeeds");
    assert!(truncated.report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "diagnostics-truncated"
            && diagnostic.fidelity == MigrationFidelity::Dropped
            && diagnostic.confidence == MigrationConfidence::Fallback
    }));
}

#[test]
fn html_loss_is_classified_for_callers() {
    let result = migrate_html(
        "<p><kbd kbd=lit>text</kbd></p>",
        &HtmlImportOptions::default(),
    )
    .expect("HTML import succeeds");
    let diagnostic = result
        .report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "attribute-dropped")
        .expect("the colliding semantic attribute cannot be represented");
    assert_eq!(diagnostic.fidelity, MigrationFidelity::Dropped);
}

#[test]
fn literal_text_has_exact_evidence_and_other_constructs_remain_unverified() {
    for source in [
        "",
        "hello",
        "plain text",
        "Grüße 123",
        "日本語",
        "hello\r\n\r\n",
    ] {
        for result in [
            migrate_markdown(source),
            migrate_djot(source),
            migrate_bbcode(source).unwrap(),
        ] {
            assert_eq!(result.report.diagnostics.len(), 1, "{source:?}");
            let row = &result.report.diagnostics[0];
            assert_eq!(row.code, "literal-text-verified", "{source:?}");
            assert_eq!(row.fidelity, MigrationFidelity::Preserved);
            assert_eq!(row.confidence, MigrationConfidence::Exact);
        }
    }
    for source in [
        "# heading",
        "*bold*",
        "[b]text[/b]",
        "a\nb",
        "    code",
        "1. item",
        "a  b",
        "a\tb",
        "hello!",
        " hello",
        "hello ",
        "a\u{00a0}b",
        "e\u{0301}",
        "a\r\nb",
        "a\rb",
    ] {
        for result in [migrate_djot(source), migrate_bbcode(source).unwrap()] {
            assert!(
                result
                    .report
                    .diagnostics
                    .iter()
                    .any(|row| row.code == "fidelity-unverified"),
                "{source:?}"
            );
        }
    }
}
