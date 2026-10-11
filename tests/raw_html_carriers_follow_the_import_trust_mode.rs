use carve::{
    html_to_ast, html_to_carve, migrate_html, to_html, to_html_with_options, to_json,
    HtmlImportDiagnosticCode, HtmlImportMode, HtmlImportOptions, Options,
};

fn options(mode: HtmlImportMode) -> HtmlImportOptions {
    HtmlImportOptions {
        mode,
        ..HtmlImportOptions::default()
    }
}

#[test]
fn untrusted_raw_html_payloads_do_not_reappear_after_filtering() {
    for mode in [HtmlImportMode::Safe, HtmlImportMode::Semantic] {
        for name in ["data-carve-raw", "data-djot-raw"] {
            for format in ["html", "HTML"] {
                for payload in [
                    "<script>alert(1)</script>",
                    "&lt;script&gt;alert(1)&lt;/script&gt;",
                ] {
                    let input = format!("<p><span {name}=\"{format}\">{payload}</span></p>");
                    let result = html_to_carve(&input, &options(mode)).unwrap();
                    assert!(
                        !to_html(&result.value).contains("<script>"),
                        "{}",
                        result.value
                    );
                    assert!(
                        result.report.diagnostics.iter().any(|diagnostic| {
                            diagnostic.code == HtmlImportDiagnosticCode::AttributeDropped
                                && diagnostic.message.contains("data-carve-raw")
                        }),
                        "{mode:?} {name} {format} {payload}: {:?}",
                        result.report.diagnostics
                    );
                    assert_eq!(
                        migrate_html(&input, &options(mode)).unwrap().value,
                        result.value
                    );
                    let ast = html_to_ast(&input, &options(mode)).unwrap().value;
                    assert!(!to_json(&ast).contains("raw_inline"));
                }
            }
        }
    }
}

#[test]
fn safe_raw_html_carriers_keep_the_filtered_content() {
    for mode in [HtmlImportMode::Safe, HtmlImportMode::Semantic] {
        let result = html_to_carve(
            "<p><span data-carve-raw=\"html\"><b>x</b></span></p>",
            &options(mode),
        )
        .unwrap();
        assert_eq!(result.value, "*x*\n");
    }
}

#[test]
fn trusted_raw_html_carriers_restore_serialized_markup() {
    for name in ["data-carve-raw", "data-djot-raw"] {
        let input = format!("<p><span {name}=\"html\"><b>x</b></span></p>");
        let result = html_to_carve(&input, &options(HtmlImportMode::Roundtrip)).unwrap();
        assert_eq!(result.value, "`<b>x</b>`{=html}\n");
        assert!(!result.report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == HtmlImportDiagnosticCode::AttributeDropped
                && diagnostic.message.contains("data-carve-raw")
        }));
    }
}

#[test]
fn inert_raw_formats_rebuild_in_every_mode() {
    for mode in [
        HtmlImportMode::Safe,
        HtmlImportMode::Semantic,
        HtmlImportMode::Roundtrip,
    ] {
        for name in ["data-carve-raw", "data-djot-raw"] {
            let input = format!("<p><span {name}=\"tex\">\\x</span></p>");
            let result = html_to_carve(&input, &options(mode)).unwrap();
            assert_eq!(result.value, "`\\x`{=tex}\n");
        }
    }
}

#[test]
fn an_invalid_format_cannot_be_normalized_into_raw_html() {
    for mode in [HtmlImportMode::Safe, HtmlImportMode::Semantic] {
        let result = html_to_carve(
            "<p><span data-carve-raw=\"h t m l\">&lt;script&gt;alert(1)&lt;/script&gt;</span></p>",
            &options(mode),
        )
        .unwrap();
        assert!(!to_html(&result.value).contains("<script>"));
        assert!(result.report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == HtmlImportDiagnosticCode::AttributeDropped
                && diagnostic.message.contains("data-carve-raw")
        }));
    }
}

#[test]
fn a_newline_label_cannot_insert_a_raw_block() {
    let input = to_html_with_options(
        "x[^lbl]\n\n[^lbl]: note\n",
        &Options::default().with_roundtrip(true),
    );
    assert!(input.contains("data-carve-footnote-label=\"lbl\""));
    let input = input.replace(
        "data-carve-footnote-label=\"lbl\"",
        "data-carve-footnote-label=\"a&#10;&#10;```&#61;html&#10;&lt;script&gt;alert(1)&lt;/script&gt;&#10;```&#10;&#10;b\"",
    );
    for mode in [HtmlImportMode::Safe, HtmlImportMode::Semantic] {
        let result = html_to_carve(&input, &options(mode)).unwrap();
        assert_eq!(result.value, "x[^1]\n\n[^1]: note\n");
        assert!(!to_html(&result.value).contains("<script>"));
        assert!(result.report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == HtmlImportDiagnosticCode::AttributeDropped
                && diagnostic.message.contains("footnote label")
        }));
    }
}

#[test]
fn html_import_raw_carriers_decode_text_payloads_in_roundtrip_mode() {
    for name in ["data-carve-raw", "data-djot-raw"] {
        for (format, payload, decoded) in [
            ("html", "&lt;b&gt;y&lt;/b&gt;", "<b>y</b>"),
            ("html", "&lt;b&gt;`y` &amp; z&lt;/b&gt;", "<b>`y` & z</b>"),
            ("tex", "x &lt; y &amp; z", "x < y & z"),
        ] {
            let input = format!("<p>a <span {name}=\"{format}\">{payload}</span> b</p>");
            for mode in [
                HtmlImportMode::Safe,
                HtmlImportMode::Semantic,
                HtmlImportMode::Roundtrip,
            ] {
                let result = html_to_carve(&input, &options(mode)).unwrap();
                if format == "html" && mode != HtmlImportMode::Roundtrip {
                    assert_eq!(
                        result.value,
                        format!("a {} b\n", decoded.replace('`', "\\`"))
                    );
                } else {
                    let fence = if decoded.contains('`') { "``" } else { "`" };
                    assert_eq!(
                        result.value,
                        format!("a {fence}{decoded}{fence}{{={format}}} b\n")
                    );
                }
                assert_eq!(
                    migrate_html(&input, &options(mode)).unwrap().value,
                    result.value
                );
                let ast = html_to_ast(&input, &options(mode)).unwrap().value;
                let json: serde_json::Value = serde_json::from_str(&to_json(&ast)).unwrap();
                let children = json["children"][0]["children"].as_array().unwrap();
                let raw = children.iter().find(|node| node["type"] == "raw_inline");
                if format == "html" && mode != HtmlImportMode::Roundtrip {
                    assert!(raw.is_none());
                } else {
                    let raw = raw.unwrap();
                    assert_eq!(raw["format"], format);
                    assert_eq!(raw["content"], decoded);
                }
            }
        }
    }
}
