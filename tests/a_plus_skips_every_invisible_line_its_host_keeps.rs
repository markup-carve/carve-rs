//! carve#5130, CARVE-P9-067: after a `+`, every invisible line the host keeps
//! is skipped, at its marker column or at or past its content column.

fn html(source: &str) -> String {
    regex::Regex::new(r">\s+<")
        .unwrap()
        .replace_all(&carve::to_html(source), "><")
        .trim()
        .to_string()
}

#[test]
fn attaches_past_lines_kept_at_the_content_column() {
    for (source, expected) in [
        ("- +\n  %%%\n  c\n  %%%\ntext\n", "<ul><li>text</li></ul>"),
        ("- +\n  {x}\ntext\n", "<ul><li><p x=\"\">text</p></li></ul>"),
        (
            "1. +\n   %%%\n   c\n   %%%\ntext\n",
            "<ol><li>text</li></ol>",
        ),
        (
            ":: t\n: +\n  %%%\n  c\n  %%%\ntext\n",
            "<dl><dt>t</dt><dd>text</dd></dl>",
        ),
        (
            "- +\n  %%%\n  c\n  %%%\n  {x}\ntext\n",
            "<ul><li><p x=\"\">text</p></li></ul>",
        ),
    ] {
        assert_eq!(html(source), expected, "{source:?}");
    }
}

#[test]
fn a_refused_plus_moves_no_kept_line() {
    for (source, expected) in [
        ("- +\n  %%%\n  c\n  %%%\n  text\n", "<ul><li>text</li></ul>"),
        ("- +\n {x}\ntext\n", "<ul><li></li></ul><p x=\"\">text</p>"),
        (
            "- +\n  %%%\n c\n  %%%\ntext\n",
            "<ul><li></li></ul><p>c</p>",
        ),
    ] {
        assert_eq!(html(source), expected, "{source:?}");
    }
}

#[test]
fn a_kept_span_ends_at_a_line_below_the_content_column() {
    // The outdented line ends the span and is the attached block; the later
    // `%%%` opens a span of its own (CARVE-P0-005).
    assert_eq!(
        html("- a\n+\n  %%%\nouter\n%%%\n> q\n"),
        "<ul><li>a\n    outer\n  </li></ul>"
    );
    // An unclosed kept span ends there too.
    assert_eq!(
        html("- a\n+\n  %%%\n> q\n"),
        "<ul><li>a\n    <blockquote><p>q</p></blockquote></li></ul>"
    );
}

#[test]
fn an_unclosed_span_at_the_marker_column_runs_to_the_end() {
    assert_eq!(html("[^n]: +\n%%%\n> q\n\nr[^n]\n"), "");
}
