use carve2::lint::{LintOptions, lint};
use serde::Deserialize as _;
use serde_json::Value;

#[test]
fn catalog_message_variants_follow_their_clause_triggers() {
    let mut parser =
        serde_json::Deserializer::from_str(include_str!("../spec-conformance/clause-cases.json"));
    parser.disable_recursion_limit();
    let corpus = Value::deserialize(&mut parser).expect("clause cases");
    let cases = [
        (
            "CC-1118",
            "fence-title-syntax",
            "These further words are not read as a title",
        ),
        (
            "CC-1119",
            "list-marker-without-blank-line",
            "This list marker directly follows term or description text",
        ),
        (
            "CC-1140",
            "fence-title-syntax",
            "These words are not part of the anonymous div opener",
        ),
        (
            "CC-1146",
            "unattached-block-attribute",
            "A blank line follows this attribute line",
        ),
        (
            "CC-1153",
            "broken-fragment-link",
            "Link to \"#%61b\" matches no id; the id \"Ab\"",
        ),
        (
            "CC-1154",
            "fence-title-syntax",
            "These further words are not read as a title",
        ),
        (
            "CC-1171",
            "fence-title-syntax",
            "The whitespace in this fence opener",
        ),
        (
            "CC-1173",
            "region-marker-renders-as-text",
            "This line is a comment and renders nothing",
        ),
    ];
    for (id, code, prefix) in cases {
        let case = corpus["cases"]
            .as_array()
            .expect("cases")
            .iter()
            .find(|case| case["id"] == id)
            .expect("case");
        let source = case["input"].as_str().expect("source");
        let findings = lint(source, &LintOptions::default());
        assert!(
            findings.iter().any(|finding| finding.code == code
                && finding.message.starts_with(prefix)
                && !finding.message.contains("${")),
            "{id}: {findings:?}"
        );
    }
}

#[test]
fn c2_w_107_percent_decoded_exact_fragment_is_not_broken() {
    let findings = lint("{#Ab}\n# H\n\n[x](#%41b)\n", &LintOptions::default());
    assert!(
        !findings
            .iter()
            .any(|finding| finding.code == "broken-fragment-link"),
        "{findings:?}"
    );
}
