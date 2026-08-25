fn indent(source: &str, width: usize) -> String {
    source
        .lines()
        .map(|line| format!("{}{line}", " ".repeat(width)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn footnote(body: &str, width: usize) -> String {
    format!("[^n]: intro\n\n{}\n\nsee[^n]\n", indent(body, width))
}

#[test]
fn a_fence_shaped_payload_is_not_rebased_as_a_second_block() {
    let body = "~~~~\n ```\n~~~~";
    let exact = carve::to_carve(&footnote(body, 2));
    let over = carve::to_carve(&footnote(body, 3));
    assert_eq!(over, exact);
    assert_eq!(carve::to_carve(&exact), exact);
    assert!(over.contains("\n  ````\n   ```\n  ````\n"), "{over}");
}

#[test]
fn a_fenced_quote_opens_at_an_authored_footnote_base() {
    let exact = footnote("::: >\n> quote\n:::", 2);
    let over = footnote("::: >\n> quote\n:::", 4);
    assert_eq!(carve::to_carve(&over), carve::to_carve(&exact));
    assert_eq!(carve::to_html(&over), carve::to_html(&exact));
    assert!(carve::to_html(&over).contains("<blockquote>"));
}
