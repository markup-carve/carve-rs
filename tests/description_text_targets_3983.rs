#[test]
fn description_leading_tab_is_display_whitespace_in_text_targets() {
    let source = ":: t\n: \ttext\n\nflush\n";
    assert_eq!(carve::to_plain_text(source), "t\n  text\n\nflush\n");
    assert!(carve::to_html(source).contains("<dd>\ttext</dd>"));
    let ansi = carve::to_ansi(source);
    assert!(!ansi.contains('\t'), "{ansi:?}");
}
