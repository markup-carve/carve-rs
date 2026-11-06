#[test]
fn authored_table_caption_expands_abbreviations_but_generated_reference_text_does_not() {
    let input = "*[A]: x\n\n{#t}\n| a |\n^ A #: c\n\nSee </#t>.\n";
    let html = carve::to_html(input);
    assert!(
        html.contains("<caption><abbr title=\"x\">A</abbr> 1: c</caption>"),
        "{html}"
    );
    assert!(html.contains("<a href=\"#t\">A 1</a>"), "{html}");
}

#[test]
fn caption_abbreviation_expands_across_words_and_table_containers() {
    for n in 0..1000 {
        let word = format!("Term{n}");
        let indent = " ".repeat(n % 3);
        let source =
            format!("*[{word}]: expansion\n\n{indent}| cell |\n{indent}^ {word} caption\n");
        let html = carve::to_html(&source);
        assert!(
            html.contains(&format!("<abbr title=\"expansion\">{word}</abbr> caption")),
            "variant {n}: {html}"
        );
    }
}
