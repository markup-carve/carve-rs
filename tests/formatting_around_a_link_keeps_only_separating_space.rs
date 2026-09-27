use carve::{html_to_carve, HtmlImportOptions};

#[test]
fn formatting_around_a_link_keeps_only_separating_space() {
    for (name, html, expected) in [
        (
            "block edges",
            "<p><strong>\n <a href=\"/x\">mk</a>\n </strong></p>",
            "*[mk](/x)*\n",
        ),
        (
            "spaces outside too",
            "<p>a <strong>\n <a href=\"/x\">mk</a>\n </strong> b</p>",
            "a *[mk](/x)* b\n",
        ),
        (
            "only inner separators",
            "<p>a<strong> <a href=\"/x\">mk</a> </strong>b</p>",
            "a{* [mk](/x) *}b\n",
        ),
        (
            "leading redundancy",
            "<p>a <strong> <a href=\"/x\">mk</a> </strong>b</p>",
            "a {*[mk](/x) *}b\n",
        ),
        (
            "trailing redundancy",
            "<p>a<strong> <a href=\"/x\">mk</a> </strong> b</p>",
            "a{* [mk](/x)*} b\n",
        ),
        (
            "nested formatting",
            "<p>a <strong> <em> <a href=\"/x\">mk</a> </em> </strong> b</p>",
            "a */[mk](/x)/* b\n",
        ),
        (
            "outer paragraph padding",
            "<p> <strong> <a href=\"/x\">mk</a> </strong> </p>",
            "*[mk](/x)*\n",
        ),
        (
            "whitespace-only formatting",
            "<p>a<strong> </strong>b</p>",
            "a{* *}b\n",
        ),
        (
            "code content",
            "<p>a <strong> <code> x </code> </strong> b</p>",
            "a *`  x  `* b\n",
        ),
        (
            "formatting without a link",
            "<p>a <strong> x </strong> b</p>",
            "a *x* b\n",
        ),
        (
            "superscript",
            "<p>a <sup> <a href=\"/x\">mk</a> </sup> b</p>",
            "a {^[mk](/x)^} b\n",
        ),
        (
            "hard break",
            "<p>a<br><strong> <a href=\"/x\">mk</a> </strong>b</p>",
            "a\\\n{*[mk](/x) *}b\n",
        ),
    ] {
        let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
        assert_eq!(result.value, expected, "{name}");
        assert!(result.report.diagnostics.is_empty(), "{name}");
    }
}
