#[test]
fn first_block_plus_carries_attributes_past_a_note_to_its_fence() {
    let source = "- +\n{x}\n[^n]: x\n```\n\n```\n";
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li>\n    <pre x=\"\"><code>\n</code></pre>\n  </li>\n</ul>"
    );
}

#[test]
fn a_separate_plus_carries_attributes_past_invisible_lines_to_a_quote() {
    let source = "- a\n+\n{.x}\n%% hidden\n[^n]: text\n> q\n";
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li>a\n    <blockquote class=\"x\"><p>q</p></blockquote>\n  </li>\n</ul>"
    );
}
