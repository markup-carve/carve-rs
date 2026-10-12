//! The import half of the PART 10 §13a carrier inventory
//! (markup-carve/carve#2894).
//!
//! The export half landed first (carve-rs#2471), so every case here is a REAL
//! ROUND TRIP - Carve, export with the carriers on, import, compare - rather
//! than an assertion against hand-written HTML. A hand-written fixture can only
//! prove the importer reads a string the test itself wrote; the round trip
//! proves the two halves agree, which is the acceptance criterion #2894 states.
//!
//! Each case is paired with a CONTROL exported WITHOUT the carriers. Without it
//! a test cannot tell a carrier being read from an import that happened to be
//! right anyway: a passing round trip is only evidence when the same document
//! fails to round trip once the carrier is gone.

use carve::{
    html_to_ast, html_to_carve, to_html, to_html_with_options, HtmlImportDiagnosticCode,
    HtmlImportMode, HtmlImportOptions, HtmlImportSeverity, Options,
};

fn export(source: &str, carriers: bool) -> String {
    to_html_with_options(source, &Options::default().with_roundtrip(carriers))
}

fn import(html: &str) -> String {
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        ..HtmlImportOptions::default()
    };
    html_to_carve(html, &options).unwrap().value
}

/// The round trip with the carriers on must reproduce the source, and the same
/// round trip with them off must NOT - or the carrier is carrying nothing.
fn carrier_is_load_bearing(source: &str) {
    assert_eq!(
        import(&export(source, true)),
        source,
        "the carrier round trip did not reproduce the source"
    );
    assert_ne!(
        import(&export(source, false)),
        source,
        "the source round trips WITHOUT the carrier, so this case proves nothing"
    );
}

