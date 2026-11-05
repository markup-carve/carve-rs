use carve::to_html;

#[test]
fn a_lazy_braced_comment_uses_written_columns_in_a_quote() {
    let cases = [
        (
            "> pre\n{% b\nb %}\n",
            "<blockquote><p>pre\n{% b\nb %}</p></blockquote>",
        ),
        (
            "> pre\n{% b\n b %}\n",
            "<blockquote><p>pre\n</p></blockquote>",
        ),
        (
            "> pre\n{% b\n> b %}\n",
            "<blockquote><p>pre\n{% b\nb %}</p></blockquote>",
        ),
        (
            "> pre\n{% b\n>  b %}\n",
            "<blockquote><p>pre\n</p></blockquote>",
        ),
        (
            "- > pre\n{% b\n b %}\n",
            "<ul>\n  <li>\n    <blockquote><p>pre\n</p></blockquote>\n  </li>\n</ul>",
        ),
        (
            "- > pre\n {% b\n b %}\n",
            "<ul>\n  <li>\n    <blockquote><p>pre\n{% b\nb %}</p></blockquote>\n  </li>\n</ul>",
        ),
        (
            "- > pre\n {% b\n  b %}\n",
            "<ul>\n  <li>\n    <blockquote><p>pre\n</p></blockquote>\n  </li>\n</ul>",
        ),
        (
            "- > pre\n{% b\n c\n d %}\n",
            "<ul>\n  <li>\n    <blockquote><p>pre\n</p></blockquote>\n  </li>\n</ul>",
        ),
        (
            "- > pre\n {% b\n  c\n b %}\n",
            "<ul>\n  <li>\n    <blockquote><p>pre\n{% b\nc\nb %}</p></blockquote>\n  </li>\n</ul>",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(to_html(source).trim(), expected, "{source:?}");
    }
}

#[test]
fn a_lazy_braced_comment_uses_written_columns_in_a_list() {
    let cases = [
        (
            "- pre\n{% b\nb %}\n",
            "<ul>\n  <li>pre\n{% b\nb %}</li>\n</ul>",
        ),
        ("- pre\n{% b\n b %}\n", "<ul>\n  <li>pre\n</li>\n</ul>"),
    ];
    for (source, expected) in cases {
        assert_eq!(to_html(source).trim(), expected, "{source:?}");
    }
}

#[test]
fn a_lazy_brace_run_does_not_move_a_sibling_list_marker() {
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>pre\n{% a</li>\n      <li>x</li>\n    </ul>\n  </li>\n</ul>";
    for source in ["- - pre\n {% a\n  - x\n", "- - pre\n{% a\n  - x\n"] {
        assert_eq!(to_html(source).trim(), expected, "{source:?}");
    }
}

#[test]
fn a_lazy_braced_comment_uses_written_columns_in_a_footnote_and_div() {
    let cases = [
        ("[^n]: pre\n{% b\nb %}\n\nx[^n]\n", true),
        ("[^n]: pre\n{% b\n b %}\n\nx[^n]\n", false),
        ("[^n]: pre\n{% b\n  b %}\n\nx[^n]\n", false),
        ("::: note\npre\n{% b\nb %}\n:::\n", true),
        ("::: note\npre\n{% b\n b %}\n:::\n", false),
    ];
    for (source, literal) in cases {
        assert_eq!(to_html(source).contains("{% b"), literal, "{source:?}");
    }
}
