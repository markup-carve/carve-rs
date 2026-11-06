#[test]
fn invalid_bare_include_with_apostrophe_is_literal() {
    let source = "{{ it's_x @lines:0-1 }} it's_y }}\n";
    assert_eq!(
        carve::to_html(source),
        "<p>{{ it's_x @lines:0-1 }} it’s_y }}</p>"
    );
}

#[test]
fn quote_bytes_in_bare_paths_do_not_hide_the_closer() {
    for n in 0..1000 {
        let quote = if n % 2 == 0 { "'" } else { "\"" };
        let prefix = " ".repeat(n % 3);
        let path = format!("file{quote}{n}.crv");
        let source = format!("{prefix}{{{{ {path} @lines:0-1 }}}} it's_y }}\n");
        let expected = format!("<p>{{{{ {path} @lines:0-1 }}}} it’s_y }}</p>");
        assert_eq!(carve::to_html(&source), expected, "variant {n}");
    }
}
