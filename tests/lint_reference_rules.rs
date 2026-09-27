use carve::lint_carve;

fn has(source: &str, rule: &str) -> bool {
    lint_carve(source)
        .iter()
        .any(|warning| warning.rule == rule)
}

#[test]
fn unresolved_references_and_heading_collisions_are_reported() {
    for (source, rule) in [
        ("# A\n\n# A\n", "duplicate-heading-id"),
        ("see </#nope>\n", "broken-crossref"),
        ("see [text][nope]\n", "unresolved-reference-link"),
        ("see[^nope]\n", "unresolved-footnote"),
        ("text\n\n[^a]: never used\n", "unused-footnote-definition"),
        (
            "x[^a]\n\n[^a]: one\n\n[^a]: two\n",
            "duplicate-footnote-definition",
        ),
        (
            "see [^a b]\n\n[^a b]: one\n\n[^a  b]: two\n",
            "footnote-labels-differ-only-in-whitespace",
        ),
    ] {
        assert!(has(source, rule), "{rule}: {source:?}");
    }
}

#[test]
fn resolved_references_are_not_reported() {
    let source =
        "# Target\n\nsee </#Target> [Target][] [text][r] and [^a b]\n\n[r]: u\n\n[^a  b]: note\n";
    let warnings = lint_carve(source);
    for rule in [
        "broken-crossref",
        "unresolved-reference-link",
        "unresolved-footnote",
        "unused-footnote-definition",
    ] {
        assert!(
            !warnings.iter().any(|w| w.rule == rule),
            "{rule}: {warnings:?}"
        );
    }
}

#[test]
fn definition_examples_in_code_comments_and_literal_prose_are_not_duplicates() {
    for body in [
        "```\n[^a]: sample\n```",
        "%%%\n[^a]: sample\n%%%",
        "\\[^a]: sample",
        "`[^a]: sample`",
    ] {
        let source = format!("x[^a]\n\n[^a]: note\n\n{body}\n");
        assert!(!has(&source, "duplicate-footnote-definition"), "{source}");
    }
}

#[test]
fn nested_references_and_original_unicode_byte_offsets_are_checked() {
    let source = "😀\r\n\r\n> see </#missing>\r\n";
    let warning = lint_carve(source)
        .into_iter()
        .find(|w| w.rule == "broken-crossref")
        .unwrap();
    assert_eq!(warning.line, 3);
    assert_eq!(&source[warning.start..warning.end], "</#missing>");
    assert!(has("[^a]: see[^missing]\n", "unresolved-footnote"));
}

#[test]
fn repeated_definitions_inside_quotes_keep_the_authored_location() {
    let source = "x[^a]\r\n\r\n> [^a]: one\r\n\r\n> [^a]: two\r\n";
    let warning = lint_carve(source)
        .into_iter()
        .find(|w| w.rule == "duplicate-footnote-definition")
        .unwrap();
    assert_eq!(warning.line, 5);
    assert_eq!(warning.column, 3);
    assert_eq!(&source[warning.start..warning.end], "[^a]:");
}

#[test]
fn reference_checks_use_the_renderers_heading_id_options() {
    let options = carve::Options::default().with_lowercase_heading_ids(true);
    let warnings =
        carve::lint_carve_with_options("# TARGET\n\nsee </#target> [TARGET][]\n", &options);
    assert!(
        !warnings
            .iter()
            .any(|w| matches!(w.rule, "broken-crossref" | "unresolved-reference-link")),
        "{warnings:?}"
    );
}
