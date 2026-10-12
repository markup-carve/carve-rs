use serde_json::Value;

#[test]
fn pending_definition_terms_keep_unclosed_code_tails() {
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/pending_term_code_lazy_tails.json")).unwrap();
    let cases = cases.as_array().unwrap();
    assert_eq!(cases.len(), 732);
    let whitespace = regex::Regex::new(r">\s+<").unwrap();
    let payload_pattern = regex::Regex::new(r"(?s)<pre><code[^>]*>(.*?)</code></pre>").unwrap();
    for case in cases {
        let source = case["source"].as_str().unwrap();
        let expected = case["html"].as_str().unwrap().replace("&nbsp;", "\u{a0}");
        for actual in [
            carve::to_html(source),
            carve::render_html(&carve::parse(source)).unwrap(),
            carve::render_html(&carve::parse_with_options(
                source,
                &carve::Options {
                    positions: true,
                    ..Default::default()
                },
            ))
            .unwrap(),
        ] {
            let normalized = whitespace
                .replace_all(&actual, "><")
                .replace("&nbsp;", "\u{a0}");
            assert_eq!(normalized.trim(), expected, "{}: {source}", case["id"]);
            let payloads: Vec<_> = payload_pattern
                .captures_iter(&actual)
                .map(|capture| capture[1].replace("&nbsp;", "\u{a0}"))
                .collect();
            let expected_payloads: Vec<_> = case["codePayloads"]
                .as_array()
                .unwrap()
                .iter()
                .map(|payload| payload.as_str().unwrap().replace("&nbsp;", "\u{a0}"))
                .collect();
            assert_eq!(
                payloads, expected_payloads,
                "{} payload: {source}",
                case["id"]
            );
        }
    }
}
