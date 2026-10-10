use serde::Deserialize;

#[derive(Deserialize)]
struct Cases {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    source: String,
    html: String,
    canonical: Option<String>,
    #[serde(default)]
    children: serde_json::Value,
}

#[test]
fn shared_nested_braced_emphasis_vectors() {
    let vectors: Cases = serde_json::from_str(include_str!("fixtures/nested-braced-emphasis.json")).unwrap();
    let mut failures = Vec::new();
    for case in vectors.cases {
        let mut document: serde_json::Value = serde_json::from_str(&carve::to_json(&carve::parse(&case.source))).unwrap();
        normalize_semantics(&mut document);
        assert_eq!(document["children"][0]["children"], case.children, "{}", case.id);
        let actual = carve::to_html(&case.source);
        if actual.trim() != case.html {
            failures.push(format!("{}: {}", case.id, actual.trim()));
        }
        let written = carve::render_carve(&carve::parse(&case.source)).unwrap();
        assert_eq!(carve::to_html(&written).trim(), case.html, "{}: {}", case.id, written);
        assert_eq!(carve::render_carve(&carve::parse(&written)).unwrap(), written, "{}", case.id);
        if let Some(canonical) = case.canonical {
            assert_eq!(written.trim_end(), canonical, "{}", case.id);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn comment_and_unclosed_code_boundaries() {
    for (source, html) in [
        ("{*a {% *} %} b*}", "<p><strong>a  b</strong></p>"),
        ("{*a {# *} #} b*}", "<p><strong>a <span class=\"critic-comment\"> *} </span> b</strong></p>"),
        ("a {*b %% c*}", "<p>a <strong>b</strong></p>"),
        ("{*a `{*}", "<p><strong>a <code>{</code></strong></p>"),
    ] {
        assert_eq!(carve::to_html(source).trim(), html, "{}", source);
        let written = carve::render_carve(&carve::parse(source)).unwrap();
        assert_eq!(carve::to_html(&written).trim(), html, "{}: {}", source, written);
    }
}

#[test]
fn malformed_recovery_vectors() {
    let vectors: Vec<Case> = serde_json::from_str(include_str!("fixtures/malformed-braced-emphasis.json")).unwrap();
    for case in vectors {
        assert_eq!(carve::to_html(&case.source).trim(), case.html, "{}", case.id);
        let written = carve::render_carve(&carve::parse(&case.source)).unwrap();
        assert_eq!(carve::to_html(&written).trim(), case.html, "{}: {}", case.id, written);
    }
}

fn normalize_semantics(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for key in ["pos", "srcByteLength", "bulletChar", "number"] {
                map.remove(key);
            }
            for child in map.values_mut() { normalize_semantics(child); }
        }
        serde_json::Value::Array(items) => {
            for child in items { normalize_semantics(child); }
        }
        _ => {}
    }
}

#[test]
fn shared_host_vectors() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!("fixtures/nested-braced-emphasis.json")).unwrap();
    for case in vectors["hostCases"].as_array().unwrap() {
        if !case["remainingInlineDepth"].is_null() { continue; }
        let source = case["source"].as_str().unwrap();
        let mut actual: serde_json::Value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
        let mut expected = case["document"].clone();
        normalize_semantics(&mut actual);
        normalize_semantics(&mut expected);
        if case["id"] == "host-heading" {
            actual["children"][0].as_object_mut().unwrap().remove("attrs");
        }
        assert_eq!(actual, expected, "{}", case["id"]);
        let html = carve::to_html(source).replace("<section id=\"a-b-c\">", "<section>");
        assert_eq!(html.trim(), case["html"].as_str().unwrap(), "{}", case["id"]);
    }
}

#[test]
fn shared_depth_vectors() {
    std::thread::Builder::new().stack_size(32 * 1024 * 1024)
        .spawn(shared_depth_vectors_with_room).unwrap().join().unwrap();
}

fn shared_depth_vectors_with_room() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!("fixtures/nested-braced-emphasis.json")).unwrap();
    for case in vectors["depthCases"].as_array().unwrap() {
        let padding = 200 - case["remainingInlineDepth"].as_u64().unwrap() as usize - 2;
        let source = format!("{}{}{}", "{/".repeat(padding), case["source"].as_str().unwrap(), "/}".repeat(padding));
        let encoded = carve::to_json(&carve::parse(&source));
        let mut decoder = serde_json::Deserializer::from_str(&encoded);
        decoder.disable_recursion_limit();
        let document = serde_json::Value::deserialize(&mut decoder).unwrap();
        let mut children = &document["children"][0]["children"];
        for _ in 0..padding {
            assert_eq!(children[0]["type"], "emphasis", "{}", case["id"]);
            children = &children[0]["children"];
        }
        let mut actual = children.clone();
        normalize_semantics(&mut actual);
        assert_eq!(actual, case["children"], "{}", case["id"]);
    }
}

fn unwrap_padding(value: &mut serde_json::Value, padding: usize) -> bool {
    if value["children"][0]["type"] == "emphasis" {
        let mut children = value["children"].clone();
        for _ in 0..padding { children = children[0]["children"].clone(); }
        value["children"] = children;
        return true;
    }
    match value {
        serde_json::Value::Object(map) => map.values_mut().any(|child| unwrap_padding(child, padding)),
        serde_json::Value::Array(items) => items.iter_mut().any(|child| unwrap_padding(child, padding)),
        _ => false,
    }
}

