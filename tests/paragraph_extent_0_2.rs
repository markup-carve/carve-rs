fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn block_openers_need_block_position() {
    for opener in [
        "# Heading",
        "> quoted",
        "---",
        "| a |",
        "```\ncode\n```",
        "::: note\nbody\n:::",
        ":: term\n:  definition",
        "[r]: /url",
        "%% hidden",
        "{.class}",
    ] {
        assert!(html(&format!("intro\n{opener}")).starts_with("<p>intro\n"));
        assert!(!html(&format!("intro\n\n{opener}")).starts_with("<p>intro\n"));
    }
}

#[test]
fn rule_applies_in_quotes_and_items() {
    assert!(html("> intro\n> # Heading").contains("<p>intro\n# Heading</p>"));
    assert!(html("- intro\n  # Heading").contains("<li>intro\n# Heading</li>"));
}

#[test]
fn tight_nested_list_is_structural() {
    assert!(html("- intro\n  - nested").contains("<ul>\n      <li>nested</li>"));
}
