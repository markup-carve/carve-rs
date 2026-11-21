//! Rules the 0.1 lint pass used to supply, now read off the 0.2 tree.

fn codes(input: &str) -> Vec<&'static str> {
    carve::lint::lint(input, &carve::lint::LintOptions::default())
        .into_iter()
        .map(|finding| finding.code)
        .collect()
}

#[test]
fn ported_rules_still_report() {
    for (input, code) in [
        ("%% #rEgion a\n", "region-marker-renders-as-text"),
        ("// #region demo\n", "region-marker-renders-as-text"),
        ("{% if x %}\n", "braced-comment-in-a-template-source"),
        ("{tabel}\n```csv\na,b\n```\n", "consumed-key-near-miss"),
        ("{table}\n```csv\na\nb,c\n```\n", "delimited-table-wide-row"),
        (
            "::: figure\n:::: figure\n::::\n:::\n",
            "figure-group-nested",
        ),
        ("::: figure\n| a |\n:::\n", "figure-group-single-panel"),
        (
            "::: figure\n![a](a.png)\n^ A #\n:::\n",
            "figure-group-panel-number",
        ),
        (
            "[^a b]: x\n[^a  b]: y\n",
            "footnote-labels-differ-only-in-whitespace",
        ),
        ("*.c,*.h\n", "glob-shaped-strong"),
        ("<!-- a\nb -->\n", "html-comment-renders-as-text"),
        ("- a\n  - b\n - c\n", "list-marker-below-content-column"),
        ("{aligns=left}\n| a | b |\n", "table-column-arity"),
        ("{aligns=left}\n|={>} A |\n| a |\n", "table-column-overlap"),
        ("{widths=60,50}\n| a | b |\n", "table-width-total"),
        ("|{.x}> a |\n", "table-marker-run-padding"),
        ("|a|<|\n|^|b|\n", "table-span-marker-blocked"),
        ("<a@b> and <not a link>\n", "bare-html-tag"),
    ] {
        assert!(codes(input).contains(&code), "{code} on {input:?}");
    }
}

#[test]
fn ported_rules_stay_quiet() {
    for (input, code) in [
        ("%% #region a\n", "region-marker-renders-as-text"),
        ("\\<!--x-->\n", "html-comment-renders-as-text"),
        ("- a\n    - b\n  - c\n", "list-marker-below-content-column"),
        (
            "{valigns=\"middle\"}\n| a |\n|{?v style=\"color: red;\"} b |\n",
            "table-column-overlap",
        ),
    ] {
        assert!(!codes(input).contains(&code), "{code} on {input:?}");
    }
}