#[test]
fn shared_host_depth_vectors() {
    std::thread::Builder::new().stack_size(32 * 1024 * 1024).spawn(|| {
        let vectors: serde_json::Value = serde_json::from_str(include_str!("fixtures/nested-braced-emphasis.json")).unwrap();
        for case in vectors["hostCases"].as_array().unwrap() {
            let Some(remaining) = case["remainingInlineDepth"].as_u64() else { continue; };
            let host_depth = match case["id"].as_str().unwrap() {
                "host-depth-quote" => 3,
                _ => 2,
            };
            let padding = 200 - remaining as usize - host_depth;
            let nested = format!("{}{{^{{^x^}}^}}{}", "{/".repeat(padding), "/}".repeat(padding));
            let source = case["source"].as_str().unwrap().replace("{^{^x^}^}", &nested);
            let encoded = carve::to_json(&carve::parse(&source));
            let mut decoder = serde_json::Deserializer::from_str(&encoded);
            decoder.disable_recursion_limit();
            let mut actual = serde_json::Value::deserialize(&mut decoder).unwrap();
            assert!(unwrap_padding(&mut actual, padding), "{}", case["id"]);
            if case["id"] == "host-depth-heading" { actual["children"][0].as_object_mut().unwrap().remove("attrs"); }
            let mut expected = case["document"].clone();
            normalize_semantics(&mut actual);
            normalize_semantics(&mut expected);
            assert_eq!(actual, expected, "{}", case["id"]);
        }
    }).unwrap().join().unwrap();
}

#[test]
fn combined_bold_italic_preserves_repeated_constituents() {
    for source in ["{*a /*b*/ c*}", "{/a /*b*/ c/}", "/*a {*b*} c*/", "/*a {/b/} c*/"] {
        let expected = carve::to_html(source);
        let written = carve::render_carve(&carve::parse(source)).unwrap();
        assert_eq!(carve::to_html(&written), expected, "{}: {}", source, written);
        assert_eq!(carve::to_carve(&written), written, "{}", source);
    }
}

#[test]
fn imports_preserve_repeated_emphasis() {
    for html in ["<p><strong>a <strong>b</strong> c</strong></p>", "<p><sup>a <sup>b</sup> c</sup></p>"] {
        let imported = carve::html_to_carve(html, &carve::HtmlImportOptions::default()).unwrap();
        assert_eq!(carve::to_html(&imported.value).trim(), html);
        assert!(imported.report.diagnostics.is_empty());
    }
    for (source, html) in [
        ("*(*word*)*", "<p><em>(<em>word</em>)</em></p>"),
        ("__one __two__ three__", "<p><strong>one <strong>two</strong> three</strong></p>"),
    ] {
        assert_eq!(carve::to_html(&carve::markdown_to_carve(source)).trim(), html);
    }
    for (source, html) in [
        ("**bold**", "<p><strong><strong>bold</strong></strong></p>"),
        ("~~sub~~", "<p><sub><sub>sub</sub></sub></p>"),
    ] {
        assert_eq!(carve::to_html(&carve::djot_to_carve(source)).trim(), html);
    }
}

#[test]
fn attribute_comment_boundaries() {
    let vectors: Vec<Case> = serde_json::from_str(include_str!("fixtures/attribute-comment-boundaries.json")).unwrap();
    for case in vectors {
        assert_eq!(carve::to_html(&case.source).trim(), case.html, "{}", case.id);
        let written = carve::render_carve(&carve::parse(&case.source)).unwrap();
        assert_eq!(carve::to_html(&written).trim(), case.html, "{}: {}", case.id, written);
    }
}

#[test]
fn native_writer_refuses_an_over_budget_api_tree() {
    std::thread::Builder::new().stack_size(32 * 1024 * 1024).spawn(|| {
        let mut document = carve::parse("x");
        let carve::ast::BlockNode::Paragraph(paragraph) = &mut document.children[0] else { panic!("paragraph"); };
        let mut children = std::mem::take(&mut paragraph.children);
        for _ in 0..199 {
            children = vec![carve::ast::InlineNode::Emphasis(carve::ast::Emphasis {
                attrs: None, kind: carve::ast::EmphasisKind::Strong, children, pos: None,
            })];
        }
        paragraph.children = children;
        let error = carve::render_carve(&document).unwrap_err();
        assert!(error.to_string().contains("native parser nesting limit"), "{error}");
    }).unwrap().join().unwrap();
}


#[test]
fn raw_code_and_autolink_attribute_tails_are_opaque() {
    for source in ["{*a `x`{=html}{#id} b*} {# n #}", "{*a <https://a.b>{#id} b*} {#n#}"] {
        let html = carve::to_html(source);
        assert!(html.starts_with("<p><strong>a "), "{html}");
        let written = carve::render_carve(&carve::parse(source)).unwrap();
        assert_eq!(carve::to_html(&written), html, "{written}");
    }
}

#[test]
fn djot_host_depth_is_reported_before_native_syntax_becomes_literal() {
    std::thread::Builder::new().stack_size(32 * 1024 * 1024).spawn(|| {
    for host in ["link", "quote"] {
        let nested = "{*".repeat(198) + "x" + &"*}".repeat(198);
        let source = if host == "link" { format!("[{nested}](/u)") } else { format!("> {nested}") };
        let result = carve::migrate_djot(&source);
        let html = carve::to_html(&result.value);
        assert!(!html.contains("{*"), "{host}: {html}");
        assert!(result.report.diagnostics.iter().any(|item| item.code == "structure-unspellable"), "{host}");
    }
    }).unwrap().join().unwrap();
}


#[test]
fn glued_editorial_comment_cannot_become_an_attribute() {
    let mut document = carve::parse("/y/ {#a}b#}");
    let carve::ast::BlockNode::Paragraph(paragraph) = &mut document.children[0] else { panic!("paragraph"); };
    paragraph.children.retain(|node| !matches!(node, carve::ast::InlineNode::Text(_)));
    let error = carve::render_carve(&document).unwrap_err();
    assert!(error.to_string().contains("glued editorial comment"));
}
