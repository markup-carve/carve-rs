use carve::to_html;

#[test]
fn every_column_resumes_the_innermost_open_paragraph() {
    for middle in ["%", "%%"] {
        for column in 0..=5 {
            let source = format!("* * u\n{middle}\n{}:\n", " ".repeat(column));
            let html = to_html(&source);
            let inner = if middle == "%" {
                "u\n%\n:".to_string()
            } else {
                "u\n        :\n      ".to_string()
            };
            assert_eq!(
                html,
                format!(
                    "<ul>\n  <li>\n    <ul>\n      <li>{inner}</li>\n    </ul>\n  </li>\n</ul>"
                ),
                "middle {middle:?}, column {column}: {source:?}",
            );
        }
    }
}

#[test]
fn a_blank_still_closes_the_open_paragraph() {
    let html = to_html("* * u\n%\n\n :\n");
    assert!(html.contains("<li>u\n%</li>"), "{html}");
    assert!(!html.contains("%\n:</li>"), "{html}");
}

#[test]
fn a_nested_marker_line_definition_leaves_no_paragraph_open() {
    let expected = [
        "<ul>\n  <li>\n    <ul>\n      <li></li>\n    </ul>\n  </li>\n</ul>\n<p>:</p>",
        "<ul>\n  <li>\n    <ul>\n      <li></li>\n    </ul>\n  </li>\n</ul>\n<p>:</p>",
        "<ul>\n  <li>\n    <ul>\n      <li></li>\n    </ul>\n    :\n  </li>\n</ul>",
        "<ul>\n  <li>\n    <ul>\n      <li></li>\n    </ul>\n    :\n  </li>\n</ul>",
        "<ul>\n  <li>\n    <ul>\n      <li>:</li>\n    </ul>\n  </li>\n</ul>",
    ];
    for (column, expected) in expected.into_iter().enumerate() {
        let source = format!("* * [d]: u\n{}:\n", " ".repeat(column));
        assert_eq!(to_html(&source), expected, "column {column}: {source:?}");
    }
}

#[test]
fn a_comment_does_not_reopen_a_nested_definition_only_item() {
    assert_eq!(
        to_html("* * [d]: u\n%%\n:\n"),
        "<ul>\n  <li>\n    <ul>\n      <li></li>\n    </ul>\n  </li>\n</ul>\n<p>:</p>"
    );
}
