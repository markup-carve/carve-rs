use carve::{djot_to_carve, to_html};

#[test]
fn official_heading_collision() {
    let source = "{#Foo-bar}\nParagraph\n\n# Foo bar\n\n## Foo  bar\n\n{#baz}\n# Foo bar\n";
    let html = to_html(&djot_to_carve(source));
    assert!(html.contains("<p id=\"Foo-bar\">Paragraph</p>"), "{html}");
    assert!(html.contains("<section id=\"Foo-bar-1\">"), "{html}");
    assert!(html.contains("<section id=\"Foo-bar-2\">"), "{html}");
    assert!(html.contains("<section id=\"baz\">"), "{html}");
}

#[test]
fn repeated_and_later_explicit_ids() {
    let html = to_html(&djot_to_carve("# A\n\n# A\n\n{#A}\np\n"));
    assert!(html.contains("<section id=\"A\">"), "{html}");
    assert!(html.contains("<section id=\"A-1\">"), "{html}");
    assert!(html.contains("<p id=\"A\">p</p>"), "{html}");
}

#[test]
fn quoted_heading_collision() {
    let html = to_html(&djot_to_carve("{#A}\np\n\n> # A\n"));
    assert!(html.contains("<h1 id=\"A-1\">A</h1>"), "{html}");
}

#[test]
fn list_heading_collision_stays_in_item() {
    let html = to_html(&djot_to_carve("{#A}\np\n\n- # A\n"));
    assert!(html.contains("<h1 id=\"A-1\">A</h1>"), "{html}");
    assert_eq!(html.matches("<li>").count(), 1, "{html}");
}

#[test]
fn unicode_before_heading_keeps_source_offset() {
    let html = to_html(&djot_to_carve("é\n\n{#A}\np\n\n# A\n"));
    assert!(html.contains("<section id=\"A-1\">"), "{html}");
}
