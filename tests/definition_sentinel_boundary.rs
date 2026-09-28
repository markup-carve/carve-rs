use std::collections::BTreeSet;

const REPRODUCERS: &[(&str, &str)] = &[
    (
        "c2089__fn-body-code-then-def",
        "[^a]: body\n\n    code\n\n[^b]: other\n\nx[^a] y[^b]\n",
    ),
    (
        "c2089__fn-body-fence-then-def",
        "[^a]: body\n\n   ```\n   a\n```\n\n[^b]: other\n\nx[^a] y[^b]\n",
    ),
    (
        "c2089__fn-body-then-def",
        "[^a]: body\n\n[^b]: other\n\nx[^a] y[^b]\n",
    ),
    (
        "sentinel_link_definition",
        "[^a]: body\n\n   ```\n   a\n```\n\n[b]: /u\n\nx[^a] y[^b]\n",
    ),
];

#[test]
fn sentinel_reproducers_match_oracle() {
    assert_eq!(carve::to_html("[^a]: body\n\n    code\n\n[^b]: other\n\nx[^a] y[^b]\n"), "<p>x<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> y<a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>body</p>\n      <p>code<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n    <li id=\"fn2\">\n      <p>other<a href=\"#fnref2\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>");
    assert_eq!(
        carve::to_html("[^a]: body\n\n   ```\n   a\n```\n\n[^b]: other\n\nx[^a] y[^b]\n"),
        "<pre><code>\n[^b]: other\n\nx[^a] y[^b]\n</code></pre>"
    );
    assert_eq!(carve::to_html("[^a]: body\n\n[^b]: other\n\nx[^a] y[^b]\n"), "<p>x<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> y<a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>body<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n    <li id=\"fn2\">\n      <p>other<a href=\"#fnref2\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>");
    assert_eq!(
        carve::to_html("[^a]: body\n\n   ```\n   a\n```\n\n[b]: /u\n\nx[^a] y[^b]\n"),
        "<pre><code>\n[b]: /u\n\nx[^a] y[^b]\n</code></pre>"
    );
}

fn pua(text: &str) -> BTreeSet<char> {
    text.chars()
        .filter(|ch| ('\u{e000}'..='\u{f8ff}').contains(ch))
        .collect()
}

#[test]
fn outputs_never_introduce_private_use_codepoints() {
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus");
    let mut sources: Vec<(String, String)> = std::fs::read_dir(corpus)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "crv"))
        .map(|path| {
            (
                path.display().to_string(),
                std::fs::read_to_string(path).unwrap(),
            )
        })
        .collect();
    assert!(!sources.is_empty());
    sources.extend(
        REPRODUCERS
            .iter()
            .map(|(name, source)| (name.to_string(), source.to_string())),
    );
    sources.push((
        "authored_pua".into(),
        "authored \u{e000} \u{e006} \u{f8ff}\n".into(),
    ));
    for (name, source) in sources {
        let allowed = pua(&source);
        let doc = carve::parse(&source);
        let outputs = [
            ("html", carve::render_html(&doc).unwrap()),
            ("json", carve::to_json(&doc)),
            ("markdown", carve::render_markdown(&doc).unwrap()),
            ("plain", carve::render_plain_text(&doc).unwrap()),
            ("carve", carve::render_carve(&doc).unwrap()),
        ];
        for (target, output) in outputs {
            let introduced: Vec<_> = pua(&output).difference(&allowed).copied().collect();
            assert!(
                introduced.is_empty(),
                "{name} {target}: introduced {introduced:?}"
            );
        }
    }
}
