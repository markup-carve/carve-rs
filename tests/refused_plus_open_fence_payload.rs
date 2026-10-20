use carve::to_html;

#[test]
fn refused_markers_leave_blank_fence_payload() {
    let cases = [
        ("- ```\n+\n\ntail", "<ul>\n  <li>\n    <pre><code>\n</code></pre>\n  </li>\n</ul>\n<p>tail</p>"),
        ("- ```\n  x\n+\n\ntail", "<ul>\n  <li>\n    <pre><code>x\n\n</code></pre>\n  </li>\n</ul>\n<p>tail</p>"),
        ("- - ```\n  +\n\ntail", "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code>\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>tail</p>"),
        ("- - ```\n    x\n  +\n\ntail", "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code>x\n\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>tail</p>"),
    ];
    for (source, expected) in cases {
        assert_eq!(to_html(source), expected, "{source:?}");
    }
}

#[test]
fn refused_markers_match_deletion_at_wider_columns() {
    let cases = [
        ("- - - ```\n    +\n\ntail", "- - - ```\n\ntail"),
        ("10. 10. ```\n    +\n\ntail", "10. 10. ```\n\ntail"),
        (
            "- [ ] - [ ] ```\n      +\n\ntail",
            "- [ ] - [ ] ```\n\ntail",
        ),
        (
            "- [ ] - [ ] - [ ] ```\n            +\n\n",
            "- [ ] - [ ] - [ ] ```\n\n",
        ),
        ("- - ```=html\n  +\n\ntail", "- - ```=html\n\ntail"),
    ];
    for (source, deleted) in cases {
        assert_eq!(to_html(source), to_html(deleted), "{source:?}");
    }
}
