use carve::{ast_json, parse, to_html_with_options, BlockNode, Options};

#[test]
fn shared_ast_vectors_retain_termination_and_decode_literal_code() {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/raw-payload-termination.json")).unwrap();
    for vector in vectors.as_array().unwrap() {
        let wire =
            serde_json::json!({"type":"document", "srcByteLength":0, "children":[vector["raw"]]});
        let doc = ast_json::from_json(&wire.to_string()).unwrap();
        let back: serde_json::Value = serde_json::from_str(&ast_json::to_json(&doc)).unwrap();
        assert_eq!(back, wire);
        let BlockNode::RawBlock(raw) = &doc.children[0] else {
            panic!("missing raw")
        };
        assert_eq!(raw.code_content(), vector["codeContent"].as_str().unwrap());
    }
}

#[test]
fn source_eof_matches_literal_code_in_safe_mode() {
    let options = Options {
        allow_raw_html: false,
        ..Options::default()
    };
    for payload in ["", "\n", "x", "x\n", "x\n\n"] {
        let source = format!("```=html\n{payload}");
        let doc = parse(&source);
        let BlockNode::RawBlock(raw) = &doc.children[0] else {
            panic!("missing raw")
        };
        assert_eq!(
            raw.content_terminated,
            if !payload.is_empty() && !payload.ends_with('\n') {
                Some(false)
            } else {
                None
            }
        );
        assert_eq!(
            to_html_with_options(&source, &options),
            to_html_with_options(&format!("```html\n{payload}"), &options)
        );
    }
}

#[test]
fn shared_source_hosts_preserve_safe_and_profile_payloads() {
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/raw-payload-source-fixtures.json")).unwrap();
    for fixture in fixtures.as_array().unwrap() {
        let source = fixture["source"].as_str().unwrap();
        let code_source = fixture["codeSource"].as_str().unwrap();
        for options in [
            Options {
                allow_raw_html: false,
                ..Options::default()
            },
            Options {
                profile: Some(carve::Profile::article()),
                ..Options::default()
            },
        ] {
            assert_eq!(
                to_html_with_options(source, &options),
                to_html_with_options(code_source, &options),
                "{}",
                fixture["id"]
            );
        }
        assert_eq!(
            ast_json::to_json(&parse(source)).contains("\"contentTerminated\":false"),
            fixture["contentTerminated"] == false,
            "{}",
            fixture["id"]
        );
    }
}

#[test]
fn formatter_reports_the_termination_change() {
    let report =
        carve::conversion_diagnostics::conversion_diagnostics(&parse("```=html\nx"), 100).unwrap();
    assert_eq!(
        report.diagnostics[0].field.as_deref(),
        Some("contentTerminated")
    );
}

#[test]
fn prosemirror_roundtrip_keeps_termination() {
    let doc = parse("```=html\nx");
    let pm = carve::to_prosemirror(&doc);
    let back = carve::from_prosemirror(&pm.json).unwrap();
    let BlockNode::RawBlock(raw) = &back.children[0] else {
        panic!("missing raw")
    };
    assert_eq!(raw.content_terminated, Some(false));
    assert_eq!(raw.code_content(), "x");
}
