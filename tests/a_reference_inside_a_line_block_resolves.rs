fn sources(content: &str, definition: &str) -> Vec<String> {
    let verse = format!("::: |\n{content}\n:::");
    let quoted = verse
        .lines()
        .map(|line| format!("> {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    [verse, quoted]
        .into_iter()
        .flat_map(|body| {
            [
                format!("{body}\n\n{definition}\n"),
                format!("{definition}\n\n{body}\n"),
            ]
        })
        .collect()
}

#[test]
fn explicit_links_resolve_in_line_blocks_and_keep_definition_attributes() {
    for source in sources("before [link][r] after", "[r]: /target {.defined}") {
        let html = carve::to_html(&source);
        assert!(
            html.contains("<a href=\"/target\" class=\"defined\">link</a>"),
            "{source:?}: {html}"
        );
    }
}

#[test]
fn image_references_resolve_in_line_blocks() {
    for source in sources("before ![alt][r] after", "[r]: /image.png") {
        let html = carve::to_html(&source);
        assert!(
            html.contains("<img src=\"/image.png\" alt=\"alt\">"),
            "{source:?}: {html}"
        );
    }
}

#[test]
fn collapsed_references_in_line_blocks_reach_the_heading_index() {
    for source in sources("before [Heading][] after", "# Heading") {
        let html = carve::to_html(&source);
        assert!(
            html.contains("<a href=\"#Heading\">Heading</a>"),
            "{source:?}: {html}"
        );
    }
}

#[test]
fn missing_definitions_keep_the_reference_spelling_in_line_blocks() {
    for source in sources("before [link][missing] after", "[other]: /target") {
        let html = carve::to_html(&source);
        assert!(
            html.contains("before [link][missing] after"),
            "{source:?}: {html}"
        );
        assert!(!html.contains("href=\"/target\""), "{source:?}: {html}");
    }
}
