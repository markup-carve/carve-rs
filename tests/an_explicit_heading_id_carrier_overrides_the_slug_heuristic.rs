use carve::{
    html_to_ast, html_to_carve, to_html_with_options, HtmlImportDiagnosticCode, HtmlImportMode,
    HtmlImportOptions, Options,
};

fn options(mode: HtmlImportMode) -> HtmlImportOptions {
    HtmlImportOptions {
        mode,
        ..HtmlImportOptions::default()
    }
}

#[test]
fn an_equal_slug_authored_id_survives_a_carrier_export_and_import() {
    for source in ["{#H}\n# H\n", "{#H .k}\n# H\n", "{.k #H}\n# H\n"] {
        let html = to_html_with_options(source, &Options::default().with_roundtrip(true));
        assert!(html.contains("data-carve-explicit-id=\"1\""));
        let imported = html_to_carve(&html, &options(HtmlImportMode::Roundtrip)).unwrap();
        assert!(imported.value.contains("#H"), "{}", imported.value);
        assert_eq!(
            to_html_with_options(&imported.value, &Options::default().with_roundtrip(true)),
            html
        );
        let ast = html_to_ast(&html, &options(HtmlImportMode::Roundtrip))
            .unwrap()
            .value;
        let json: serde_json::Value =
            serde_json::from_str(&carve::ast_json::to_json(&ast)).unwrap();
        assert_eq!(json["children"][0]["attrs"]["id"], "H");
        assert!(json["children"][0]["attrs"].get("order").is_none());
    }
}

#[test]
fn presence_is_authoritative_on_direct_and_hoisted_headings_in_every_mode() {
    for mode in [
        HtmlImportMode::Safe,
        HtmlImportMode::Semantic,
        HtmlImportMode::Roundtrip,
    ] {
        for name in ["data-carve-explicit-id", "data-djot-explicit-id"] {
            for value in ["1", "", "false"] {
                for html in [
                    format!("<h1 id=\"H\" {name}=\"{value}\">H</h1>"),
                    format!("<section id=\"H\" {name}=\"{value}\"><h1>H</h1></section>"),
                ] {
                    let result = html_to_carve(&html, &options(mode)).unwrap();
                    assert_eq!(result.value, "{#H}\n# H\n");
                    assert!(!result
                        .report
                        .diagnostics
                        .iter()
                        .any(|d| d.code == HtmlImportDiagnosticCode::AttributeDropped
                            && d.message.contains(name)));
                    let ast = html_to_ast(&html, &options(mode)).unwrap().value;
                    let json: serde_json::Value =
                        serde_json::from_str(&carve::ast_json::to_json(&ast)).unwrap();
                    assert_eq!(json["children"][0]["attrs"]["id"], "H");
                }
            }
        }
    }
}

#[test]
fn an_unmarked_generated_section_id_keeps_the_existing_heuristic() {
    let html = "<section id=\"H\"><h1>H</h1></section>";
    assert_eq!(
        html_to_carve(html, &options(HtmlImportMode::Roundtrip))
            .unwrap()
            .value,
        "# H\n"
    );
    assert_eq!(
        html_to_carve("<h1 id=\"H\">H</h1>", &options(HtmlImportMode::Roundtrip))
            .unwrap()
            .value,
        "# H\n"
    );
}

#[test]
fn a_wrapper_carrier_does_not_overwrite_a_heading_id() {
    let html = "<section id=\"H\" data-carve-explicit-id=\"1\"><h1 id=\"Other\">H</h1></section>";
    let result = html_to_carve(html, &options(HtmlImportMode::Roundtrip)).unwrap();
    assert_eq!(result.value, "{#Other}\n# H\n");
}
