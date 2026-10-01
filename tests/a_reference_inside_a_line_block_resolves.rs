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

#[test]
fn quoted_verse_keeps_its_definitions_and_registers_neither_links_nor_notes() {
    for prefix in ["> ", "> > "] {
        let source = format!("{prefix}::: |\n{prefix}[r]: /hidden\n{prefix}[^n]: note\n{prefix}see [t][r]\n{prefix}::: \n\n[t][r] [^n]\n");
        let html = carve::to_html(&source);
        assert!(html.contains("[r]: /hidden"), "{source:?}: {html}");
        assert!(html.contains("[^n]: note"), "{source:?}: {html}");
        assert!(!html.contains("href=\"/hidden\""), "{source:?}: {html}");
        assert!(!html.contains("doc-endnotes"), "{source:?}: {html}");
    }
}

#[test]
fn a_quoted_verse_closer_cannot_be_taken_from_a_deeper_quote() {
    let source = "> ::: |\n> > :::\n> [r]: /hidden\n> :::\n\n[t][r]\n";
    let html = carve::to_html(source);
    assert!(html.contains("[r]: /hidden"), "{html}");
    assert!(!html.contains("href=\"/hidden\""), "{html}");
}

#[test]
fn an_unterminated_quoted_verse_does_not_hide_a_later_document_definition() {
    let source = "> ::: |\n> verse\n\n[r]: /target\n\n[t][r]\n";
    let html = carve::to_html(source);
    assert!(html.contains("href=\"/target\""), "{html}");
}

#[test]
fn lazy_verse_lines_do_not_expose_later_quoted_definitions() {
    for continuation in ["lazy", "  > :::"] {
        let source = format!("> ::: |\n> verse\n{continuation}\n> [r]: /hidden\n> [^n]: note\n> :::\n\n[t][r] [^n]\n");
        let html = carve::to_html(&source);
        assert!(html.contains("[r]: /hidden"), "{source:?}: {html}");
        assert!(html.contains("[^n]: note"), "{source:?}: {html}");
        assert!(!html.contains("href=\"/hidden\""), "{source:?}: {html}");
        assert!(!html.contains("doc-endnotes"), "{source:?}: {html}");
    }
}

#[test]
fn a_code_fence_in_quoted_verse_prevents_lazy_quote_continuation() {
    let source = "> ::: |\n> ```\n> verse\nlazy\n> [r]: /target\n\n[t][r]\n";
    let html = carve::to_html(source);
    assert!(html.contains("href=\"/target\""), "{html}");
}

#[test]
fn an_unclosed_fence_after_verse_prose_keeps_the_lazy_quote_open() {
    let source = "> ::: |\n> verse\n> ```\nlazy\n> [r]: /hidden\n> :::\n\n[t][r]\n";
    let html = carve::to_html(source);
    assert!(html.contains("[r]: /hidden"), "{html}");
    assert!(!html.contains("href=\"/hidden\""), "{html}");
}

#[test]
fn attached_verse_lines_do_not_expose_later_quoted_definitions() {
    for marker in ["+", "+ "] {
        for attached in ["attached", "# Heading", "---", "{.k}"] {
            let source = format!("> ::: |\n{marker}\n{attached}\n> [r]: /hidden\n> [^n]: note\n> :::\n\n[t][r] [^n]\n");
            let html = carve::to_html(&source);
            assert!(html.contains("[r]: /hidden"), "{source:?}: {html}");
            assert!(!html.contains("href=\"/hidden\""), "{source:?}: {html}");
            assert!(!html.contains("doc-endnotes"), "{source:?}: {html}");
        }
    }
}

#[test]
fn wrapped_attributes_in_verse_end_lazy_quote_continuation() {
    let source = "> ::: |\n> {.k\n> #x}\nlazy\n> [r]: /target\n\n[t][r]\n";
    let html = carve::to_html(source);
    assert!(html.contains("href=\"/target\""), "{html}");
}

#[test]
fn a_table_continuation_in_verse_prevents_lazy_quote_continuation() {
    let source = "> ::: |\n> | a |\n> + b |\nlazy\n> [r]: /target\n> :::\n\n[t][r]\n";
    let html = carve::to_html(source);
    assert!(html.contains("href=\"/target\""), "{html}");
}

#[test]
fn an_outer_attachment_ends_verse_inside_a_nested_quote() {
    let source = "> > ::: |\n> > verse\n+\nattached\n> > [r]: /target\n> > :::\n\n[t][r]\n";
    let html = carve::to_html(source);
    assert!(html.contains("href=\"/target\""), "{html}");
}

#[test]
fn an_attachment_keeps_exactly_one_block_in_quoted_verse() {
    let source = "> ::: |\n> verse\n+\n---\nafter\n> [r]: /target\n\n[t][r]\n";
    let html = carve::to_html(source);
    assert!(html.contains("href=\"/target\""), "{html}");
}

#[test]
fn an_attached_colon_fence_closes_quoted_verse() {
    let source = "> ::: |\n> verse\n+\n:::\n> [r]: /target\n\n[t][r]\n";
    let html = carve::to_html(source);
    assert!(html.contains("href=\"/target\""), "{html}");
}

#[test]
fn a_document_abbreviation_ends_lazy_quoted_verse() {
    let source = "> ::: |\n> verse\n*[HTML]: Hyper Text\n> [r]: /target\n\n[t][r]\n";
    let html = carve::to_html(source);
    assert!(html.contains("href=\"/target\""), "{html}");
}

#[test]
fn an_attachment_splits_quoted_verse_when_a_code_fence_is_open() {
    let source = "> ::: |\n> ```\n+\nattached\n> [r]: /target\n> :::\n\n[t][r]\n";
    let html = carve::to_html(source);
    assert!(html.contains("href=\"/target\""), "{html}");
}

#[test]
fn an_attachment_clears_the_previous_quoted_table_run() {
    let source = "> ::: |\n> | a |\n+\nx\n> + b |\nlazy\n> [r]: /hidden\n> :::\n\n[t][r]\n";
    let html = carve::to_html(source);
    assert!(html.contains("[r]: /hidden"), "{html}");
    assert!(!html.contains("href=\"/hidden\""), "{html}");
}