#[test]
fn an_ordered_lists_marker_carrier_restores_its_delimiter() {
    // §13a lists `data-marker` on `<ul>` AND `<ol>`. carve-php and carve-js
    // both write it on an ordered list beside §12's `data-delim`; this engine's
    // writer leaves the delimiter to §12 alone, so the carrier arrives only in
    // the other two engines' documents - which is exactly why the READER has to
    // take it. The control is a hand-written element for that reason.
    let html = r#"<ol data-marker=")"><li>a</li></ol>"#;
    assert_eq!(import(html), "1) a\n");
    // The deprecated spelling does not apply: `data-marker` carries no prefix
    // and so was never renamed (carve#2902 ruling 3).
    assert_eq!(import(r#"<ol data-marker="."><li>a</li></ol>"#), "1. a\n");
}

#[test]
fn an_ordered_marker_carrier_is_never_written_back_as_an_attribute() {
    // Ruling 5: a reader consumes the inventory and never returns a reserved
    // name as the author's content. Before `data-marker` was read on an `<ol>`
    // it fell through to the attribute policy, which refuses a carrier - so
    // this guards the consumption, not the refusal.
    let back = import(r#"<ol data-marker=")"><li>a</li></ol>"#);
    assert!(
        !back.contains("data-marker"),
        "a reserved carrier name came back as content: {back:?}"
    );
}

#[test]
fn a_footnote_label_carrier_restores_the_authored_label() {
    // Labels are assigned 1..N over document order, because an id is generated
    // navigation. So `[^note]` came back as `[^1]` - silently, since both
    // render the same `<sup>1</sup>` - and two documents differing only in
    // their labels imported identically.
    carrier_is_load_bearing("a[^note]\n\n[^note]: n\n");
}

#[test]
fn a_malformed_footnote_label_falls_back_to_the_ordinal() {
    // Empty keys and bracket delimiters cannot name a carried reference.
    for bad in ["a[b", "a]b", "", " "] {
        let html = format!(
            r##"<p>a<a id="fnref1" href="#fn1" role="doc-noteref" data-carve-footnote-label="{bad}"><sup>1</sup></a></p>"##
        ) + r##"<section role="doc-endnotes"><ol><li id="fn1"><p>n</p></li></ol></section>"##;
        let back = import(&html);
        assert!(
            back.contains("[^1]"),
            "a malformed label {bad:?} was taken as the label: {back:?}"
        );
    }
}

#[test]
fn two_endnotes_cannot_be_merged_by_a_repeated_label() {
    // A repeated label would fold two notes into one and lose a definition. The
    // second note keeps its ordinal rather than colliding.
    let html = concat!(
        r##"<p>a<a id="fnref1" href="#fn1" role="doc-noteref" data-carve-footnote-label="n"><sup>1</sup></a>"##,
        r##"b<a id="fnref2" href="#fn2" role="doc-noteref" data-carve-footnote-label="n"><sup>2</sup></a></p>"##,
        r##"<section role="doc-endnotes"><ol><li id="fn1"><p>one</p></li><li id="fn2"><p>two</p></li></ol></section>"##,
    );
    let back = import(html);
    assert!(
        back.contains("[^n]"),
        "the first label was not taken: {back:?}"
    );
    assert!(
        back.contains("[^2]"),
        "the second note did not keep an ordinal of its own: {back:?}"
    );
}

#[test]
fn the_deprecated_spelling_is_read_for_a_prefixed_carrier() {
    // Documents released carve-php versions exported carry `data-djot-`
    // (carve#2902 ruling 0). The alias has to buy the SAME import, so this is a
    // differential against the canonical spelling rather than a literal.
    let canonical = export("a[^note]\n\n[^note]: n\n", true);
    let legacy = canonical.replace("data-carve-", "data-djot-");
    assert_ne!(canonical, legacy, "the fixture carries no prefixed carrier");
    assert_eq!(import(&legacy), import(&canonical));
}

const MODES: [HtmlImportMode; 3] = [
    HtmlImportMode::Safe,
    HtmlImportMode::Semantic,
    HtmlImportMode::Roundtrip,
];

fn import_in(html: &str, mode: HtmlImportMode) -> carve::HtmlImportResult<String> {
    let options = HtmlImportOptions {
        mode,
        ..HtmlImportOptions::default()
    };
    html_to_carve(html, &options).unwrap()
}

fn tree(html: &str, mode: HtmlImportMode) -> serde_json::Value {
    let options = HtmlImportOptions {
        mode,
        ..HtmlImportOptions::default()
    };
    serde_json::from_str(&carve::to_json(&html_to_ast(html, &options).unwrap().value)).unwrap()
}

/// The `attribute-dropped` rows at or above `severity`.
fn dropped(html: &str, mode: HtmlImportMode, severity: HtmlImportSeverity) -> usize {
    import_in(html, mode)
        .report
        .diagnostics
        .iter()
        .filter(|d| d.code == HtmlImportDiagnosticCode::AttributeDropped && d.severity == severity)
        .count()
}

#[test]
fn a_marked_generated_section_drops_its_id_without_the_slug_guess() {
    // `custom` is not what `H` slugs to, so the unmarked wrapper keeps it as
    // authored; the carrier is what says it was generated.
    let marked = r#"<section id="custom" data-carve-ext="section"><h1>H</h1></section>"#;
    let unmarked = r#"<section id="custom"><h1>H</h1></section>"#;
    for mode in MODES {
        assert_eq!(import_in(marked, mode).value, "# H\n");
        assert_eq!(
            import_in(&marked.replace("data-carve-", "data-djot-"), mode).value,
            "# H\n"
        );
        assert_eq!(import_in(unmarked, mode).value, "{#custom}\n# H\n");
        assert_eq!(dropped(marked, mode, HtmlImportSeverity::Info), 0);
    }
}

#[test]
fn a_section_value_the_core_does_not_write_is_not_the_carrier() {
    let html = r#"<section id="custom" data-carve-ext="tabs"><h1>H</h1></section>"#;
    for mode in MODES {
        let result = import_in(html, mode);
        assert_eq!(result.value, "{#custom}\n# H\n");
        assert_eq!(dropped(html, mode, HtmlImportSeverity::Info), 1);
    }
}

#[test]
fn an_id_slot_places_the_id_and_clamps_to_the_last_slot() {
    let section = |slot: &str, heading: &str| {
        format!(
            r#"<section id="mine" data-carve-explicit-id data-carve-id-slot="{slot}">{heading}</section>"#
        )
    };
    let two = r#"<h1 class="c" k="v">H</h1>"#;
    for mode in MODES {
        for (slot, heading, source) in [
            ("0", two, "{#mine .c k=v}\n# H\n"),
            ("1", two, "{.c #mine k=v}\n# H\n"),
            ("2", two, "{.c k=v #mine}\n# H\n"),
            // Past the end, and past what an integer holds: last.
            ("9", two, "{.c k=v #mine}\n# H\n"),
            ("99999999999999999999999999", two, "{.c k=v #mine}\n# H\n"),
            // The other slots keep the order the heading element spells.
            (
                "1",
                r#"<h1 k="v" class="c">H</h1>"#,
                "{k=v #mine .c}\n# H\n",
            ),
            ("1", "<h1>H</h1>", "{#mine}\n# H\n"),
        ] {
            let html = section(slot, heading);
            let result = import_in(&html, mode);
            assert_eq!(result.value, source, "{mode:?} {html}");
            assert_eq!(dropped(&html, mode, HtmlImportSeverity::Info), 0, "{html}");
            assert_eq!(
                import_in(&html.replace("data-carve-", "data-djot-"), mode).value,
                source
            );
        }
    }
}

#[test]
fn an_id_slot_outside_its_grammar_is_reported_and_the_id_leads() {
    for mode in MODES {
        for slot in ["x", "-1", "1.5", "", " 1", "+1"] {
            let html = format!(
                r#"<section id="mine" data-carve-explicit-id data-carve-id-slot="{slot}"><h1 class="c">H</h1></section>"#
            );
            let result = import_in(&html, mode);
            assert_eq!(result.value, "{#mine .c}\n# H\n", "{slot:?}");
            assert_eq!(
                dropped(&html, mode, HtmlImportSeverity::Info),
                1,
                "{slot:?}"
            );
            assert!(!result.value.contains("data-carve-"));
        }
        // It rides beside the explicit-id carrier and is nothing without it.
        let alone = r#"<section id="mine" data-carve-id-slot="1"><h1 class="c">H</h1></section>"#;
        assert_eq!(import_in(alone, mode).value, "{#mine .c}\n# H\n");
        assert_eq!(dropped(alone, mode, HtmlImportSeverity::Info), 1);
    }
}

#[test]
fn a_tight_carrier_wins_over_what_the_paragraphs_imply() {
    for mode in MODES {
        let tight = r#"<ul data-carve-tight="true"><li><p class="x">a</p></li><li>b</li></ul>"#;
        assert_eq!(tree(tight, mode)["children"][0]["tight"], true);
        assert_eq!(import_in(tight, mode).value, "- {.x}\n  a\n- b\n");
        let loose = r#"<ol data-carve-tight="false"><li>a</li><li>b</li></ol>"#;
        assert_eq!(tree(loose, mode)["children"][0]["tight"], false);
        assert_eq!(
            tree("<ol><li>a</li><li>b</li></ol>", mode)["children"][0]["tight"],
            true
        );
        assert_eq!(
            tree(&tight.replace("data-carve-", "data-djot-"), mode)["children"][0]["tight"],
            true
        );
        for html in [tight, loose] {
            assert_eq!(dropped(html, mode, HtmlImportSeverity::Info), 0);
            assert!(!import_in(html, mode).value.contains("data-carve-"));
        }
        // Outside `true`/`false` it is not a carrier: the `<p>` decides.
        for value in ["yes", "TRUE", "1", ""] {
            let html = format!(
                r#"<ul data-carve-tight="{value}"><li><p class="x">a</p></li><li>b</li></ul>"#
            );
            assert_eq!(
                tree(&html, mode)["children"][0]["tight"],
                false,
                "{value:?}"
            );
            assert_eq!(
                dropped(&html, mode, HtmlImportSeverity::Info),
                1,
                "{value:?}"
            );
            assert!(!import_in(&html, mode).value.contains("data-carve-"));
        }
    }
}

#[test]
fn contradicting_carriers_restore_neither_meaning_with_one_warning() {
    // PART 10 §13d, the general rule. Each row: the element, the same element
    // with neither carrier, and the node types neither carrier may build.
    for (html, ordinary, forbidden) in [
        (
            r#"<p><a href="https://e.com" data-carve-autolink data-carve-ref="r">https://e.com</a></p>"#,
            r#"<p><a href="https://e.com">https://e.com</a></p>"#,
            vec!["autolink", "link_reference_definition"],
        ),
        (
            r#"<section id="custom" data-carve-ext="section" data-carve-explicit-id><h1>H</h1></section>"#,
            r#"<section id="custom"><h1>H</h1></section>"#,
            vec![],
        ),
        (
            r#"<p>a <span data-carve-escaped data-carve-raw="latex">*</span></p>"#,
            r#"<p>a <span>*</span></p>"#,
            vec!["escaped_text", "raw_inline"],
        ),
    ] {
        for mode in [HtmlImportMode::Safe, HtmlImportMode::Semantic] {
            for html in [html.to_owned(), html.replace("data-carve-", "data-djot-")] {
                let json = tree(&html, mode);
                assert_eq!(json, tree(ordinary, mode), "{mode:?} {html}");
                let text = json.to_string();
                for kind in &forbidden {
                    assert!(!text.contains(&format!("\"type\":\"{kind}\"")), "{html}");
                }
                assert!(!text.contains("data-carve-") && !text.contains("data-djot-"));
                assert_eq!(
                    dropped(&html, mode, HtmlImportSeverity::Warning),
                    1,
                    "{html}"
                );
                assert_eq!(dropped(&html, mode, HtmlImportSeverity::Info), 0, "{html}");
                assert_eq!(
                    import_in(&html, mode).value,
                    import_in(ordinary, mode).value
                );
            }
        }
    }
    // Trusted mode keeps an unsupported `<span>` as raw HTML, so only the two
    // rows whose ordinary element Carve spells are compared there.
    let anchor = r#"<p><a href="https://e.com" data-carve-autolink data-carve-ref="r">https://e.com</a></p>"#;
    assert_eq!(
        import_in(anchor, HtmlImportMode::Roundtrip).value,
        "[https://e.com](https://e.com)\n"
    );
    assert_eq!(
        dropped(
            anchor,
            HtmlImportMode::Roundtrip,
            HtmlImportSeverity::Warning
        ),
        1
    );
}

#[test]
fn an_invalid_value_contradicts_nothing() {
    // §13e comes first: an invalid value is not a carrier, so the valid one
    // beside it keeps its meaning and the invalid one is reported.
    for mode in MODES {
        let anchor = r#"<p><a href="https://e.com" data-carve-autolink data-carve-ref="a]b">https://e.com</a></p>"#;
        assert_eq!(import_in(anchor, mode).value, "<https://e.com>\n");
        assert_eq!(dropped(anchor, mode, HtmlImportSeverity::Warning), 0);
        assert_eq!(dropped(anchor, mode, HtmlImportSeverity::Info), 1);
    }
    // A raw `html` format is no carrier outside trusted mode.
    let span = r#"<p>a <span data-carve-escaped data-carve-raw="html">*</span></p>"#;
    assert_eq!(import_in(span, HtmlImportMode::Safe).value, "a \\*\n");
    assert_eq!(
        dropped(span, HtmlImportMode::Safe, HtmlImportSeverity::Warning),
        0
    );
}

#[test]
fn the_default_export_is_unchanged_by_this_work() {
    // §13 keeps the option OFF by default on every surface, and #2894 says the
    // default export is unchanged by the follow-up.
    for source in [
        "{#H}\n# H\n",
        "a[^note]\n\n[^note]: n\n",
        "1) a\n",
        "# Generated\n",
        "{.c #mine}\n# H\n",
        "- {.x}\n  a\n- b\n",
    ] {
        assert_eq!(to_html(source), export(source, false));
        assert!(!to_html(source).contains("data-carve-"));
    }
}
