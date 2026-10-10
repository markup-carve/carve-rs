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
}

#[test]
fn shared_nested_braced_emphasis_vectors() {
    let vectors: Cases = serde_json::from_str(include_str!("fixtures/nested-braced-emphasis.json")).unwrap();
    let mut failures = Vec::new();
    for case in vectors.cases {
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
