// carve#4799 (superseding option C) and carve#4890 (option B): a paragraph
// holding only braced comments renders nothing and reads as a `%%` line.

#[test]
fn renders_nothing_and_hosts_no_caption() {
    let cases = [
        ("{% c %}\n", ""),
        ("{% c %}\n{% d %}\n", ""),
        ("> {% c %}\n", "<blockquote>\n\n</blockquote>"),
        (
            "> {% c %}\n^ cap\n",
            "<blockquote>\n\n</blockquote>\n<p>^ cap</p>",
        ),
        ("{#a}\n{% c %}\n\np\n", "<p id=\"a\">p</p>"),
        (
            "- a\n- {% c %}\n- b\n",
            "<ul>\n  <li>a</li>\n  <li></li>\n  <li>b</li>\n</ul>",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(carve::to_html(source).trim_end(), expected, "{source:?}");
    }
}

#[test]
fn lists_skip_it_between_siblings() {
    let cases = [
        (
            "- a\n\n{% c %}\n\n- b\n",
            "<ul>\n  <li><p>a</p></li>\n  <li><p>b</p></li>\n</ul>",
        ),
        (
            "- a\n\n{% c\nd %}\n\n- b\n",
            "<ul>\n  <li><p>a</p></li>\n  <li><p>b</p></li>\n</ul>",
        ),
        (
            "- p\n\n  - a\n\n  {% c %}\n  - b\n",
            "<ul>\n  <li>p\n    <ul>\n      <li><p>a</p></li>\n      <li><p>b</p></li>\n    </ul>\n  </li>\n</ul>",
        ),
        ("- {% c\n  d %}\n\n  Body.\n", "<ul>\n  <li>Body.</li>\n</ul>"),
    ];
    for (source, expected) in cases {
        assert_eq!(carve::to_html(source).trim_end(), expected, "{source:?}");
    }
}

#[test]
fn a_following_text_line_keeps_the_paragraph() {
    assert_eq!(carve::to_html("{% c %}\nx\n").trim_end(), "<p>\nx</p>");
    // A hard break is content.
    assert_eq!(carve::to_html("{% c %}\\\n").trim_end(), "<p><br>\n</p>");
}

#[test]
fn a_comment_between_siblings_keeps_its_position() {
    let json = carve::to_json_with_options(
        "- x\n\n{% a\nb %}\n\n- y\n",
        &carve::Options::default().with_positions(true),
    );
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let comment = &value["children"][0]["items"][0]["children"][1];
    assert_eq!(comment["type"], "comment");
    assert_eq!(comment["pos"]["startLine"], 3);
    assert_eq!(comment["pos"]["endLine"], 4);
}
