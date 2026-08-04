//! Node kinds the published AST must carry (carve-rs#513).
//!
//! All three engines render byte-identical HTML for these, so nothing caught
//! them: the corpus pins HTML, and the divergence is AST-only. That surface is
//! exactly what carve-lsp, the pandoc bridge and any fmt-over-the-wire consumer
//! reads.

fn types(json: &str) -> Vec<String> {
    // Type names in document order, good enough to pin a shape.
    let mut out = Vec::new();
    let mut rest = json;
    while let Some(i) = rest.find("\"type\":\"") {
        rest = &rest[i + 8..];
        if let Some(j) = rest.find('"') {
            out.push(rest[..j].to_string());
        }
    }
    out
}

fn json(src: &str) -> String {
    carve::to_json_with_options(src, &carve::Options::default())
}

#[test]
fn the_combined_bold_italic_form_publishes_a_strong_wrapping_an_emphasis() {
    // PART 11 §6: `/*x*/` and `*/x/*` BOTH yield a strong wrapping an emphasis.
    // `boldItalic` records the authored spelling; it does not replace nesting.
    // This engine holds the combined form as one node internally, which is fine
    // - what it publishes has to be the two.
    let t = types(&json("/*bold italic*/\n"));
    assert_eq!(
        t,
        vec!["document", "paragraph", "strong", "emphasis", "text"]
    );
    assert!(json("/*bold italic*/\n").contains("boldItalic"));
}

#[test]
fn an_abbreviation_definition_is_a_document_child() {
    // PART 12 §7: an abbreviation_def is a child of the DOCUMENT, exactly as a
    // footnote is. It was hoisted here and then consumed once its expansion had
    // been harvested, so the published tree lost it.
    let t = types(&json("*[HTML]: Hyper Text\n\nThe HTML spec.\n"));
    assert_eq!(t.first().map(String::as_str), Some("document"));
    assert!(
        t.contains(&"abbreviation_def".to_string()),
        "abbreviation_def missing from {t:?}"
    );
}

#[test]
fn keeping_the_definition_does_not_change_the_html() {
    // It renders nothing, and must not leave a blank line where it stood.
    assert_eq!(
        carve::to_html("*[HTML]: Hyper Text\n\nThe HTML spec.\n"),
        "<p>The <abbr title=\"Hyper Text\">HTML</abbr> spec.</p>"
    );
}
