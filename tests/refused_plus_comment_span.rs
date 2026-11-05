//! A `%%%` span after a refused own-line `+` closes no item (carve#5170).

fn assert_rows(rows: &[(&str, &str)]) {
    let whitespace = regex::Regex::new(r">\s+<").unwrap();
    for (source, html) in rows {
        for output in [
            carve::to_html(source),
            carve::render_html(&carve::parse(source)).unwrap(),
        ] {
            assert_eq!(
                whitespace.replace_all(&output, "><").trim(),
                *html,
                "{source:?}"
            );
        }
    }
}

#[test]
fn a_span_after_a_refused_own_line_plus_keeps_the_next_line_in_the_host() {
    assert_rows(&[
        (
            "- a\n+\n%%%\nx\n%%%\n  text\n",
            "<ul><li>a\n    text\n  </li></ul>",
        ),
        (
            "- a\n+\n%%%\nx\n%%%\n%%%\ny\n%%%\n  text\n",
            "<ul><li>a\n    text\n  </li></ul>",
        ),
        (
            "- a\n+\n%%%%\nx\n%%%%\n  text\n",
            "<ul><li>a\n    text\n  </li></ul>",
        ),
        (
            "- a\n+\n%%%\nx\n%%%\n\n  text\n",
            "<ul><li><p>a</p><p>text</p></li></ul>",
        ),
        (
            "1. a\n+\n%%%\nx\n%%%\n   text\n",
            "<ol><li>a\n    text\n  </li></ol>",
        ),
        (
            ":: t\n: a\n+\n%%%\nx\n%%%\n  text\n",
            "<dl><dt>t</dt><dd><p>a</p><p>text</p></dd></dl>",
        ),
    ]);
}

#[test]
fn the_span_reads_as_its_line_comment_spelling() {
    for host in [
        "- a\n+\n",
        "1. a\n+\n",
        "- first\n  more\n+\n",
        ":: t\n: a\n+\n",
    ] {
        let pad = if host.starts_with("1.") { "   " } else { "  " };
        let span = format!("{host}%%%\nx\n%%%\n{pad}text\n");
        let line = format!("{host}%% c\n{pad}text\n");
        assert_eq!(carve::to_html(&span), carve::to_html(&line), "{span:?}");
    }
}

#[test]
fn an_attribute_run_after_a_refused_plus_still_stands_at_the_marker_column() {
    assert_rows(&[
        (
            "- a\n+\n{x}\n  - c\n",
            "<ul><li>a</li></ul><ul x=\"\"><li>c</li></ul>",
        ),
        (
            "- a\n+\n{x}\n- c\n",
            "<ul><li>a</li></ul><ul x=\"\"><li>c</li></ul>",
        ),
    ]);
}
