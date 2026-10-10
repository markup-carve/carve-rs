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
