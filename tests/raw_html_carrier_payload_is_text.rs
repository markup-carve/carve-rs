use carve::{
    html_to_ast, html_to_carve, parse, render_html_with_options, to_html_with_options, to_json,
    HtmlImportMode, HtmlImportOptions, Options,
};

fn escaped(payload: &str) -> String {
    payload
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[test]
fn raw_html_carrier_payload_cannot_open_or_close_an_element() {
    for payload in [
        "<b>x</b>",
        "<b>unclosed",
        "</span><script>void(0)</script>",
        "&amp; < > \" '",
        "<!-- comment -->",
        "\u{e100}<b>x</b>\u{e101}",
    ] {
        let source = format!("`{payload}`{{=html}} tail *s*\n");
        let html = to_html_with_options(&source, &Options::default().with_roundtrip(true));
        assert_eq!(
            html,
            format!(
                "<p><span data-carve-raw=\"html\">{}</span> tail <strong>s</strong></p>",
                escaped(payload)
            ),
            "{payload:?}",
        );
        let doc = parse(&source);
        assert_eq!(
            render_html_with_options(&doc, &Options::default().with_roundtrip(true)).unwrap(),
            html
        );
        assert_eq!(
            to_html_with_options(&source, &Options::default()),
            format!("<p>{payload} tail <strong>s</strong></p>")
        );
        for mode in [
            HtmlImportMode::Safe,
            HtmlImportMode::Semantic,
            HtmlImportMode::Roundtrip,
        ] {
            let options = HtmlImportOptions {
                mode,
                ..Default::default()
            };
            let imported = html_to_ast(&html, &options).unwrap();
            let ast: serde_json::Value = serde_json::from_str(&to_json(&imported.value)).unwrap();
            let children = ast["children"][0]["children"].as_array().unwrap();
            assert_eq!(
                children.last().unwrap()["type"],
                "strong",
                "{mode:?} {payload:?}"
            );
            if mode == HtmlImportMode::Roundtrip {
                assert_eq!(children[0]["type"], "raw_inline");
                assert_eq!(children[0]["content"], payload);
                assert_eq!(html_to_carve(&html, &options).unwrap().value, source);
            } else {
                assert!(children.iter().all(|node| node["type"] != "raw_inline"));
                assert!(children.iter().any(|node| node["type"] == "text"
                    && node["value"].as_str().unwrap_or_default().contains(payload)));
            }
        }
    }
}

#[test]
fn trusted_carrier_keeps_bidi_bytes_but_disabled_raw_still_strips_them() {
    let source = "`<b>\u{202e}x\u{2066}</b>`{=html}\n";
    let trusted = to_html_with_options(source, &Options::default().with_roundtrip(true));
    assert_eq!(
        trusted,
        "<p><span data-carve-raw=\"html\">&lt;b&gt;\u{202e}x\u{2066}&lt;/b&gt;</span></p>"
    );
    let imported = html_to_ast(
        &trusted,
        &HtmlImportOptions {
            mode: HtmlImportMode::Roundtrip,
            ..Default::default()
        },
    )
    .unwrap();
    let ast: serde_json::Value = serde_json::from_str(&to_json(&imported.value)).unwrap();
    assert_eq!(
        ast["children"][0]["children"][0]["content"],
        "<b>\u{202e}x\u{2066}</b>"
    );
    for roundtrip in [false, true] {
        let mut options = Options::default().with_roundtrip(roundtrip);
        options.allow_raw_html = false;
        let html = to_html_with_options(source, &options);
        assert!(!html.contains('\u{202e}') && !html.contains('\u{2066}'));
        assert!(html.contains("&lt;b&gt;x&lt;/b&gt;"));
    }
}
