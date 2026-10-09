//! A fence's title can come from the header or from an attribute line above it,
//! and the attribute line WINS. The HTML target uses the winner, so emitting the
//! authored header in the Markdown info string described the same document
//! differently in the two targets -- announcing a title that had lost
//! (carve#352, corpus 11-fenced-code-10).
//!
//! The parser already resolves the override into `attrs`, so no new information is
//! needed.

fn info_string(src: &str) -> String {
    carve::to_markdown(src)
        .lines()
        .next()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn a_header_title_is_kept() {
    // With a language in front of it: a title alone is dropped, because in
    // Markdown the first token of the info string IS the language. See
    // markdown_title_needs_language.rs.
    assert_eq!(
        info_string("```php \"notes.php\"\nremember the milk\n```\n"),
        "```php \"notes.php\""
    );
}

#[test]
fn an_attribute_line_beats_the_header() {
    let src = "{title=\"from the attribute line\"}\n```php \"from the header\"\ncode\n```\n";
    assert_eq!(info_string(src), "```php \"from the attribute line\"");
}

#[test]
fn it_agrees_with_what_the_html_target_says_the_title_is() {
    let src = "{title=\"from the attribute line\"}\n```php \"from the header\"\ncode\n```\n";
    assert!(carve::to_html(src).contains("title=\"from the attribute line\""));
    assert!(info_string(src).contains("from the attribute line"));
}

#[test]
fn no_title_emits_none() {
    assert_eq!(info_string("```php\ncode\n```\n"), "```php");
}

#[test]
fn a_grouping_label_rides_along() {
    assert_eq!(
        info_string("```php \"f.php\" [Build]\ncode\n```\n"),
        "```php \"f.php\" [Build]"
    );
}

#[test]
fn a_backtick_title_uses_a_tilde_fence_longer_than_the_payload() {
    let source = "```php \"src/`Auth.php\"\n~~~\n$ok = true;\n```\n";
    let markdown = carve::to_markdown(source);
    assert!(markdown.starts_with("~~~~php \"src/`Auth.php\"\n"));
    let events: Vec<_> = pulldown_cmark::Parser::new(&markdown).collect();
    assert!(events.iter().any(|event| matches!(event, pulldown_cmark::Event::Start(pulldown_cmark::Tag::CodeBlock(pulldown_cmark::CodeBlockKind::Fenced(info))) if info.as_ref() == "php \"src/`Auth.php\"")));
    let payload: String = events
        .iter()
        .filter_map(|event| match event {
            pulldown_cmark::Event::Text(text) => Some(text.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(payload, "~~~\n$ok = true;\n");
}
